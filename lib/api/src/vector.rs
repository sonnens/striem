use crate::{ApiState, sinks::SINKS, sources::SOURCES};
use axum::{Router, extract::State, routing::get};
use toml::{Table, toml};
use striem_config::output::Destination;

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

    if let Some(Destination::Vector(ref cfg)) = state.config.output {
        if let Some(api) = &cfg.api {
            let api_address = api.address().to_string();
            let api_config = toml! {
                [api]
                enabled = true
                address = api_address
            };
            config.extend(api_config);
        }

        config.entry("sources")
        .or_insert_with(|| toml::Table::new().into())
        .as_table_mut()
        .map(|sources| {
            let address = cfg.cfg.address().to_string();
            let source_striem = toml! {
                [source-striem]
                type = "vector"
                address = address
                version = "2"
            };
            sources.extend(source_striem);
        });

        // TODO: set valid_tokens based on the list of sources
        if let Some(hec) = &cfg.hec {
                config.entry("sources")
                .or_insert_with(|| toml::Table::new().into())
                .as_table_mut()
                .map(|sources| {
                    let address = hec.address().to_string();
                    let hec_striem = toml! {
                        [source-striem-hec]
                        type = "splunk_hec"
                        address = address
                        store_hec_token = true
                    };
                    sources.extend(hec_striem);
                });
        }

        if let Some(http) = &cfg.http {
            /* some log producers, notably Github webhooks
             * send JSON data but don't set the content-type header
             * so rather than relying on Vector's json decoding codec
             * take the raw body and attempt to parse it with VRL
             */ 
            let vrl = [ r#"body, _ = string(.)"#,
                                   r#"if !is_null(body) {"#,
                                   r#"  . = parse_json(body) ?? body"#,
                                   r#"}"#] .join("\n");

                config.entry("sources")
                .or_insert_with(|| toml::Table::new().into())
                .as_table_mut()
                .map(|sources| {
                    let address = http.address().to_string();
                    let http_striem = toml! {
                        [source-striem-http]
                        type = "http_server"
                        address = address
                        headers = ['*']
                        strict_path = false

                        [source-striem-http.decoding]
                        codec = "vrl"
                        vrl = {"source" = vrl}
                    };
                    sources.extend(http_striem);
                });
        }
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
