use anyhow::{Result, anyhow};
use backoff::{ExponentialBackoff, future::retry};
use log::warn;
use std::sync::Arc;
use striem_common::event::Event;
use striem_config::output::Destination;
use tokio::sync::{RwLock, broadcast};

use log::{debug, info};

use sigmars::{MemBackend, SigmaCollection};

use striem_config::{self as config, StrIEMConfig, StringOrList, input::Listener};

use striem_api as api;
use striem_storage as storage;
use striem_vector::{Client as VectorClient, Server as VectorServer};

use crate::detection::DetectionHandler;

pub struct App {
    pub detections: Arc<RwLock<SigmaCollection>>,
    pub config: StrIEMConfig,
    server: VectorServer,
    channel: broadcast::Sender<Arc<Vec<Event>>>,
    shutdown: broadcast::Sender<()>,
}

impl App {
    pub async fn new(config: StrIEMConfig) -> Result<Self> {
        let shutdown = broadcast::channel::<()>(1).0;
        let channel = broadcast::channel::<Arc<Vec<Event>>>(64).0;

        let server = VectorServer::new();

        let mut detections = SigmaCollection::default();

        if let Some(StringOrList::String(path)) = &config.detections {
            debug!("... loading Sigma detection rules from {}", path);
        } else {
            debug!("... loading detection rules");
        }
        let count = match &config.detections {
            Some(config::StringOrList::String(path)) => detections
                .load_from_dir(path)
                .map_err(|e| anyhow!(e.to_string())),
            Some(config::StringOrList::List(paths)) => paths
                .iter()
                .map(|path| {
                    detections
                        .load_from_dir(path)
                        .map_err(|e| anyhow!(e.to_string()))
                })
                .collect::<Result<Vec<_>>>()
                .map(|r| r.iter().sum()),
            None => {
                warn!("No detection rules loaded");
                Ok(0)
            }
        }?;

        let mut backend = MemBackend::new().await;
        detections.init(&mut backend).await;

        let detections = Arc::new(RwLock::new(detections));

        info!("... loaded {} Sigma detections", count);
        Ok(App {
            detections,
            config,
            server,
            shutdown,
            channel,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        if let Some(ref storage) = self.config.storage {
            info!("... initializing Parquet storage handler");
            self.run_parquet(storage).await?;
        }

        if self.config.detections.is_some() && self.detections.read().await.len() > 0 {
            info!("... initializing detection handler");
            let src = self.server.subscribe().await?;
            let dest = self.channel.clone();
            let mut detection_handler = DetectionHandler::new(
                src,
                dest,
                self.detections.clone(),
                self.shutdown.subscribe(),
            );

            tokio::spawn(async move {
                detection_handler.run().await;
            });
        }

        if self.config.api.enabled {
            info!("... initializing API server and Vector configuration");
            let shutdown = self.shutdown.subscribe();
            let detections = self.detections.clone();
            let config = self.config.clone();
            tokio::spawn(async move {
                api::serve(&config, detections, shutdown)
                    .await
                    .expect("API server failed");
            });
        }

        if let Some(Destination::Vector(ref vector)) = self.config.output {
            info!("... initializing Vector output to {}", vector.cfg.url);
            self.run_vector(vector).await?;
        }

        let shutdown = self.shutdown.subscribe();
        if let Listener::Vector(ref vector) = self.config.input {
            info!("... listening for Vector events on {}", vector.address);
            self.server.serve(&vector.address, shutdown).await?;
        }

        Ok(())
    }

    pub fn shutdown(&self) -> broadcast::Sender<()> {
        self.shutdown.clone()
    }

    async fn run_parquet(&self, config: &striem_config::storage::StorageConfig) -> Result<()> {
        let writer = storage::ParquetBackend::new(&config.schema, &config.path)
            .expect("Failed to create Parquet backend");

        let rx = self.server.subscribe().await?;
        let rx_internal = self.channel.subscribe();
        let shutdown = self.shutdown.subscribe();
        tokio::spawn(async move {
            writer.run(rx, rx_internal, shutdown).await;
        });
        Ok(())
    }
    async fn run_vector(
        &self,
        vector: &striem_config::output::VectorDestinationConfig,
    ) -> Result<()> {
        let url = vector.cfg.url.to_string();
        let rx_internal = self.channel.subscribe();
        let shutdown = self.shutdown.subscribe();
        tokio::spawn(async move {
            let mut sink = retry(ExponentialBackoff::default(), || async {
                VectorClient::new(&url, rx_internal.resubscribe(), shutdown.resubscribe())
                    .await
                    .map_err(|e| {
                        warn!("Failed to connect to Vector at {}: {}", url, e);
                        e.into()
                    })
            })
            .await
            .expect("Failed to connect to Vector client");

            info!("... connected to downstream Vector at {}", url);

            sink.run().await.expect("Vector client failed");
        });
        Ok(())
    }
}
