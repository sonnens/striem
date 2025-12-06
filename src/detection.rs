use anyhow::Result;

use log::{error, info, trace};
use serde_json::{Value, json};
use sigmars::SigmaCollection;
use striem_common::event::Event;

use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

pub(crate) struct DetectionHandler {
    src: broadcast::Receiver<Arc<Vec<Event>>>,
    dest: broadcast::Sender<Arc<Vec<Event>>>,
    rules: Arc<RwLock<SigmaCollection>>,
    shutdown: broadcast::Receiver<()>,
}

impl DetectionHandler {
    pub(crate) fn new(
        src: broadcast::Receiver<Arc<Vec<Event>>>,
        dest: broadcast::Sender<Arc<Vec<Event>>>,
        rules: Arc<RwLock<SigmaCollection>>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            src,
            dest,
            rules,
            shutdown,
        }
    }

    pub(crate) async fn run(&mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    info!("Detection worker shutting down...");
                    return;
                },
                result = self.src.recv() => {
                    if let Ok(events) = result {
                        for event in events.iter() {
                            if let Err(e) = self.apply(event).await {
                                error!("error applying detection rules: {}", e);
                            }
                        }
                    } else {
                        info!("source channel closed");
                        return;
                    }
                }
            }
        }
    }

    async fn apply(&self, event: &Event) -> Result<()> {
        let filter = event
            .metadata
            .get("logsource")
            .map(|v| sigmars::event::LogSource::from(v.clone()))
            .unwrap_or_default();

        let raw_data = event
            .metadata
            .get("ocsf")
            .and_then(|_| match event.data.get("raw_data") {
                Some(Value::String(raw_data)) => serde_json::from_str::<Value>(raw_data).ok(),
                _ => None,
            });

        let data = match raw_data {
            Some(ref d) => d,
            None => &event.data,
        };

        let sigma_event = sigmars::event::RefEvent {
            data,
            metadata: &event.metadata,
            logsource: filter,
        };

        let rules = self.rules.read().await;

        let detections = rules
            .get_matches_from_ref(&sigma_event)
            .await
            .map_err(|e| anyhow::anyhow!("error applying rules: {}", e))?
            .iter()
            .filter_map(|d| rules.get(d))
            .filter_map(|d| {
                let correlation_uid = event
                    .data
                    .as_object()
                    .and_then(|v| v.get("metadata"))
                    .and_then(|v| v.as_object())
                    .and_then(|v| v.get("uid"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| event.id.to_string());

                let mut ocsf = Event::default();

                let mut data: Value = d.into();
                data["metadata"]["uid"] = json!(event.id.to_string());
                data["metadata"]["correlation_uid"] = json!(correlation_uid);
                data["metadata"]["product"] = json!({
                    "vendor_name": "StrIEM",
                    "product_name": "StrIEM"
                });
                ocsf.data = data;
                ocsf.metadata
                    .extend(event.metadata.iter().map(|(k, v)| (k.clone(), v.clone())));
                ocsf.metadata.extend([
                    ("ocsf".to_string(), json!(true)),
                    ("striem".to_string(), json!(true)),
                ]);
                Some(ocsf)
            })
            .collect::<Vec<_>>();
        drop(rules);

        if !detections.is_empty() {
            trace!("event {} matched {} detections", event.id, detections.len());
        }
        let _ = self.dest.send(Arc::new(detections));
        Ok(())
    }
}
