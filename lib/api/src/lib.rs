mod actions;
mod alerts;
mod detections;
pub mod features;
mod persist;
mod routes;
mod server;
mod sinks;
mod sources;
mod vector;

use axum::http::HeaderValue;
pub use server::serve;

use std::sync::Arc;
use tokio::sync::RwLock;

use sigmars::SigmaCollection;
use striem_config::StrIEMConfig;

use actions::Mcp;

#[cfg(feature = "duckdb")]
pub(crate) type Pool = r2d2::Pool<duckdb::DuckdbConnectionManager>;
#[cfg(all(feature = "sqlite", not(feature = "duckdb")))]
pub(crate) type Pool = r2d2::Pool<sqlite::SqliteConnectionManager>;
#[cfg(not(any(feature = "duckdb", feature = "sqlite")))]
pub(crate) type Pool = ();

#[derive(Clone)]
pub(crate) struct ApiState {
    pub detections: Arc<RwLock<SigmaCollection>>,
    pub actions: Option<Arc<Mcp>>,
    pub data: Option<String>,
    pub db: Option<Pool>,
    pub features: HeaderValue,
    pub config: StrIEMConfig,
}

#[cfg(feature = "duckdb")]
pub(crate) fn initdb(config: &StrIEMConfig) -> Option<Pool> {
    // Create DuckDB connection pool with metadata caching enabled
    // Metadata cache significantly improves query performance on large Parquet datasets
    // by avoiding repeated schema reads
    if let Some(ref data_dir) = config.db {
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
            .and_then(|pool| {
                let mut conn = pool.get().map_err(anyhow::Error::from)?;
                crate::persist::init(&mut conn)?;
                Ok(pool)
            })
            .ok()
    } else if let Some(_) = &config.storage {
        duckdb::DuckdbConnectionManager::memory_with_flags(
            duckdb::Config::default()
                .with("parquet_metadata_cache", "true")
                .ok()?,
        )
        .map_err(anyhow::Error::from)
        .and_then(|db| Ok(r2d2::Pool::builder().build(db)?))
        .map_err(anyhow::Error::from)
        .ok()
    } else {
        None
    }
}

#[cfg(all(feature = "sqlite", not(feature = "duckdb")))]
pub(crate) fn db_pool(config: &StrIEMConfig) -> Option<Pool> {
    unimplemented!("SQLite support is not yet implemented");
    None
}

#[cfg(not(any(feature = "duckdb", feature = "sqlite")))]
pub(crate) fn db_pool(_config: &StrIEMConfig) -> Option<Pool> {
    None
}
