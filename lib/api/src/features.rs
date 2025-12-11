//! Feature flag middleware for API responses.
//!
//! Adds X-Feature-Flag header to all responses to communicate
//! enabled features to the frontend.

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use std::sync::RwLock;
const HARD_FLAGS: &'static [&str] = &[
    #[cfg(feature = "duckdb")]
    "duckdb"
];

pub(crate) static FEATURE_FLAGS: RwLock<Vec<String>> = RwLock::new(Vec::new());


/// Middleware to add feature flags to response headers.
///
/// # Usage
/// ```no_run
/// use axum::{Router, middleware};
/// 
/// let app = Router::new()
///     .layer(middleware::from_fn(feature_flag_middleware));
/// ```
pub async fn feature_flag_middleware(
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;

    let _ = HeaderValue::from_str(FEATURE_FLAGS
    .read()
    .map(|flags| flags.iter()
        .map(|f| f.clone())
        .chain(HARD_FLAGS.iter()
                .map(|f| f.to_string()))
        .collect::<Vec<String>>())
    .unwrap_or_default()
    .join(",")
    .as_str())
    .map(|v| response.headers_mut().append("X-Feature-Flag", v));


    response
}
