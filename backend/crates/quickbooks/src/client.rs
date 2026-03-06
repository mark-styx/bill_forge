//! QuickBooks API client

use crate::oauth::QuickBooksEnvironment;
use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// QuickBooks API client
pub struct QuickBooksClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token
    access_token: String,
    /// Company ID (realm ID)
    company_id: String,
    /// Environment
    environment: QuickBooksEnvironment,
}

impl QuickBooksClient {
    /// Create a new QuickBooks API client
    pub fn new(
        access_token: String,
        company_id: String,
        environment: QuickBooksEnvironment,
    ) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            access_token,
            company_id,
            environment,
        }
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

    /// Make a GET request to QuickBooks API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .context("Failed to send GET request to QuickBooks API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("QuickBooks API request failed: {}", error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse QuickBooks API response")
    }

    /// Make a POST request to QuickBooks API
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
            .context("Failed to send POST request to QuickBooks API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("QuickBooks API request failed: {}", error_text);
        }

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

    /// Get company info
    pub async fn get_company_info(&self) -> Result<serde_json::Value> {
        self.get("companyinfo/companyid").await
    }
}
