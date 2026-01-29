//! API middleware

use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};
use tracing::info;

/// Request logging middleware
pub async fn log_request(request: Request<Body>, next: Next) -> Response<Body> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = std::time::Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    info!(
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = duration.as_millis(),
        "Request completed"
    );

    response
}
