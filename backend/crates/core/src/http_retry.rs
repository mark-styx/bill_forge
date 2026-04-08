//! Shared HTTP retry utilities for integration crates
//!
//! Provides exponential backoff computation, retry error types, and retry
//! configuration used by all integration clients (QuickBooks, Xero, Bill.com,
//! Salesforce, Workday, Sage Intacct).

use std::time::Duration;

/// Maximum number of retry attempts before giving up
pub const MAX_RETRIES: u32 = 3;

/// Configuration for HTTP retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base backoff in milliseconds (doubled each attempt)
    pub base_backoff_ms: u64,
    /// Maximum backoff cap in milliseconds
    pub max_backoff_ms: u64,
    /// Maximum jitter in milliseconds
    pub max_jitter_ms: u64,
    /// Maximum Retry-After header value to honor (seconds)
    pub max_retry_after_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: MAX_RETRIES,
            base_backoff_ms: 500,
            max_backoff_ms: 30_000,
            max_jitter_ms: 500,
            max_retry_after_secs: 60,
        }
    }
}

/// Errors returned by HTTP retry logic
#[derive(Debug)]
pub enum HttpRetryError {
    /// 401 Unauthorized - token expired or invalid
    TokenExpired { body: String },
    /// 429 Too Many Requests - rate limited after exhausting retries
    RateLimited { retry_after: Option<u64> },
    /// Other API error (4xx/5xx not handled above)
    ApiError { status: u16, body: String },
    /// Network/transport error
    Transport(String),
}

impl std::fmt::Display for HttpRetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpRetryError::TokenExpired { body } => {
                write!(f, "Token expired: {}", body)
            }
            HttpRetryError::RateLimited { retry_after } => {
                write!(f, "Rate limited (retry_after: {:?})", retry_after)
            }
            HttpRetryError::ApiError { status, body } => {
                write!(f, "API error (status {}): {}", status, body)
            }
            HttpRetryError::Transport(msg) => write!(f, "Transport error: {}", msg),
        }
    }
}

impl std::error::Error for HttpRetryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}


/// Compute backoff duration for a given attempt number.
///
/// Uses exponential backoff: `min(2^attempt * base_ms, max_ms) + jitter`.
/// If `retry_after_secs` is provided (from Retry-After header), uses that
/// value capped at `config.max_retry_after_secs`.
pub fn compute_backoff(config: &RetryConfig, attempt: u32, retry_after_secs: Option<u64>) -> Duration {
    if let Some(secs) = retry_after_secs {
        let capped = secs.min(config.max_retry_after_secs);
        return Duration::from_secs(capped);
    }
    let base_ms: u64 = config.base_backoff_ms * (1u64 << attempt);
    let capped_ms = base_ms.min(config.max_backoff_ms);
    // Simple jitter: use current time nanos modulo max_jitter
    let jitter_ms = if config.max_jitter_ms > 0 {
        let jitter_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        (jitter_nanos / 1_000_000) % config.max_jitter_ms
    } else {
        0
    };
    Duration::from_millis(capped_ms + jitter_ms)
}

/// Determine whether a given HTTP status code is retryable.
///
/// Returns `true` for 429 (rate limited) and 5xx (server errors).
pub fn is_retryable_status(status_code: u16) -> bool {
    status_code == 429 || (500..600).contains(&status_code)
}

/// Parse the Retry-After header value from a response header string.
pub fn parse_retry_after(header_value: Option<&str>) -> Option<u64> {
    header_value.and_then(|v| v.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_backoff_exponential() {
        let config = RetryConfig {
            max_jitter_ms: 0, // Disable jitter for deterministic test
            ..RetryConfig::default()
        };

        let d0 = compute_backoff(&config, 0, None);
        let d1 = compute_backoff(&config, 1, None);
        let d2 = compute_backoff(&config, 2, None);

        assert_eq!(d0, Duration::from_millis(500));
        assert_eq!(d1, Duration::from_millis(1000));
        assert_eq!(d2, Duration::from_millis(2000));
    }

    #[test]
    fn compute_backoff_caps_at_max() {
        let config = RetryConfig {
            max_jitter_ms: 0,
            max_backoff_ms: 30_000,
            ..RetryConfig::default()
        };

        // 2^10 * 500 = 512_000, should be capped to 30_000
        let d = compute_backoff(&config, 10, None);
        assert_eq!(d, Duration::from_millis(30_000));
    }

    #[test]
    fn compute_backoff_retry_after_header() {
        let config = RetryConfig::default();

        let d = compute_backoff(&config, 0, Some(5));
        assert_eq!(d, Duration::from_secs(5));
    }

    #[test]
    fn compute_backoff_retry_after_capped() {
        let config = RetryConfig {
            max_retry_after_secs: 60,
            ..RetryConfig::default()
        };

        let d = compute_backoff(&config, 0, Some(120));
        assert_eq!(d, Duration::from_secs(60));
    }

    #[test]
    fn compute_backoff_with_jitter_bounded() {
        let config = RetryConfig::default(); // max_jitter_ms = 500

        let d = compute_backoff(&config, 0, None);
        // Base is 500ms, jitter adds 0-499ms
        assert!(d >= Duration::from_millis(500));
        assert!(d < Duration::from_millis(1000));
    }

    #[test]
    fn is_retryable_status_codes() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(599));
        assert!(!is_retryable_status(200));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn parse_retry_after_valid() {
        assert_eq!(parse_retry_after(Some("5")), Some(5));
        assert_eq!(parse_retry_after(Some("120")), Some(120));
    }

    #[test]
    fn parse_retry_after_invalid() {
        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("not-a-number")), None);
        assert_eq!(parse_retry_after(Some("")), None);
    }
}
