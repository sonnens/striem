//! HTTP API server for StrIEM management interface.
//!
//! Provides REST endpoints for:
//! - Source management (add/remove data sources)
//! - Detection rule management (list/enable/disable/upload)
//! - Data querying (DuckDB SQL queries on Parquet files)
//! - Vector configuration generation
//!
//! # Architecture
//! - Axum for HTTP routing and middleware
//! - Tower HTTP for CORS and static file serving
//! - DuckDB connection pool for query execution
//! - Shared state (Arc) for detection rules and configuration

use std::sync::Arc;

use anyhow::Result;
use axum::middleware;
use log::info;
use sigmars::SigmaCollection;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use striem_common::prelude::*;
use striem_config::StrIEMConfig;
use striem_config::StringOrList;

use crate::{actions::Mcp,
    db_pool,
    ApiState,
    features::{feature_flag_middleware, FEATURE_FLAGS},
    routes::create_router,
    sources::SOURCES,
    persistence as persist
};

/// Initialize and run the API server.
///
/// # Database Initialization
/// Creates DuckDB connection pool if storage is configured.
/// Uses file-backed DB if data_dir specified, otherwise in-memory.
/// Enables parquet_metadata_cache for faster queries on large datasets.
///
/// # UI Serving
/// Serves Next.js static export from binary path or configured ui.path.
/// Redirects / to /ui for convenience.
pub async fn serve(
    config: &StrIEMConfig,
    detections: Arc<RwLock<SigmaCollection>>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let data = if let Some(dir) = &config.storage {
        Some(dir.path.clone())
    } else {
        None
    };

    // Create DB connection pool
    let db = db_pool(config)
    .inspect(|_| {
        FEATURE_FLAGS.write().map(|mut flags| {
            if !flags.contains(&"persistence".to_string()) {
                flags.push("persistence".to_string());
            }
        }).ok();
    });

    if let Some(db) = db.as_ref() {
        // Initialize persistence layer
        let mut conn = db.get()?;
        persist::init(&mut conn)?;
        let mut sources = SOURCES.write().await;
        sources.append(&mut persist::get_all_sources(&mut conn)?);
    };

    let actions = if let Some(mcp_config) = &config.api.mcp {
        match &mcp_config.url {
            StringOrList::String(url) => Some(Arc::new(Mcp::new(url.clone()))),
            StringOrList::List(urls) if !urls.is_empty() => {
                Some(Arc::new(Mcp::new(urls[0].clone())))
            }
            _ => None,
        }
    } else {
        None
    }
    .inspect(|_| {
        FEATURE_FLAGS.write().map(|mut flags| {
            if !flags.contains(&"mcp".to_string()) {
                flags.push("mcp".to_string());
            }
        }).ok();
    });

    let fqdn = match config.fqdn.as_ref() {
        Some(fqdn) => fqdn.clone(),
        None => config.input.url(),
    };

    let vector = config
        .output
        .as_ref()
        .map(|o| match o {
            striem_config::output::Destination::Vector(v) => v.cfg.address.to_string(),
            _ => "".to_string(),
        })
        .unwrap_or_else(|| format!("0.0.0.0:{}", DEFAULT_VECTOR_LISTEN_PORT));

    let ui = config
        .api
        .ui
        .as_ref()
        .and_then(|ui| if ui.enabled { ui.path.clone() } else { None })
        .map(std::path::PathBuf::from)
        // Fallback: look for 'ui' directory next to binary (production deployment)
        // This supports cargo build integration where UI is copied to target/ui
        .or_else(|| {
            std::env::current_exe()
                .map_err(anyhow::Error::from)
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("ui"))
        })
        .filter(|p| p.exists());

    
    let mut app = create_router()
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(feature_flag_middleware))
        .with_state(ApiState {
            detections,
            actions,
            data,
            db,
            vector,
            fqdn,
        });

    if let Some(path) = ui {
        app = app
            .nest_service(
                "/ui",
                ServeDir::new(path).append_index_html_on_directories(true),
            )
            .route(
                "/",
                axum::routing::get(|| async { axum::response::Redirect::to("/ui") }),
            );
    }
    let listener = tokio::net::TcpListener::bind(&config.api.address).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
            info!("API shutting down...");
        })
        .await?;
    Ok(())
}
