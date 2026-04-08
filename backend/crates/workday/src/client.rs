//! Workday REST API client
//!
//! Uses the Workday REST API with OAuth 2.0 Bearer token authentication.
//! Key operations:
//! - Supplier queries and sync
//! - Supplier invoice creation
//! - Ledger account queries
//! - Spend category queries
//! - Multi-company support
//!
//! Base URL pattern: `https://{tenant_url}/api/v1/{resource}`

use crate::types::*;
use anyhow::{Context, Result};
use billforge_core::http_retry::{self, RetryConfig};
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::sleep;

/// Workday REST API client
pub struct WorkdayClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token (Bearer)
    access_token: String,
    /// Tenant URL (e.g. "https://impl.workday.com")
    tenant_url: String,
    /// Tenant name (e.g. "acme_corp")
    tenant_name: String,
}

impl WorkdayClient {
    /// Create a new Workday API client
    pub fn new(
        access_token: String,
        tenant_url: String,
        tenant_name: String,
    ) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            access_token,
            tenant_url,
            tenant_name,
        }
    }

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!(
            "{}/api/v1/{}",
            self.tenant_url, resource
        )
    }

    /// Send an HTTP request with retry logic for 429/5xx errors.
    async fn send_with_retry(
        &self,
        request_fn: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let config = RetryConfig::default();
        let mut attempt = 0u32;

        loop {
            let result = request_fn().send().await;

            let response = match result {
                Ok(resp) => resp,
                Err(err) => {
                    if attempt == 0 {
                        tracing::warn!(attempt, error = %err, "Workday transport error, retrying once");
                        attempt += 1;
                        continue;
                    }
                    anyhow::bail!("Workday transport error after retry: {}", err);
                }
            };

            let status = response.status();
            let status_code = status.as_u16();

            if status.is_success() {
                return Ok(response);
            }

            if http_retry::is_retryable_status(status_code) {
                let retry_after = http_retry::parse_retry_after(
                    response.headers().get("Retry-After").and_then(|v| v.to_str().ok()),
                );
                attempt += 1;
                if attempt >= config.max_retries {
                    let body = response.text().await.unwrap_or_default();
                    anyhow::bail!("Workday API request failed after {} retries ({}): {}", attempt, status_code, body);
                }
                let backoff = http_retry::compute_backoff(&config, attempt, retry_after);
                tracing::warn!(attempt, status_code, ?backoff, "Workday retryable error, backing off");
                sleep(backoff).await;
                continue;
            }

            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Workday API request failed (HTTP {}): {}", status, error_text);
        }
    }

    /// Make a GET request to Workday REST API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .send_with_retry(|| {
                self.http_client
                    .get(&url)
                    .bearer_auth(&self.access_token)
                    .header(CONTENT_TYPE, "application/json")
            })
            .await?;

        let body = response
            .text()
            .await
            .context("Failed to read Workday API response")?;

        serde_json::from_str(&body)
            .context("Failed to parse Workday API response")
    }

    /// Make a POST request to Workday REST API
    async fn post<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);
        let body_bytes = serde_json::to_vec(body).context("Failed to serialize POST body")?;

        let response = self
            .send_with_retry(|| {
                self.http_client
                    .post(&url)
                    .bearer_auth(&self.access_token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(reqwest::Body::from(body_bytes.clone()))
            })
            .await?;

        let response_body = response
            .text()
            .await
            .context("Failed to read Workday API response")?;

        serde_json::from_str(&response_body)
            .context("Failed to parse Workday API response")
    }

    // ──────────────────────────── Supplier Operations ────────────────────────────

    /// Query suppliers with pagination
    pub async fn query_suppliers(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<WorkdayQueryResponse<WorkdaySupplier>> {
        let resource = format!(
            "suppliers?offset={}&limit={}",
            page * page_size,
            page_size
        );

        self.get(&resource).await
    }

    /// Get a single supplier by ID
    pub async fn get_supplier(&self, supplier_id: &str) -> Result<WorkdaySupplier> {
        self.get(&format!("suppliers/{}", supplier_id)).await
    }

    // ──────────────────────────── Ledger Account Operations ────────────────────────────

    /// Query ledger accounts with pagination
    pub async fn query_ledger_accounts(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<WorkdayQueryResponse<WorkdayLedgerAccount>> {
        let resource = format!(
            "ledgerAccounts?offset={}&limit={}",
            page * page_size,
            page_size
        );

        self.get(&resource).await
    }

    // ──────────────────────────── Spend Category Operations ────────────────────────────

    /// Query spend categories with pagination
    pub async fn query_spend_categories(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<WorkdayQueryResponse<WorkdaySpendCategory>> {
        let resource = format!(
            "spendCategories?offset={}&limit={}",
            page * page_size,
            page_size
        );

        self.get(&resource).await
    }

    // ──────────────────────────── Supplier Invoice Operations ────────────────────────────

    /// Create a supplier invoice in Workday
    pub async fn create_supplier_invoice(
        &self,
        invoice: &WorkdaySupplierInvoice,
    ) -> Result<WorkdaySupplierInvoice> {
        self.post("supplierInvoices", invoice).await
    }

    /// Get a supplier invoice by ID
    pub async fn get_supplier_invoice(
        &self,
        invoice_id: &str,
    ) -> Result<WorkdaySupplierInvoice> {
        self.get(&format!("supplierInvoices/{}", invoice_id)).await
    }

    // ──────────────────────────── Company Operations ────────────────────────────

    /// Query companies (for multi-company support)
    pub async fn query_companies(&self) -> Result<WorkdayQueryResponse<WorkdayCompany>> {
        self.get("companies").await
    }

    // ──────────────────────────── Worker / User Info ────────────────────────────

    /// Get current authenticated user (worker) info
    pub async fn get_worker_info(&self) -> Result<serde_json::Value> {
        self.get("me").await
    }
}
