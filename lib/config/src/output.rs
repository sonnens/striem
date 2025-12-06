use serde::Deserialize;

use striem_common::prelude::*;

use crate::HostConfig;

#[derive(Debug, Deserialize, Clone)]
pub struct VectorDestinationConfig {
    #[serde(flatten)]
    pub cfg: HostConfig,
    pub hec: Option<HostConfig>,
    pub http: Option<HostConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    Vector(VectorDestinationConfig),
    Http(HostConfig),
}

impl Destination {
    pub fn reconcile(&mut self) {
        match self {
            Destination::Vector(v) => v.cfg.reconcile(),
            Destination::Http(cfg) => cfg.reconcile(),
        }
    }
}

impl Default for Destination {
    fn default() -> Self {
        Destination::Vector(VectorDestinationConfig {
            cfg: HostConfig::default().set_port(DEFAULT_VECTOR_LISTEN_PORT),
            hec: None,
            http: None,
        })
    }
}
