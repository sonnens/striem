use crate::{ApiState, sinks::SINKS, sources::SOURCES};
use axum::{Router, extract::State, routing::get};
use toml::{Table, toml};

async fn get_vector_config(State(state): State<ApiState>) -> String {

    let fqdn = state.config.fqdn.as_ref().map(|f| f.clone()).unwrap_or_else(|| {
        state.config.input.url()
    });

    let mut config = toml! {
        [schema]
        log_namespace = true

        // this ensures the ocsf-* wildcard input always has at least one producer
        [sources.ocsf-stdin]
        type = "stdin"
        decoding = { codec = "json" }
        framing = { method = "newline_delimited" }

        [sinks.sink-striem]
        type = "vector"
        inputs = ["ocsf-*"]
        address = fqdn
    };

    if let Some(ref cfg) = state.config.output {
        config.entry("sources")
        .or_insert_with(|| toml::Table::new().into())
        .as_table_mut()
        .map(|sources| {
            let address = cfg.address().to_string();
            let source_striem = toml! {
                ["source-striem"]
                type = "vector"
                address = address
                version = "2"
            };
            sources.extend(source_striem);
        });
    }

    SOURCES.read().await.iter().for_each(|source| {
        Table::try_from(source)
            .map(|t| {
                t.get("sources").and_then(|s| s.as_table()).map(|s| {
                    config
                        .entry("sources")
                        .or_insert_with(|| Table::new().into())
                        .as_table_mut()
                        .map(|st| {
                            st.extend(s.clone());
                        });
                });

                t.get("transforms")
                    .and_then(|transforms| transforms.as_table())
                    .map(|transforms| {
                        config
                            .entry("transforms")
                            .or_insert_with(|| Table::new().into())
                            .as_table_mut()
                            .map(|config_transforms| {
                                config_transforms.extend(transforms.clone());
                            });
                    });
            })
            .unwrap_or_else(|_| {
                log::error!("Failed to convert source to TOML: {}", source.id());
            });
    });

    let sinks = SINKS.read().await;

    config
        .entry("sinks")
        .or_insert_with(|| Table::new().into())
        .as_table_mut()
        .map(|config_sinks| {
            sinks.iter().for_each(|sink| {
                Table::try_from(sink)
                    .map(|s| config_sinks.extend(s))
                    .unwrap_or_else(|_| {
                        log::error!("Failed to convert sink to TOML: {}", sink.id);
                    });
            });
        });

    config.to_string()
}

pub fn create_router() -> axum::Router<ApiState> {
    Router::new().route("/", get(get_vector_config))
}
