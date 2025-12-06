use axum::{Router, extract, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ApiState;

use super::{SOURCES, Source};

use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct OktaConfig {
    #[serde(rename = "type")]
    _type: String,
    pub domain: String,
    pub token: String,
    pub scrape_interval_secs: Option<u64>,
    pub scrape_timeout_secs: Option<u64>,
    pub since: Option<u64>,
}

impl<'de> Deserialize<'de> for OktaConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OktaConfigHelper {
            pub domain: String,
            pub token: String,
            pub scrape_interval_secs: Option<u64>,
            pub scrape_timeout_secs: Option<u64>,
            pub since: Option<u64>,
        }

        let helper = OktaConfigHelper::deserialize(deserializer)?;
        Ok(OktaConfig {
            _type: "okta".into(),
            domain: helper.domain,
            token: helper.token,
            scrape_interval_secs: helper.scrape_interval_secs,
            scrape_timeout_secs: helper.scrape_timeout_secs,
            since: helper.since,
        })
    }
}

pub struct Okta {
    id: String,
    config: OktaConfig,
}

impl Source for Okta {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn friendly_id(&self) -> String {
        self.config.domain.clone()
    }

    fn sourcetype(&self) -> String {
        "okta".to_string()
    }

    fn config(&self) -> &dyn erased_serde::Serialize {
        &self.config
    }

    fn logsource_vendor(&self) -> Option<String> {
        Some("okta".to_string())
    }

    fn logsource_product(&self) -> Option<String> {
        Some("audit".to_string())
    }
}

async fn post_okta_config(
    config: extract::Json<OktaConfig>,
) -> Result<axum::Json<Value>, axum::response::ErrorResponse> {
    let id = Uuid::now_v7();

    let okta: Box<dyn Source> = Box::new(Okta {
        id: id.to_string(),
        config: config.0,
    });

    let mut sources = SOURCES.write().await;
    sources.push(okta);
    drop(sources);

    Ok(axum::Json::from(json!({id.to_string(): "okta"})))
}

pub fn create_router() -> axum::Router<ApiState> {
    Router::new().route("/", post(post_okta_config))
}
