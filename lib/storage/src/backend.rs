use super::writer::Writer;
use super::{ocsf, util::visit_dirs};
use anyhow::{Result, anyhow};
use log::{debug, error, info};
use parquet::arrow::parquet_to_arrow_schema;
use serde_json::Value;
use std::{collections::HashMap, path::Path, sync::Arc};
use striem_common::event::Event;

pub struct ParquetBackend {
    pub heap: HashMap<ocsf::Class, Writer>,
}

impl std::fmt::Debug for ParquetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParquetBackend {{ heap: {:?} }}", self.heap.keys())
    }
}

impl ParquetBackend {
    pub fn new(schema: &String, out: &String) -> Result<Self> {
        let dir = Path::new(&schema);
        let mut heap = HashMap::new();
        for (s, path) in visit_dirs(dir).map_err(|e| anyhow!(e.to_string()))? {
            let arrow_schema = Arc::new(parquet_to_arrow_schema(&s, None)?.with_metadata(
                HashMap::from([
                    (
                        "created_by".to_string(),
                        format!(
                            "StrIEM version {} (build {})",
                            env!("CARGO_PKG_VERSION"),
                            env!("CARGO_GIT_SHA")
                        ),
                    ),
                    ("description".to_string(), s.name().to_string()),
                    (
                        "schema_file".to_string(),
                        path.trim_start_matches(&format!("/{}", schema)).to_string(),
                    ),
                ]),
            ));

            // TODO: path structure could be configurable
            // this structure is to keep schemas organized for DuckDB's sake
            let class: ocsf::Class = s.name().parse().map_err(|e: String| anyhow!(e))?;
            let category = ocsf::Category::try_from((class as u32 % 10000) / 1000)?;
            let outpath = format!("{}/{}/{}", out, category.to_string(), class.to_string());

            let writer = Writer::new(outpath, arrow_schema)?;

            heap.insert(class, writer);
        }

        Ok(Self { heap })
    }

    /// Push a JSON object on to the appropriate parquet writer buffer
    ///
    /// uses the "class_uid" field to determine the appropriate schema
    /// based on the schema's name (ie 'message <name> { ... }')
    pub async fn write(&self, value: &Value) -> Result<()> {
        let writer = value
            .get("class_uid")
            .and_then(|v| v.as_u64())
            .and_then(|v| ocsf::Class::try_from(v as u32).ok())
            .and_then(|k| self.heap.get(&k))
            .ok_or(anyhow::anyhow!("invalid OCSF"))?;

        writer.write(value).await?;

        Ok(())
    }

    async fn process(&self, events: Arc<Vec<Event>>) {
        for event in &*events {
            if let Err(e) = self.write(&event.data).await {
                error!("Failed to write event: {}", e);
            }
        }
    }

    /// Run the backend, creating one task for each ParquetWriter
    /// which represents one OCSF schema class
    pub async fn run(
        mut self,
        mut upstream_rx: tokio::sync::broadcast::Receiver<Arc<Vec<Event>>>,
        mut internal_rx: tokio::sync::broadcast::Receiver<Arc<Vec<Event>>>,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) {
        for w in self.heap.values_mut() {
            w.run().await.expect("Failed to start writer");
        }

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // finalizing & closing the file is handeled by Writer's Drop impl
                    result = upstream_rx.recv() => {
                        if let Ok(events) = result {
                            self.process(events).await;
                        } else {
                            debug!("Upstream channel closed, shutting down ParquetBackend");
                            break;
                        }
                    },
                    result = internal_rx.recv() => {
                        if let Ok(events) = result {
                            self.process(events).await;
                        } else {
                            debug!("Internal channel closed, shutting down ParquetBackend");
                            break;
                        }
                    },
                    _ = shutdown.recv() => {
                        info!("shutting down Parquet writer...");
                        return;
                    }
                };
            }
        });
    }
}
