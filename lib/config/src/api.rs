use serde::Deserialize;

use crate::StringOrList;
use striem_common::prelude::*;

const TRUE: fn() -> bool = || true;

fn default_address() -> String {
    format!("127.0.0.1:{}", DEFAULT_API_LISTEN_PORT)
}

#[derive(Debug, Deserialize, Clone)]
pub struct MCPConfig {
    pub url: StringOrList,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct UIConfig {
    #[serde(default = "TRUE")]
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ApiConfig {
    #[serde(default = "TRUE")]
    pub enabled: bool,
    #[serde(default = "default_address")]
    pub address: String,
    pub data: Option<String>,
    pub mcp: Option<MCPConfig>,
    pub ui: Option<UIConfig>,
}
