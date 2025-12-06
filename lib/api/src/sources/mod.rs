mod aws_cloudtrail;
mod okta;
use std::collections::BTreeMap;

use axum::{Router, extract::State};
use erased_serde as es;
use serde::{Serialize, ser::SerializeMap};

use tokio::sync::RwLock;

use std::sync::LazyLock;

use crate::ApiState;

pub(crate) static SOURCES: LazyLock<RwLock<Vec<Box<dyn Source>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

#[derive(Serialize, Clone, Default)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum Decoding {
    #[default]
    Json,
}
#[derive(Serialize, Clone, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformType {
    #[default]
    Remap,
}

#[derive(Serialize, Default)]
pub struct Transform {
    #[serde(flatten)]
    _type: TransformType,
    inputs: Vec<String>,
    source: Option<String>,
    file: Option<String>,
}

/// A data source in StrIEM is defines it's own Sigma taxonomy
/// classification, it's Vector `source` configuration, and any
/// `transform` configurations needed for preprocessing.
///
/// It is serialized to a Vector configuration as
/// source-{sourcetype}_{id} in the `sources` section,
/// with transforms to insert the Sigma taxonomy (as a metadata field)
/// and OCSF normalization as logsource-{sourcetype}_{id}
/// and ocsf-{sourcetype}_{id}
pub trait Source: Send + Sync {
    fn id(&self) -> String;

    /// the Vector source type
    fn sourcetype(&self) -> String;

    /// A human friendly identifier
    fn friendly_id(&self) -> String {
        self.sourcetype()
    }

    /// Sigma taxonomy fields
    fn logsource_vendor(&self) -> Option<String> {
        None
    }
    fn logsource_product(&self) -> Option<String> {
        None
    }
    fn logsource_service(&self) -> Option<String> {
        None
    }

    /// Vector source configuration
    fn config(&self) -> &dyn es::Serialize;

    fn preprocess_transforms(&self) -> Option<(BTreeMap<String, Transform>, String)> {
        None
    }
}

impl Serialize for dyn Source {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let source_id = format!("source-{}_{}", self.sourcetype(), self.id());
        let logsource_id = format!("logsource-{}_{}", self.sourcetype(), self.id());
        let ocsf_id = format!("ocsf-{}_{}", self.sourcetype(), self.id());

        let mut logsource = BTreeMap::new();

        if let Some(vendor) = self.logsource_vendor() {
            logsource.insert("vendor".to_string(), vendor);
        }
        if let Some(product) = self.logsource_product() {
            logsource.insert("product".to_string(), product);
        }
        if let Some(service) = self.logsource_service() {
            logsource.insert("service".to_string(), service);
        }

        let sigma = format!("%sigma = {}", serde_json::json!({"logsource": logsource}));

        let mut map = serializer.serialize_map(Some(2))?;

        map.serialize_entry(
            "sources",
            &BTreeMap::from([(source_id.clone(), &self.config())]),
        )?;

        let (mut transforms, final_id) = match self.preprocess_transforms() {
            Some((transforms, final_id)) => (transforms, final_id),
            None => (BTreeMap::new(), source_id.clone()),
        };

        // This workaround is until Vector supports environment variable interpolation
        // in HTTP provider configuration
        let remaps_dir = if let Some(dir) = std::env::var("STRIEM_REMAPS").ok() {
            dir
        } else {
            "${STRIEM_REMAPS}".to_string()
        };

        // adds the Sigma taxonomy metadata, and OCSF remap transform
        transforms.extend([
            (
                logsource_id.clone(),
                Transform {
                    inputs: vec![final_id],
                    source: Some(format!("%source_id = \"{}\"\n{}\n", source_id, sigma)),
                    file: None,
                    ..Default::default()
                },
            ),
            (
                ocsf_id.clone(),
                Transform {
                    inputs: vec![logsource_id],
                    source: None,
                    file: Some(format!("{}/{}/remap.vrl", remaps_dir, self.sourcetype())),
                    ..Default::default()
                },
            ),
        ]);

        let transforms = transforms.into_iter().collect::<BTreeMap<_, _>>();

        map.serialize_entry("transforms", &transforms)?;

        map.end()
    }
}

async fn list_sources(State(_): State<ApiState>) -> axum::Json<Vec<serde_json::Value>> {
    let sources = SOURCES.read().await;

    axum::Json(
        sources
            .iter()
            .map(|source| {
                serde_json::json!({
                    "id": source.id(),
                    "sourcetype": source.sourcetype(),
                    "friendly_id": source.friendly_id(),
                })
            })
            .collect(),
    )
}

async fn get_source(
    State(_): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let sources = SOURCES.read().await;

    let source = sources
        .iter()
        .find(|source| source.id() == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Source with id {} not found", id),
            )
        })?;

    let source_json = serde_json::to_value(&source)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(source_json))
}

async fn delete_source(
    State(_): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<()>, (axum::http::StatusCode, String)> {
    let mut sources = SOURCES.write().await;

    let index = sources
        .iter()
        .position(|source| source.id() == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Source with id {} not found", id),
            )
        })?;

    sources.remove(index);

    Ok(axum::Json(()))
}

#[allow(dead_code)]
pub fn create_router() -> axum::Router<ApiState> {
    Router::new()
        .route("/", axum::routing::get(list_sources))
        .route(
            "/{id}",
            axum::routing::get(get_source).delete(delete_source),
        )
        .nest("/aws_cloudtrail", aws_cloudtrail::create_router())
        .nest("/okta", okta::create_router())
}
