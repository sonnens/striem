mod actions;
mod alerts;
mod detections;
mod routes;
mod server;
mod sinks;
mod sources;
mod vector;

pub use server::serve;

use actions::Mcp;
use sigmars::SigmaCollection;
use tokio::sync::RwLock;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct ApiState {
    pub detections: Arc<RwLock<SigmaCollection>>,
    pub actions: Option<Arc<Mcp>>,
    pub data: Option<String>,
    pub db: Option<r2d2::Pool<duckdb::DuckdbConnectionManager>>,
    pub vector: String,
    pub fqdn: String,
}
