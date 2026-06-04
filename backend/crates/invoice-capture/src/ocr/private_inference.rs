//! Private-inference OCR provider (refs #334)
//!
//! Routes OCR requests to a customer-managed endpoint inside the tenant's
//! VPC / on-prem cluster.  The endpoint is expected to accept a POST with
//! raw document bytes and return an [`OcrExtractionResult`] JSON payload.
//!
//! Health is tracked lazily: on dispatch failure the row is marked
//! `unhealthy` so subsequent requests fall back to the standard provider
//! until a health check succeeds.

use billforge_core::domain::OcrExtractionResult;
use billforge_core::types::TenantId;
use std::time::Duration;

/// Timeout for the OCR inference HTTP call.
const OCR_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for the lightweight `/health` probe.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Tenant-scoped private-inference configuration loaded from
/// `tenant_private_inference`.
#[derive(Debug, Clone)]
pub struct PrivateInferenceConfig {
    pub enabled: bool,
    pub ocr_endpoint_url: Option<String>,
    pub kms_key_ref: Option<String>,
    pub health_status: HealthStatus,
}

/// Mirrors the `health_status` CHECK constraint in the DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "healthy" => HealthStatus::Healthy,
            "unhealthy" => HealthStatus::Unhealthy,
            _ => HealthStatus::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors produced by the private-inference OCR path.
/// All variants map to "should fall back to standard provider".
#[derive(Debug)]
pub enum PrivateInferenceError {
    /// Private inference is not enabled for this tenant.
    Disabled,
    /// No OCR endpoint URL is configured.
    NoEndpoint,
    /// The endpoint was marked unhealthy from a prior failure.
    Unhealthy,
    /// The HTTP call timed out.
    Timeout,
    /// The endpoint returned a non-2xx or unparseable response.
    BadResponse(String),
    /// A network / transport error occurred.
    Transport(String),
}

impl PrivateInferenceError {
    /// Every variant means we should fall back.
    pub fn should_fallback(&self) -> bool {
        true
    }
}

impl std::fmt::Display for PrivateInferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrivateInferenceError::Disabled => write!(f, "private inference disabled"),
            PrivateInferenceError::NoEndpoint => write!(f, "no private OCR endpoint configured"),
            PrivateInferenceError::Unhealthy => write!(f, "private inference endpoint unhealthy"),
            PrivateInferenceError::Timeout => write!(f, "private inference OCR request timed out"),
            PrivateInferenceError::BadResponse(msg) => {
                write!(f, "bad private inference response: {}", msg)
            }
            PrivateInferenceError::Transport(msg) => {
                write!(f, "private inference transport error: {}", msg)
            }
        }
    }
}

impl std::error::Error for PrivateInferenceError {}

// ---------------------------------------------------------------------------
// Load config
// ---------------------------------------------------------------------------

/// Load private-inference config for a tenant.  Returns `None` when no row
/// exists (tenant has never been opted in).
pub async fn load_for_tenant(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Option<PrivateInferenceConfig> {
    let row: Option<(bool, Option<String>, Option<String>, String)> = sqlx::query_as(
        r#"SELECT enabled, ocr_endpoint_url, kms_key_ref, health_status
           FROM tenant_private_inference
           WHERE tenant_id = $1"#,
    )
    .bind(*tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .ok()?;

    row.map(|(enabled, ocr_endpoint_url, kms_key_ref, health_status)| PrivateInferenceConfig {
        enabled,
        ocr_endpoint_url,
        kms_key_ref,
        health_status: HealthStatus::from_db(&health_status),
    })
}

// ---------------------------------------------------------------------------
// Run private OCR
// ---------------------------------------------------------------------------

/// POST document bytes to the tenant's private OCR endpoint and parse the
/// standardised [`OcrExtractionResult`] response.
pub async fn run_private_ocr(
    cfg: &PrivateInferenceConfig,
    document_bytes: &[u8],
) -> Result<OcrExtractionResult, PrivateInferenceError> {
    if !cfg.enabled {
        return Err(PrivateInferenceError::Disabled);
    }

    let url = cfg
        .ocr_endpoint_url
        .as_deref()
        .ok_or(PrivateInferenceError::NoEndpoint)?;

    if cfg.health_status != HealthStatus::Healthy {
        return Err(PrivateInferenceError::Unhealthy);
    }

    let client = reqwest::Client::builder()
        .timeout(OCR_TIMEOUT)
        .build()
        .map_err(|e| PrivateInferenceError::Transport(e.to_string()))?;

    let response = client
        .post(url)
        .header("Content-Type", "application/octet-stream")
        .body(document_bytes.to_vec())
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                PrivateInferenceError::Timeout
            } else {
                PrivateInferenceError::Transport(e.to_string())
            }
        })?;

    if !response.status().is_success() {
        return Err(PrivateInferenceError::BadResponse(format!(
            "HTTP {}",
            response.status()
        )));
    }

    response
        .json::<OcrExtractionResult>()
        .await
        .map_err(|e| PrivateInferenceError::BadResponse(format!("JSON parse: {}", e)))
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

/// Lightweight GET to `{endpoint_base}/health`.  On success the DB row is
/// marked `healthy`; on failure it is marked `unhealthy` with the error text.
pub async fn check_health(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    cfg: &PrivateInferenceConfig,
) -> HealthStatus {
    let url = match cfg.ocr_endpoint_url.as_deref() {
        Some(u) => {
            // Append /health, stripping any trailing slash.
            let base = u.trim_end_matches('/');
            format!("{}/health", base)
        }
        None => {
            // No endpoint to probe — leave status as-is.
            return cfg.health_status;
        }
    };

    let client = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build();

    let new_status = match client {
        Ok(c) => match c.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => HealthStatus::Healthy,
            Ok(resp) => {
                let err_text = format!("HTTP {}", resp.status());
                let _ = mark_health(pool, tenant_id, HealthStatus::Unhealthy, &err_text).await;
                return HealthStatus::Unhealthy;
            }
            Err(e) => {
                let err_text = e.to_string();
                let _ = mark_health(pool, tenant_id, HealthStatus::Unhealthy, &err_text).await;
                return HealthStatus::Unhealthy;
            }
        },
        Err(e) => {
            let err_text = e.to_string();
            let _ = mark_health(pool, tenant_id, HealthStatus::Unhealthy, &err_text).await;
            return HealthStatus::Unhealthy;
        }
    };

    let _ = mark_health(pool, tenant_id, new_status, "").await;
    new_status
}

/// Mark the tenant's private-inference endpoint unhealthy (called on
/// dispatch failure so subsequent requests skip the endpoint).
pub async fn mark_unhealthy(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    error: &str,
) {
    let _ = mark_health(pool, tenant_id, HealthStatus::Unhealthy, error).await;
}

/// Internal helper to persist health status.
async fn mark_health(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    status: HealthStatus,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE tenant_private_inference
           SET health_status        = $1,
               last_health_error    = NULLIF($2, ''),
               last_health_check_at = NOW(),
               updated_at           = NOW()
           WHERE tenant_id = $3"#,
    )
    .bind(status.as_str())
    .bind(error)
    .bind(*tenant_id.as_uuid())
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_roundtrip() {
        assert_eq!(HealthStatus::from_db("healthy"), HealthStatus::Healthy);
        assert_eq!(HealthStatus::from_db("unhealthy"), HealthStatus::Unhealthy);
        assert_eq!(HealthStatus::from_db("unknown"), HealthStatus::Unknown);
        assert_eq!(HealthStatus::from_db("garbage"), HealthStatus::Unknown);
    }

    #[test]
    fn health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
        assert_eq!(HealthStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn all_errors_should_fallback() {
        let errors = vec![
            PrivateInferenceError::Disabled,
            PrivateInferenceError::NoEndpoint,
            PrivateInferenceError::Unhealthy,
            PrivateInferenceError::Timeout,
            PrivateInferenceError::BadResponse("test".into()),
            PrivateInferenceError::Transport("conn refused".into()),
        ];
        for err in errors {
            assert!(
                err.should_fallback(),
                "expected should_fallback() for {:?}",
                err
            );
        }
    }

    #[tokio::test]
    async fn run_private_ocr_disabled_returns_disabled_error() {
        let cfg = PrivateInferenceConfig {
            enabled: false,
            ocr_endpoint_url: Some("https://example.com/ocr".into()),
            kms_key_ref: None,
            health_status: HealthStatus::Healthy,
        };
        let result = run_private_ocr(&cfg, b"test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PrivateInferenceError::Disabled));
    }

    #[tokio::test]
    async fn run_private_ocr_no_endpoint_returns_no_endpoint_error() {
        let cfg = PrivateInferenceConfig {
            enabled: true,
            ocr_endpoint_url: None,
            kms_key_ref: None,
            health_status: HealthStatus::Healthy,
        };
        let result = run_private_ocr(&cfg, b"test").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PrivateInferenceError::NoEndpoint));
    }

    #[tokio::test]
    async fn run_private_ocr_unhealthy_returns_unhealthy_error() {
        let cfg = PrivateInferenceConfig {
            enabled: true,
            ocr_endpoint_url: Some("https://example.com/ocr".into()),
            kms_key_ref: None,
            health_status: HealthStatus::Unhealthy,
        };
        let result = run_private_ocr(&cfg, b"test").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PrivateInferenceError::Unhealthy));
    }

    #[tokio::test]
    async fn run_private_ocr_transport_error_on_bad_url() {
        let cfg = PrivateInferenceConfig {
            enabled: true,
            ocr_endpoint_url: Some("http://127.0.0.1:1/ocr".into()),
            kms_key_ref: None,
            health_status: HealthStatus::Healthy,
        };
        let result = run_private_ocr(&cfg, b"test").await;
        assert!(result.is_err());
        // Should be either Transport or Timeout depending on the OS
        let err = result.unwrap_err();
        assert!(err.should_fallback());
    }
}
