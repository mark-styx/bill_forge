//! Xero API client

use crate::oauth::XeroEnvironment;
use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;
use std::time::Duration;
use tokio::time::sleep;

/// Errors returned by the Xero API client
#[derive(Debug)]
pub enum ClientError {
    /// 401 Unauthorized - token expired or invalid
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
                write!(f, "Token expired: {}", body)
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

type TokenRefresher = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync,
>;

const MAX_RETRIES: u32 = 3;

/// Xero API client
pub struct XeroClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token (interior-mutable for refresh)
    access_token: RwLock<String>,
    /// Xero tenant ID (organization)
    tenant_id: String,
    /// Environment
    environment: XeroEnvironment,
    /// Optional token refresher callback for 401 handling
    token_refresher: Option<TokenRefresher>,
}

impl XeroClient {
    /// Create a new Xero API client
    pub fn new(
        access_token: String,
        tenant_id: String,
        environment: XeroEnvironment,
    ) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            access_token: RwLock::new(access_token),
            tenant_id,
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

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!(
            "{}/api.xro/2.0/{}",
            self.environment.base_url(),
            resource
        )
    }

    /// Build headers with tenant ID
    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "Xero-Tenant-Id",
            HeaderValue::from_str(&self.tenant_id)
                .context("Invalid tenant ID header")?,
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

    /// Execute a request with retry logic for 429 (rate limit), 5xx (transient server
    /// errors), and 401 (token refresh if refresher is configured).
    ///
    /// `request_fn` is a closure that builds a fresh `RequestBuilder` each call,
    /// because `RequestBuilder` is not cloneable.
    async fn execute_with_retry(
        &self,
        request_fn: impl Fn(String, HeaderMap) -> reqwest::RequestBuilder,
    ) -> std::result::Result<reqwest::Response, ClientError> {
        let mut attempt = 0u32;
        let mut refreshed = false;

        loop {
            let token = {
                self.access_token
                    .read()
                    .expect("access_token lock poisoned")
                    .clone()
            };
            let headers = self.build_headers().map_err(|e| ClientError::Transport(e))?;

            let result = request_fn(token, headers).send().await;

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
                if !refreshed {
                    if let Some(ref refresher) = self.token_refresher {
                        tracing::warn!(
                            attempt,
                            "Received 401, attempting token refresh"
                        );
                        match refresher().await {
                            Ok(new_token) => {
                                {
                                    let mut guard = self.access_token.write().expect("access_token lock poisoned");
                                    *guard = new_token;
                                }
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

    /// Make a GET request to Xero API
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
            .context("Failed to parse Xero API response")
    }

    /// Make a POST request to Xero API
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
            .context("Failed to parse Xero API response")
    }

    /// Make a PUT request to Xero API
    async fn put<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);
        let body_bytes = serde_json::to_vec(body).context("Failed to serialize PUT body")?;

        let response = self
            .execute_with_retry(|token, headers| {
                self.http_client
                    .put(&url)
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
            .context("Failed to parse Xero API response")
    }

    /// Query contacts (vendors) with pagination
    pub async fn query_contacts(&self, page: i32, page_size: i32) -> Result<Vec<XeroContact>> {
        let resource = format!(
            "Contacts?page={}&pageSize={}",
            page, page_size
        );

        let response: XeroResponse<XeroContact> = self.get(&resource).await?;

        Ok(response.Items.unwrap_or_default())
    }

    /// Get contact by ID
    pub async fn get_contact(&self, contact_id: &str) -> Result<XeroContact> {
        let response: XeroResponse<XeroContact> = self.get(&format!("Contacts/{}", contact_id)).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Contact not found"))
    }

    /// Create a contact
    pub async fn create_contact(&self, contact: &XeroContact) -> Result<XeroContact> {
        #[derive(Serialize)]
        struct CreateContactRequest {
            #[serde(rename = "Contacts")]
            contacts: Vec<XeroContact>,
        }

        let request = CreateContactRequest {
            contacts: vec![contact.clone()],
        };

        let response: XeroResponse<XeroContact> = self.post("Contacts", &request).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Failed to create contact"))
    }

    /// Query accounts with pagination
    pub async fn query_accounts(&self, page: i32, page_size: i32) -> Result<Vec<XeroAccount>> {
        let resource = format!(
            "Accounts?page={}&pageSize={}",
            page, page_size
        );

        let response: XeroResponse<XeroAccount> = self.get(&resource).await?;

        Ok(response.Items.unwrap_or_default())
    }

    /// Get account by ID
    pub async fn get_account(&self, account_id: &str) -> Result<XeroAccount> {
        let response: XeroResponse<XeroAccount> = self.get(&format!("Accounts/{}", account_id)).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Account not found"))
    }

    /// Create an invoice (bill) in Xero
    pub async fn create_invoice(&self, invoice: &XeroInvoice) -> Result<XeroInvoice> {
        #[derive(Serialize)]
        struct CreateInvoiceRequest {
            #[serde(rename = "Invoices")]
            invoices: Vec<XeroInvoice>,
        }

        let request = CreateInvoiceRequest {
            invoices: vec![invoice.clone()],
        };

        let response: XeroResponse<XeroInvoice> = self.post("Invoices", &request).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Failed to create invoice"))
    }

    /// Update an invoice in Xero
    pub async fn update_invoice(&self, invoice_id: &str, invoice: &XeroInvoice) -> Result<XeroInvoice> {
        #[derive(Serialize)]
        struct UpdateInvoiceRequest {
            #[serde(rename = "Invoices")]
            invoices: Vec<XeroInvoice>,
        }

        let request = UpdateInvoiceRequest {
            invoices: vec![invoice.clone()],
        };

        let response: XeroResponse<XeroInvoice> = self.post(&format!("Invoices/{}", invoice_id), &request).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Failed to update invoice"))
    }

    /// Query invoices with pagination
    pub async fn query_invoices(&self, page: i32, page_size: i32) -> Result<Vec<XeroInvoice>> {
        let resource = format!(
            "Invoices?page={}&pageSize={}",
            page, page_size
        );

        let response: XeroResponse<XeroInvoice> = self.get(&resource).await?;

        Ok(response.Items.unwrap_or_default())
    }

    /// Get invoice by ID
    pub async fn get_invoice(&self, invoice_id: &str) -> Result<XeroInvoice> {
        let response: XeroResponse<XeroInvoice> = self.get(&format!("Invoices/{}", invoice_id)).await?;

        response
            .Items
            .and_then(|items| items.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Invoice not found"))
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
            .map_err(|e| ClientError::Transport(anyhow::Error::from(e)))
    }
}
