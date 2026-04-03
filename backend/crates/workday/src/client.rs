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
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;

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
    pub fn new(access_token: String, tenant_url: String, tenant_name: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            access_token,
            tenant_url,
            tenant_name,
        }
    }

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!("{}/api/v1/{}", self.tenant_url, resource)
    }

    /// Make a GET request to Workday REST API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .context("Failed to send GET request to Workday API")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Workday API response")?;

        if !status.is_success() {
            anyhow::bail!("Workday API request failed (HTTP {}): {}", status, body);
        }

        serde_json::from_str(&body).context("Failed to parse Workday API response")
    }

    /// Make a POST request to Workday REST API
    async fn post<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send POST request to Workday API")?;

        let status = response.status();
        let response_body = response
            .text()
            .await
            .context("Failed to read Workday API response")?;

        if !status.is_success() {
            anyhow::bail!(
                "Workday API request failed (HTTP {}): {}",
                status,
                response_body
            );
        }

        serde_json::from_str(&response_body).context("Failed to parse Workday API response")
    }

    // ──────────────────────────── Supplier Operations ────────────────────────────

    /// Query suppliers with pagination
    pub async fn query_suppliers(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<WorkdayQueryResponse<WorkdaySupplier>> {
        let resource = format!("suppliers?offset={}&limit={}", page * page_size, page_size);

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
    pub async fn get_supplier_invoice(&self, invoice_id: &str) -> Result<WorkdaySupplierInvoice> {
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
