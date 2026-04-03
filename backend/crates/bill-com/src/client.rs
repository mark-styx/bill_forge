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
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;

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
        Self {
            http_client: reqwest::Client::new(),
            session,
            environment,
            dev_key,
        }
    }

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!("{}/{}", self.environment.base_url(), resource)
    }

    /// Make a GET request to Bill.com API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .get(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("devKey", &self.dev_key)
            .header("sessionId", &self.session.session_id)
            .send()
            .await
            .context("Failed to send GET request to Bill.com API")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Bill.com API response")?;

        if !status.is_success() {
            anyhow::bail!("Bill.com API request failed (HTTP {}): {}", status, body);
        }

        serde_json::from_str(&body).context("Failed to parse Bill.com API response")
    }

    /// Make a POST request to Bill.com API
    async fn post<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("devKey", &self.dev_key)
            .header("sessionId", &self.session.session_id)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request to Bill.com API")?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("Failed to read Bill.com API response")?;

        if !status.is_success() {
            anyhow::bail!(
                "Bill.com API request failed (HTTP {}): {}",
                status,
                body_text
            );
        }

        serde_json::from_str(&body_text).context("Failed to parse Bill.com API response")
    }

    // ──────────────────────────── Vendor Operations ────────────────────────────

    /// List vendors with pagination
    pub async fn list_vendors(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<BillComListResponse<BillComVendor>> {
        let resource = format!("vendors?page={}&pageSize={}", page, page_size);

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
        let resource = format!("bills?page={}&pageSize={}", page, page_size);

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
    pub async fn list_funding_accounts(
        &self,
    ) -> Result<BillComListResponse<BillComFundingAccount>> {
        self.get("funding-accounts/banks").await
    }
}
