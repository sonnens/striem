use std::net::SocketAddr;

use serde::Deserialize;

use striem_common::prelude::*;

use crate::HostConfig;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Listener {
    Vector(HostConfig),
    Http(HostConfig),
}

impl Listener {
    pub fn reconcile(&mut self) {
        match self {
            Listener::Vector(cfg) => {
                cfg.reconcile();
            }
            Listener::Http(cfg) => {
                cfg.reconcile();
            }
        }
    }
    pub fn url(&self) -> String {
        match self {
            Listener::Vector(cfg) => cfg.url.to_string(),
            Listener::Http(cfg) => cfg.url.to_string(),
        }
    }
    pub fn address(&self) -> SocketAddr {
        match self {
            Listener::Vector(cfg) => cfg.address,
            Listener::Http(cfg) => cfg.address,
        }
    }
}

impl Default for Listener {
    fn default() -> Self {
        Listener::Vector(HostConfig::default().set_port(DEFAULT_VECTOR_LISTEN_PORT))
    }
}
