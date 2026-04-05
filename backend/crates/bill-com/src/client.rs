//! Bill.com REST API client
//!
//! Uses the Bill.com REST API with session-based authentication.
//! All requests include devKey + sessionId headers.
//! Key operations:
//! - Vendor CRUD (sync vendor master data)
//! - Bill CRUD (push approved invoices)
//! - Payment creation (ACH, check, virtual card)
//! - Bulk payment support
//! - Funding account queries
//!
//! Reference: https://developer.bill.com/docs

use crate::auth::BillComEnvironment;
use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use tokio::time::sleep;

/// Errors returned by the Bill.com API client
#[derive(Debug)]
pub enum ClientError {
    /// 401 Unauthorized - session expired or invalid
    TokenExpired { body: String },
    /// 429 Too Many Requests - rate limited
    RateLimited { retry_after: Option<u64> },
    /// Other API error (4xx/5xx not handled above)
    ApiError { status: u16, body: String },
    /// Network/transport error
    Transport(anyhow::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::TokenExpired { body } => {
                write!(f, "Session expired: {}", body)
            }
            ClientError::RateLimited { retry_after } => {
                write!(
                    f,
                    "Rate limited (retry_after: {:?})",
                    retry_after
                )
            }
            ClientError::ApiError { status, body } => {
                write!(f, "API error (status {}): {}", status, body)
            }
            ClientError::Transport(err) => write!(f, "Transport error: {}", err),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::Transport(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

const MAX_RETRIES: u32 = 3;

/// Bill.com REST API client
pub struct BillComClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Active session
    session: BillComSession,
    /// Environment
    environment: BillComEnvironment,
    /// Developer key (required in all request headers)
    dev_key: String,
}

impl BillComClient {
    /// Create a new Bill.com API client with an active session
    pub fn new(session: BillComSession, environment: BillComEnvironment, dev_key: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            session,
            environment,
            dev_key,
        }
    }

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!("{}/{}", self.environment.base_url(), resource)
    }

    /// Build headers with devKey and sessionId for Bill.com API requests
    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "devKey",
            HeaderValue::from_str(&self.dev_key).context("Invalid devKey header")?,
        );
        headers.insert(
            "sessionId",
            HeaderValue::from_str(&self.session.session_id).context("Invalid sessionId header")?,
        );
        Ok(headers)
    }

    /// Compute backoff duration for a given attempt number.
    /// Uses exponential backoff: min(2^attempt * 500ms, 30s) + random jitter (0-500ms).
    /// If `retry_after_secs` is provided (from Retry-After header), use that capped at 60s.
    fn compute_backoff(attempt: u32, retry_after_secs: Option<u64>) -> Duration {
        if let Some(secs) = retry_after_secs {
            let capped = secs.min(60);
            return Duration::from_secs(capped);
        }
        let base_ms: u64 = 500 * (1u64 << attempt);
        let capped_ms = base_ms.min(30_000);
        // Simple jitter: use current time nanos modulo 500ms
        let jitter_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            % 500_000_000) as u64
            / 1_000_000;
        Duration::from_millis(capped_ms + jitter_ms)
    }

    /// Execute a request with retry logic for 429 (rate limit) and 5xx (transient server errors).
    /// Bill.com uses session-based auth; a 401 means the session expired and needs
    /// re-login, which is out of scope for the client - just return the error.
    async fn execute_with_retry(
        &self,
        request_fn: impl Fn(HeaderMap) -> reqwest::RequestBuilder,
    ) -> std::result::Result<reqwest::Response, ClientError> {
        let mut attempt = 0u32;

        loop {
            let headers = self.build_headers().map_err(|e| ClientError::Transport(e))?;

            let result = request_fn(headers).send().await;

            let response = match result {
                Ok(resp) => resp,
                Err(err) => {
                    // Transport/network error: retry once
                    if attempt == 0 {
                        tracing::warn!(
                            attempt,
                            error = %err,
                            "Transport error, retrying once"
                        );
                        attempt += 1;
                        continue;
                    }
                    return Err(ClientError::Transport(
                        anyhow::Error::from(err).context("Transport error after retry"),
                    ));
                }
            };

            let status = response.status();
            let status_code = status.as_u16();

            if status.is_success() {
                return Ok(response);
            }

            // Extract Retry-After before consuming the body
            let retry_after_header = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            // Read the body for error reporting
            let body_text = response.text().await.unwrap_or_default();

            if status_code == 401 {
                // Bill.com uses session-based auth; 401 means session expired.
                // Session refresh (re-login) is handled outside the client.
                return Err(ClientError::TokenExpired { body: body_text });
            }

            if status_code == 429 {
                attempt += 1;
                if attempt >= MAX_RETRIES {
                    return Err(ClientError::RateLimited { retry_after: retry_after_header });
                }
                let backoff = Self::compute_backoff(attempt, retry_after_header);
                tracing::warn!(
                    attempt,
                    ?backoff,
                    retry_after = retry_after_header,
                    "Rate limited, retrying"
                );
                sleep(backoff).await;
                continue;
            }

            if status.is_server_error() {
                // 5xx: retry with backoff
                attempt += 1;
                if attempt >= MAX_RETRIES {
                    return Err(ClientError::ApiError {
                        status: status_code,
                        body: body_text,
                    });
                }
                let backoff = Self::compute_backoff(attempt, None);
                tracing::warn!(
                    attempt,
                    status_code,
                    ?backoff,
                    "Server error, retrying"
                );
                sleep(backoff).await;
                continue;
            }

            // Other 4xx: permanent client error, fail immediately
            return Err(ClientError::ApiError {
                status: status_code,
                body: body_text,
            });
        }
    }

    /// Make a GET request to Bill.com API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .execute_with_retry(|headers| {
                self.http_client.get(&url).headers(headers)
            })
            .await
            .map_err(|e| anyhow::Error::from(e))?;

        let body = response
            .text()
            .await
            .context("Failed to read Bill.com API response")?;

        serde_json::from_str(&body)
            .context("Failed to parse Bill.com API response")
    }

    /// Make a POST request to Bill.com API
    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        resource: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.build_url(resource);
        let body_bytes = serde_json::to_vec(body).context("Failed to serialize POST body")?;

        let response = self
            .execute_with_retry(|headers| {
                self.http_client
                    .post(&url)
                    .headers(headers)
                    .body(reqwest::Body::from(body_bytes.clone()))
                    .header(CONTENT_TYPE, "application/json")
            })
            .await
            .map_err(|e| anyhow::Error::from(e))?;

        let body_text = response
            .text()
            .await
            .context("Failed to read Bill.com API response")?;

        serde_json::from_str(&body_text)
            .context("Failed to parse Bill.com API response")
    }

    // ──────────────────────────── Vendor Operations ────────────────────────────

    /// List vendors with pagination
    pub async fn list_vendors(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<BillComListResponse<BillComVendor>> {
        let resource = format!(
            "vendors?page={}&pageSize={}",
            page, page_size
        );

        self.get(&resource).await
    }

    /// Get a single vendor by ID
    pub async fn get_vendor(&self, vendor_id: &str) -> Result<BillComVendor> {
        self.get(&format!("vendors/{}", vendor_id)).await
    }

    /// Create a new vendor
    pub async fn create_vendor(&self, vendor: &BillComVendor) -> Result<BillComVendor> {
        self.post("vendors", vendor).await
    }

    // ──────────────────────────── Bill Operations ────────────────────────────

    /// Create a bill (push approved invoice to Bill.com)
    pub async fn create_bill(&self, bill: &BillComBill) -> Result<BillComBill> {
        self.post("bills", bill).await
    }

    /// List bills with pagination
    pub async fn list_bills(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<BillComListResponse<BillComBill>> {
        let resource = format!(
            "bills?page={}&pageSize={}",
            page, page_size
        );

        self.get(&resource).await
    }

    /// Get a single bill by ID
    pub async fn get_bill(&self, bill_id: &str) -> Result<BillComBill> {
        self.get(&format!("bills/{}", bill_id)).await
    }

    // ──────────────────────────── Payment Operations ────────────────────────────

    /// Create a payment for a bill
    pub async fn create_payment(&self, payment: &BillComPayment) -> Result<BillComPayment> {
        self.post("payments", payment).await
    }

    /// Create a bulk payment for multiple bills
    pub async fn create_bulk_payment(
        &self,
        bulk_request: &BillComBulkPaymentRequest,
    ) -> Result<serde_json::Value> {
        self.post("payments/bulk", bulk_request).await
    }

    /// Get a single payment by ID
    pub async fn get_payment(&self, payment_id: &str) -> Result<BillComPayment> {
        self.get(&format!("payments/{}", payment_id)).await
    }

    // ──────────────────────────── Funding Account Operations ────────────────────────────

    /// List funding accounts (bank accounts for payment disbursement)
    pub async fn list_funding_accounts(&self) -> Result<BillComListResponse<BillComFundingAccount>> {
        self.get("funding-accounts/banks").await
    }

    /// Execute a GET request to an arbitrary URL using the retry logic.
    /// This is exposed for integration testing only.
    #[doc(hidden)]
    pub async fn execute_get_for_test(&self, url: &str) -> std::result::Result<String, ClientError> {
        let response = self
            .execute_with_retry(|headers| {
                self.http_client.get(url).headers(headers)
            })
            .await?;
        response
            .text()
            .await
            .map_err(|e| ClientError::Transport(anyhow::Error::from(e)))
    }
}
