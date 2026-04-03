//! API middleware

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    middleware::Next,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Request logging middleware
pub async fn log_request(request: Request<Body>, next: Next) -> Response<Body> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();

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

/// Per-IP token bucket for rate limiting
#[derive(Clone)]
struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

/// Shared rate limiter state
#[derive(Clone)]
pub struct RateLimiterState {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    max_tokens: u32,
    refill_interval_secs: u64,
}

impl RateLimiterState {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_tokens: max_requests,
            refill_interval_secs: window_secs,
        }
    }
}

/// Rate limiting middleware for auth endpoints.
/// Limits requests per source IP using a token bucket algorithm.
pub async fn rate_limit_auth(
    request: Request<Body>,
    next: Next,
) -> Result<Response<Body>, StatusCode> {
    // Extract client IP from X-Forwarded-For header, X-Real-IP, or connection info
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Use extension-based rate limiter state
    let state = request.extensions().get::<RateLimiterState>().cloned();

    if let Some(state) = state {
        let mut buckets = state.buckets.lock().await;
        let now = Instant::now();

        let bucket = buckets.entry(client_ip.clone()).or_insert(TokenBucket {
            tokens: state.max_tokens,
            last_refill: now,
        });

        // Refill tokens if window has elapsed
        let elapsed = now.duration_since(bucket.last_refill).as_secs();
        if elapsed >= state.refill_interval_secs {
            bucket.tokens = state.max_tokens;
            bucket.last_refill = now;
        }

        if bucket.tokens == 0 {
            warn!(
                client_ip = %client_ip,
                "Rate limit exceeded on auth endpoint"
            );
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }

        bucket.tokens -= 1;

        // Periodic cleanup: remove stale entries (every 100th request approx)
        if buckets.len() > 10_000 {
            let cutoff = now - std::time::Duration::from_secs(state.refill_interval_secs * 2);
            buckets.retain(|_, v| v.last_refill > cutoff);
        }
    }

    Ok(next.run(request).await)
}
