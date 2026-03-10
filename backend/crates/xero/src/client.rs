//! Xero API client

use crate::oauth::XeroEnvironment;
use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Xero API client
pub struct XeroClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token
    access_token: String,
    /// Xero tenant ID (organization)
    tenant_id: String,
    /// Environment
    environment: XeroEnvironment,
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
            access_token,
            tenant_id,
            environment,
        }
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

    /// Make a GET request to Xero API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);
        let headers = self.build_headers()?;

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .headers(headers)
            .send()
            .await
            .context("Failed to send GET request to Xero API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero API request failed: {}", error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse Xero API response")
    }

    /// Make a POST request to Xero API
    async fn post<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);
        let headers = self.build_headers()?;

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.access_token)
            .headers(headers)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request to Xero API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero API request failed: {}", error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse Xero API response")
    }

    /// Make a PUT request to Xero API
    async fn put<T: DeserializeOwned, B: Serialize>(&self, resource: &str, body: &B) -> Result<T> {
        let url = self.build_url(resource);
        let headers = self.build_headers()?;

        let response = self
            .http_client
            .put(&url)
            .bearer_auth(&self.access_token)
            .headers(headers)
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request to Xero API")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero API request failed: {}", error_text);
        }

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
}
