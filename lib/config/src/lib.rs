//! Configuration management for StrIEM.
//!
//! Supports loading from:
//! - YAML configuration files
//! - Environment variables (STRIEM_ prefix)
//! - Defaults
//!
//! Environment variables override file settings, enabling Docker/K8s deployments
//! without rebuilding config files.

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

/// Configuration value that accepts either a single string or array of strings.
/// Enables flexible YAML syntax: `detections: ./rules` or `detections: [./rules1, ./rules2]`
#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrList {
    String(String),
    List(Vec<String>),
}

/// Host configuration with address/URL reconciliation.
///
/// # Reconciliation Logic
/// Automatically synchronizes port between SocketAddr and URL.
/// If URL is localhost/127.0.0.1, port is updated to match SocketAddr.
/// This allows "address: 0.0.0.0:8080" to imply "url: http://localhost:8080".
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

    /// Synchronize port between address and URL for localhost/loopback addresses.
    ///
    /// # Use Case
    /// When user specifies only `address: 0.0.0.0:3000`, infer that
    /// `url: http://localhost:3000` for generating Vector configs.
    ///
    /// Only updates URL port if host is localhost/127.0.0.1 to avoid
    /// breaking external/cluster URLs.
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

const PWD: fn() -> String = || std::env::current_dir().and_then(|p| Ok(p.to_string_lossy().into()))
    .unwrap_or_else(|_| ".".into());

#[derive(Debug, Deserialize, Default, Clone)]
struct StrIEMConfigOptions {

    /// Path to the StrIEM source configuration & rule database
    /// (defaults to current working directory)
    #[serde(default = "PWD")]
    db: String,

    /// Location of top-level Sigma detection directory
    /// (can be a list or single path)
    #[serde(with = "serde_yaml::with::singleton_map")]
    detections: Option<StringOrList>,

    /// Input listener configuration
    #[serde(with = "serde_yaml::with::singleton_map")]
    input: Option<input::Listener>,

    /// Output destination configuration
    #[serde(with = "serde_yaml::with::singleton_map")]
    output: Option<output::Destination>,

    /// Storage backend configuration
    #[serde(with = "serde_yaml::with::singleton_map")]
    storage: Option<storage::StorageConfig>,

    /// API server configuration
    api: Option<api::ApiConfig>,

    /// Fully qualified domain name for this StrIEM instance
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
