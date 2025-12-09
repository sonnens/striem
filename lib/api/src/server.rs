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

use crate::actions::Mcp;
use crate::{ApiState, routes::create_router};
use anyhow::Result;
use log::info;
use sigmars::SigmaCollection;
use tokio::sync::RwLock;
use std::sync::Arc;
use striem_common::prelude::*;
use striem_config::StrIEMConfig;
use striem_config::StringOrList;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

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

    let data_dir = config
        .api
        .data
        .as_ref()
        .or_else(|| config.storage.as_ref().map(|s| &s.path));

    // Create DuckDB connection pool with metadata caching enabled
    // Metadata cache significantly improves query performance on large Parquet datasets
    // by avoiding repeated schema reads
    let db = if let Some(data_dir) = data_dir {
        std::fs::create_dir_all(data_dir)
            .map_err(anyhow::Error::from)
            .and_then(|_| {
                let path = format!("{}/striem.db", data_dir);
                duckdb::DuckdbConnectionManager::file_with_flags(
                    path.as_str(),
                    duckdb::Config::default().with("parquet_metadata_cache", "true")?,
                )
                .map_err(anyhow::Error::from)
                .and_then(|db| Ok(r2d2::Pool::builder().build(db)?))
                .map_err(anyhow::Error::from)
            })
            .ok()
    } else if let Some(_) = &config.storage {
        duckdb::DuckdbConnectionManager::memory_with_flags(
            duckdb::Config::default().with("parquet_metadata_cache", "true")?,
        )
        .map_err(anyhow::Error::from)
        .and_then(|db| Ok(r2d2::Pool::builder().build(db)?))
        .map_err(anyhow::Error::from)
        .ok()
    } else {
        None
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
    };

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
