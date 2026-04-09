//! QuickBooks API client

use crate::oauth::QuickBooksEnvironment;
use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;
use std::time::Duration;
use tokio::time::sleep;

/// Re-export shared HTTP retry error type as ClientError for backward compatibility
pub use billforge_core::http_retry::HttpRetryError as ClientError;

type TokenRefresher = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync,
>;

use billforge_core::http_retry::{self, RetryConfig};

/// Request wrapper for sparse bill updates.
/// `sparse: true` tells QuickBooks to only modify the supplied fields
/// instead of overwriting unset fields with defaults.
#[derive(Debug, Serialize)]
pub struct UpdateBillRequest {
    pub sparse: bool,
    #[serde(flatten)]
    pub bill: QBBill,
}

/// Response envelope for bill update operations.
#[derive(Debug, Deserialize)]
pub struct UpdateBillResponse {
    pub Bill: QBBill,
}

/// Request wrapper for sparse vendor updates.
#[derive(Debug, Serialize)]
pub struct UpdateVendorRequest {
    pub sparse: bool,
    #[serde(flatten)]
    pub vendor: QBVendor,
}

/// Response envelope for vendor update operations.
#[derive(Debug, Deserialize)]
pub struct UpdateVendorResponse {
    pub Vendor: QBVendor,
}

/// QuickBooks API client
pub struct QuickBooksClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token (interior-mutable for refresh)
    access_token: RwLock<String>,
    /// Company ID (realm ID)
    company_id: String,
    /// Environment
    environment: QuickBooksEnvironment,
    /// Optional token refresher callback for 401 handling
    token_refresher: Option<TokenRefresher>,
}

impl QuickBooksClient {
    /// Create a new QuickBooks API client
    pub fn new(
        access_token: String,
        company_id: String,
        environment: QuickBooksEnvironment,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            access_token: RwLock::new(access_token),
            company_id,
            environment,
            token_refresher: None,
        }
    }

    /// Set a token refresher callback. On 401 responses, the client will call
    /// this to obtain a fresh access token, update internal state, and retry once.
    pub fn with_token_refresher<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String>> + Send + 'static,
    {
        self.token_refresher = Some(Box::new(move || {
            let fut = f();
            Box::pin(fut) as Pin<Box<dyn Future<Output = Result<String>> + Send>>
        }));
        self
    }

    /// Build API URL for a query
    fn build_url(&self, resource: &str) -> String {
        format!(
            "{}/v3/company/{}/{}",
            self.environment.base_url(),
            self.company_id,
            resource
        )
    }

    /// Build headers for QuickBooks API requests
    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    /// Execute a request with retry logic for 429 (rate limit), 5xx (transient server
    /// errors), and 401 (token refresh if refresher is configured).
    async fn execute_with_retry(
        &self,
        request_fn: impl Fn(String, HeaderMap) -> reqwest::RequestBuilder,
    ) -> std::result::Result<reqwest::Response, ClientError> {
        let config = RetryConfig::default();
        let mut attempt = 0u32;
        let mut refreshed = false;

        loop {
            let token = {
                self.access_token
                    .read()
                    .expect("access_token lock poisoned")
                    .clone()
            };
            let headers = self.build_headers().map_err(|e| ClientError::Transport(e.to_string()))?;

            let result = request_fn(token, headers).send().await;

            let response = match result {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt == 0 {
                        tracing::warn!(attempt, error = %err, "Transport error, retrying once");
                        attempt += 1;
                        continue;
                    }
                    return Err(ClientError::Transport(format!("Transport error after retry: {}", err)));
                }
            };

            let status = response.status();
            let status_code = status.as_u16();

            if status.is_success() {
                return Ok(response);
            }

            let retry_after_header = http_retry::parse_retry_after(
                response.headers().get("Retry-After").and_then(|v| v.to_str().ok()),
            );
            let body_text = response.text().await.unwrap_or_default();

            if status_code == 401 {
                if !refreshed {
                    if let Some(ref refresher) = self.token_refresher {
                        tracing::warn!(attempt, "Received 401, attempting token refresh");
                        match refresher().await {
                            Ok(new_token) => {
                                let mut guard = self.access_token.write().expect("access_token lock poisoned");
                                *guard = new_token;
                                refreshed = true;
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Token refresh failed");
                            }
                        }
                    }
                }
                return Err(ClientError::TokenExpired { body: body_text });
            }

            if http_retry::is_retryable_status(status_code) {
                attempt += 1;
                if attempt >= config.max_retries {
                    if status_code == 429 {
                        return Err(ClientError::RateLimited { retry_after: retry_after_header });
                    }
                    return Err(ClientError::ApiError { status: status_code, body: body_text });
                }
                let backoff = http_retry::compute_backoff(&config, attempt, if status_code == 429 { retry_after_header } else { None });
                tracing::warn!(attempt, status_code, ?backoff, "Retryable error, backing off");
                sleep(backoff).await;
                continue;
            }

            return Err(ClientError::ApiError { status: status_code, body: body_text });
        }
    }

    /// Make a GET request to QuickBooks API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .execute_with_retry(|token, headers| {
                self.http_client
                    .get(&url)
                    .bearer_auth(&token)
                    .headers(headers)
            })
            .await
            .map_err(|e| anyhow::Error::from(e))?;

        response
            .json()
            .await
            .context("Failed to parse QuickBooks API response")
    }

    /// Make a POST request to QuickBooks API
    async fn post<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);
        let body_bytes = serde_json::to_vec(body).context("Failed to serialize POST body")?;

        let response = self
            .execute_with_retry(|token, headers| {
                self.http_client
                    .post(&url)
                    .bearer_auth(&token)
                    .headers(headers)
                    .body(reqwest::Body::from(body_bytes.clone()))
                    .header(CONTENT_TYPE, "application/json")
            })
            .await
            .map_err(|e| anyhow::Error::from(e))?;

        response
            .json()
            .await
            .context("Failed to parse QuickBooks API response")
    }

    /// Query vendors
    pub async fn query_vendors(&self, start_position: i32, max_results: i32) -> Result<Vec<QBVendor>> {
        let query = format!(
            "SELECT * FROM Vendor STARTPOSITION {} MAXRESULTS {}",
            start_position, max_results
        );

        let response: QBQueryResponse<QBVendor> = self
            .get(&format!("query?query={}", urlencoding::encode(&query)))
            .await?;

        Ok(response
            .QueryResponse
            .map(|qr| qr.results)
            .unwrap_or_default())
    }

    /// Get vendor by ID
    pub async fn get_vendor(&self, vendor_id: &str) -> Result<QBVendor> {
        self.get(&format!("vendor/{}", vendor_id)).await
    }

    /// Query accounts
    pub async fn query_accounts(&self, start_position: i32, max_results: i32) -> Result<Vec<QBAccount>> {
        let query = format!(
            "SELECT * FROM Account STARTPOSITION {} MAXRESULTS {}",
            start_position, max_results
        );

        let response: QBQueryResponse<QBAccount> = self
            .get(&format!("query?query={}", urlencoding::encode(&query)))
            .await?;

        Ok(response
            .QueryResponse
            .map(|qr| qr.results)
            .unwrap_or_default())
    }

    /// Create a bill (invoice) in QuickBooks
    pub async fn create_bill(&self, bill: &QBBill) -> Result<QBBill> {
        #[derive(Serialize)]
        struct CreateBillRequest {
            #[serde(rename = "Bill")]
            bill: QBBill,
        }

        let request = CreateBillRequest { bill: bill.clone() };

        self.post("bill", &request).await
    }

    /// Query bills
    pub async fn query_bills(&self, start_position: i32, max_results: i32) -> Result<Vec<QBBill>> {
        let query = format!(
            "SELECT * FROM Bill STARTPOSITION {} MAXRESULTS {}",
            start_position, max_results
        );

        let response: QBQueryResponse<QBBill> = self
            .get(&format!("query?query={}", urlencoding::encode(&query)))
            .await?;

        Ok(response
            .QueryResponse
            .map(|qr| qr.results)
            .unwrap_or_default())
    }

    /// Update a bill in QuickBooks using sparse update.
    ///
    /// The caller must populate `Id` and `SyncToken` from a prior fetch (e.g. `query_bills`);
    /// QuickBooks uses SyncToken for optimistic concurrency — stale tokens return 400.
    pub async fn update_bill(&self, bill: &QBBill) -> Result<QBBill> {
        let request = UpdateBillRequest {
            sparse: true,
            bill: bill.clone(),
        };

        let response: UpdateBillResponse = self
            .post("bill?operation=update", &request)
            .await?;

        Ok(response.Bill)
    }

    /// Update a vendor in QuickBooks.
    ///
    /// The caller must populate `Id` and `SyncToken` from a prior fetch (e.g. `query_vendors`);
    /// QuickBooks uses SyncToken for optimistic concurrency — stale tokens return 400.
    pub async fn update_vendor(&self, vendor: &QBVendor) -> Result<QBVendor> {
        let request = UpdateVendorRequest {
            sparse: true,
            vendor: vendor.clone(),
        };

        let response: UpdateVendorResponse = self
            .post("vendor?operation=update", &request)
            .await?;

        Ok(response.Vendor)
    }

    /// Get company info
    pub async fn get_company_info(&self) -> Result<serde_json::Value> {
        self.get("companyinfo/companyid").await
    }

    /// Execute a GET request to an arbitrary URL using the retry logic.
    /// This is exposed for integration testing only.
    #[doc(hidden)]
    pub async fn execute_get_for_test(&self, url: &str) -> std::result::Result<String, ClientError> {
        let response = self
            .execute_with_retry(|token, headers| {
                self.http_client
                    .get(url)
                    .bearer_auth(&token)
                    .headers(headers)
            })
            .await?;
        response
            .text()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))
    }
}
