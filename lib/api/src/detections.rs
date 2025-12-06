use anyhow::Result;
use axum::{extract::State, routing::get};

use crate::ApiState;

async fn list_rules(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<serde_json::Value>>, (axum::http::StatusCode, String)> {
    
    let rules = serde_json::to_value(&*state.detections.read().await)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .as_array()
        .and_then(|r| {
            Some(
                r.iter()
                    .flat_map(|rule| {
                        rule.as_object().and_then(|obj| {
                            Some(serde_json::json!({
                                "id": obj.get("id")?,
                                "title": obj.get("title")?,
                                "description": obj.get("description")?,
                                "enabled": obj.get("enabled")?.as_bool().unwrap_or_else(|| true),
                                "level": obj.get("level")?,
                                "logsource": obj.get("logsource")?,
                            }))
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();

    Ok(axum::Json(rules))
}

async fn get_rule(
    State(state): State<ApiState>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let detections = state.detections.read().await;
    let rule = detections.get(&rule_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Rule with id {} not found", rule_id),
        )
    })?;

    let rule_json = serde_json::to_value(&rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(rule_json))
}

#[derive(serde::Deserialize)]
struct PatchRulePayload {
    enabled: bool,
}

async fn patch_rule(
    State(state): State<ApiState>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
    axum::extract::Json(payload): axum::extract::Json<PatchRulePayload>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let detections = state.detections.read().await;
    let rule = detections.get(&rule_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Rule with id {} not found", rule_id),
        )
    })?;

    if payload.enabled {
        rule.enable();
    } else {
        rule.disable();
    }

    let rule_json = serde_json::to_value(&rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(rule_json))
}

async fn post_rule(
    State(state): State<ApiState>,
    body: String,
) -> Result<axum::Json<String>, (axum::http::StatusCode, String)> {
    // Parse the YAML content
    let rule: sigmars::SigmaRule = serde_yaml::from_str(&body)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;
    let id = rule.id.clone();
    let mut detections = state.detections.write().await;
    if let Some(_) = detections.get(&id) {
        return Err((
            axum::http::StatusCode::CONFLICT,
            format!("Rule with id {} already exists", rule.id),
        ));
    }
    detections.add(rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(id))
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new()
        .route("/", get(list_rules).post(post_rule))
        .route("/{id}", get(get_rule).patch(patch_rule))
}
