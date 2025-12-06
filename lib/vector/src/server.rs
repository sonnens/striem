use std::sync::Arc;

use anyhow::{Result, anyhow};
use log::{debug, info};
use striem_common::event::Event;
use tokio::sync::broadcast;

use crate::{
    event::event_wrapper::Event as VectorEventWrapper,
    vector::{
        self,
        vector_server::{Vector, VectorServer},
    },
};

struct VectorService {
    channel: broadcast::Sender<Arc<Vec<Event>>>,
}

#[tonic::async_trait]
impl Vector for VectorService {
    async fn push_events(
        &self,
        request: tonic::Request<vector::PushEventsRequest>,
    ) -> Result<tonic::Response<vector::PushEventsResponse>, tonic::Status> {
        let events = request
            .into_inner()
            .events
            .iter_mut()
            .map(|wrapped| {
                let event = wrapped
                    .event
                    .take()
                    .ok_or_else(|| tonic::Status::invalid_argument("missing event"))?;
                match event {
                    VectorEventWrapper::Log(e) => {
                        debug!("received log event: {:?}", e);
                        Ok(e.into())
                    }
                    _ => Err(tonic::Status::unimplemented(
                        "only log events are supported by this server",
                    )),
                }
            })
            .collect::<Result<Vec<Event>, tonic::Status>>()?;

        let events = Arc::new(events);

        self.channel
            .send(events)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(vector::PushEventsResponse {}))
    }

    async fn health_check(
        &self,
        _: tonic::Request<vector::HealthCheckRequest>,
    ) -> Result<tonic::Response<vector::HealthCheckResponse>, tonic::Status> {
        Ok(tonic::Response::new(vector::HealthCheckResponse {
            status: vector::ServingStatus::Serving.into(),
        }))
    }
}

pub struct Server {
    service: Option<VectorService>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        Self {
            service: Some(VectorService {
                channel: broadcast::channel(256).0,
            }),
        }
    }

    pub async fn serve(
        &mut self,
        addr: &std::net::SocketAddr,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        //let addr = addr.parse()?;

        let service = self
            .service
            .take()
            .ok_or_else(|| anyhow!("service already running"))?;

        tonic::transport::Server::builder()
            .add_service(
                VectorServer::new(service)
                    .accept_compressed(tonic::codec::CompressionEncoding::Gzip),
            )
            .serve_with_shutdown(*addr, async {
                let _ = shutdown.recv().await;
                info!("Vector listener shutting down...");
            })
            .await?;
        Ok(())
    }

    pub async fn subscribe(&self) -> Result<broadcast::Receiver<Arc<Vec<Event>>>> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow!("service not running"))?;
        Ok(service.channel.subscribe())
    }
}
