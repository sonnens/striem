use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use url::Url;

use anyhow::{Result, anyhow};
use config::Config;
use serde::Deserialize;

pub mod api;
pub mod input;
pub mod output;
pub mod storage;

mod tests;

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrList {
    String(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
pub struct HostConfig {
    #[serde(default = "HostConfig::default_address")]
    pub address: SocketAddr,
    #[serde(default = "HostConfig::default_url")]
    pub url: Url,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            address: HostConfig::default_address(),
            url: HostConfig::default_url(),
        }
    }
}

impl HostConfig {
    fn default_address() -> SocketAddr {
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0).into()
    }

    fn default_url() -> Url {
        Url::parse("http://localhost").expect("Invalid default URL")
    }

    pub fn set_port(mut self, port: u16) -> Self {
        self.address.set_port(port);
        self.reconcile();
        self
    }

    pub fn reconcile(&mut self) {
        if self.address.port() == 0 {
            if let Some(port) = self.url.port() {
                self.address.set_port(port);
            }
        } else {
            if let Some(host) = self.url.host_str() {
                if host == "localhost" {
                    self.url
                        .set_port(Some(self.address.port()))
                        .expect("Invalid port");
                } else if let Ok(ip) = host.parse::<SocketAddr>() {
                    if ip.ip().is_loopback() {
                        self.url
                            .set_port(Some(self.address.port()))
                            .expect("Invalid port");
                    }
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
struct StrIEMConfigOptions {
    #[serde(with = "serde_yaml::with::singleton_map")]
    detections: Option<StringOrList>,

    #[serde(with = "serde_yaml::with::singleton_map")]
    input: Option<input::Listener>,

    #[serde(with = "serde_yaml::with::singleton_map")]
    output: Option<output::Destination>,

    #[serde(with = "serde_yaml::with::singleton_map")]
    storage: Option<storage::StorageConfig>,

    api: Option<api::ApiConfig>,

    fqdn: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StrIEMConfig {
    pub detections: Option<StringOrList>,

    pub input: input::Listener,

    pub output: Option<output::Destination>,

    pub storage: Option<storage::StorageConfig>,

    pub api: api::ApiConfig,

    pub fqdn: Option<String>,
}

impl Into<StrIEMConfig> for StrIEMConfigOptions {
    fn into(self) -> StrIEMConfig {
        let mut input = self.input.unwrap_or_default();
        input.reconcile();
        StrIEMConfig {
            detections: self.detections,
            input: input,
            output: self.output.map(|mut o| {
                o.reconcile();
                o
            }),
            storage: self.storage,
            api: self.api.unwrap_or_default(),
            fqdn: self.fqdn,
        }
    }
}

impl StrIEMConfig {
    pub fn new() -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_file(file: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::with_name(file))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_yaml(s: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(s, config::FileFormat::Yaml))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    fn check(config: &StrIEMConfigOptions) -> Result<()> {
        let api = if let Some(ref api) = config.api {
            api.enabled
        } else {
            false
        };
        if !(config.output.is_some() || config.storage.is_some()) {
            if !api {
                Err(anyhow!(
                    "No output, storage, or API configured; StrIEM cannot run"
                ))?
            }
            log::warn!("No output or storage configured; events will be dropped");
        }
        Ok(())
    }
}
