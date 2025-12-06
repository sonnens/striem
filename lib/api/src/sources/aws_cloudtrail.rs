use axum::{
    Router,
    extract::{self, State},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use erased_serde as es;
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

use crate::ApiState;

use super::{Decoding, SOURCES, Source, Transform};

#[derive(Serialize, Deserialize)]
pub struct ImdsAuthentication {
    max_attempts: u32,
    connect_timeout: Duration,
    read_timeout: Duration,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum AwsAuthentication {
    AccessKey {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        assume_role: Option<String>,
        external_id: Option<String>,
        region: Option<String>,
        session_name: Option<String>,
    },
    File {
        credentials_file: String,
        profile: String,
        region: Option<String>,
    },
    Role {
        assume_role: String,
        external_id: Option<String>,
        imds: ImdsAuthentication,
        region: Option<String>,
        session_name: Option<String>,
    },
    Default {
        imds: Option<ImdsAuthentication>,
        region: Option<String>,
    },
}
impl Default for AwsAuthentication {
    fn default() -> Self {
        AwsAuthentication::Default {
            imds: None,
            region: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct SqsConfig {
    queue_url: String,
}

#[derive(Serialize, Default)]
pub struct AwsCloudtrailConfig {
    #[serde(rename = "type")]
    _type: String,
    pub auth: Option<AwsAuthentication>,
    pub sqs: SqsConfig,
    pub region: Option<String>,
    #[serde(default)]
    pub decoding: Decoding,
}

impl<'de> Deserialize<'de> for AwsCloudtrailConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct AwsCloudtrailConfigHelper {
            #[serde(default)]
            pub auth: Option<AwsAuthentication>,
            pub sqs: SqsConfig,
            pub region: Option<String>,
        }

        let helper = AwsCloudtrailConfigHelper::deserialize(deserializer)?;
        Ok(AwsCloudtrailConfig {
            _type: "aws_s3".to_string(),
            auth: helper.auth,
            sqs: helper.sqs,
            region: helper.region,
            ..Default::default()
        })
    }
}

pub struct AwsCloudtrail {
    id: String,
    config: AwsCloudtrailConfig,
}

impl Source for AwsCloudtrail {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn friendly_id(&self) -> String {
        self.config.sqs.queue_url.clone()
    }

    fn sourcetype(&self) -> String {
        "aws_cloudtrail".to_string()
    }

    fn config(&self) -> &dyn es::Serialize {
        &self.config
    }

    fn logsource_product(&self) -> Option<String> {
        Some("aws".to_string())
    }

    fn logsource_service(&self) -> Option<String> {
        Some("cloudtrail".to_string())
    }

    fn preprocess_transforms(&self) -> Option<(BTreeMap<String, Transform>, String)> {
        let source_id = format!("source-{}_{}", self.sourcetype(), self.id());
        let pre_id = format!("pre-{}_{}", self.sourcetype(), self.id());

        let transforms = BTreeMap::from([(
            pre_id.clone(),
            Transform {
                inputs: vec![source_id.clone()],
                source: Some(". = .Records".to_string()),
                file: None,
                ..Default::default()
            },
        )]);
        Some((transforms, pre_id))
    }
}

#[allow(dead_code)]
async fn post_aws_cloudtrail_config(
    State(_state): State<ApiState>,
    config: extract::Json<AwsCloudtrailConfig>,
) -> Result<axum::Json<Value>, axum::response::ErrorResponse> {
    let id = Uuid::now_v7();

    let aws_cloudtrail: Box<dyn Source> = Box::new(AwsCloudtrail {
        id: id.to_string(),
        config: config.0,
    });

    let mut sources = SOURCES.write().await;
    sources.push(aws_cloudtrail);

    Ok(axum::Json::from(json!({id.to_string(): "aws_cloudtrail"})))
}

#[allow(dead_code)]
pub fn create_router() -> axum::Router<ApiState> {
    Router::new().route("/", post(post_aws_cloudtrail_config))
}
