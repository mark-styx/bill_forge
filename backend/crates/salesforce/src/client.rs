//! Salesforce REST API client
//!
//! Uses the Salesforce REST API (v59.0) with SOQL queries.
//! Key operations:
//! - Query Accounts (vendor master data sync)
//! - Query Contacts (vendor contact sync)
//! - Query Opportunities (PO number linkage)
//! - Create/Update records for payment status push-back
//!
//! Reference: https://developer.salesforce.com/docs/atlas.en-us.api_rest.meta

use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Salesforce API version
const API_VERSION: &str = "v59.0";

/// Salesforce REST API client
pub struct SalesforceClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Access token
    access_token: String,
    /// Instance URL (e.g. "https://na1.salesforce.com")
    instance_url: String,
}

impl SalesforceClient {
    /// Create a new Salesforce API client
    pub fn new(access_token: String, instance_url: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            access_token,
            instance_url: instance_url.trim_end_matches('/').to_string(),
        }
    }

    /// Build API URL for a resource
    fn build_url(&self, resource: &str) -> String {
        format!(
            "{}/services/data/{}/{}",
            self.instance_url, API_VERSION, resource
        )
    }

    /// Make a GET request to Salesforce API
    async fn get<T: DeserializeOwned>(&self, resource: &str) -> Result<T> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .context("Failed to send GET request to Salesforce API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce API request failed ({}): {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse Salesforce API response")
    }

    /// Make a POST request to Salesforce API
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
            .context("Failed to send POST request to Salesforce API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce API request failed ({}): {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse Salesforce API response")
    }

    /// Make a PATCH request to Salesforce API (for updates)
    async fn patch<B: Serialize>(&self, resource: &str, body: &B) -> Result<()> {
        let url = self.build_url(resource);

        let response = self
            .http_client
            .patch(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send PATCH request to Salesforce API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce API update failed ({}): {}", status, error_text);
        }

        Ok(())
    }

    /// Execute a SOQL query
    async fn query<T: DeserializeOwned>(&self, soql: &str) -> Result<SalesforceQueryResponse<T>> {
        let encoded_query = urlencoding::encode(soql);
        self.get(&format!("query?q={}", encoded_query)).await
    }

    /// Fetch next page of query results
    async fn query_more<T: DeserializeOwned>(
        &self,
        next_url: &str,
    ) -> Result<SalesforceQueryResponse<T>> {
        // next_url is a relative path like /services/data/v59.0/query/01gxx...
        let url = format!("{}{}", self.instance_url, next_url);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .context("Failed to fetch next page from Salesforce")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce query pagination failed: {}", error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse Salesforce query response")
    }

    // ──────────────────────────── Account Operations (Vendor Master) ────────────────────────────

    /// Query accounts from Salesforce (for vendor master sync)
    /// By default, fetches accounts with Type = 'Vendor' or 'Partner'
    pub async fn query_vendor_accounts(
        &self,
        custom_filter: Option<&str>,
    ) -> Result<Vec<SalesforceAccount>> {
        let where_clause = custom_filter.unwrap_or("Type IN ('Vendor', 'Partner', 'Supplier')");

        let soql = format!(
            "SELECT Id, Name, Type, Industry, Website, Phone, \
             BillingStreet, BillingCity, BillingState, BillingPostalCode, BillingCountry, \
             AnnualRevenue, NumberOfEmployees, Description, OwnerId, \
             LastModifiedDate, CreatedDate \
             FROM Account \
             WHERE {} \
             ORDER BY Name",
            where_clause
        );

        let mut all_accounts = Vec::new();
        let mut response: SalesforceQueryResponse<SalesforceAccount> = self.query(&soql).await?;
        all_accounts.extend(response.records);

        // Paginate through all results
        while !response.done {
            if let Some(next_url) = &response.next_records_url {
                response = self.query_more(next_url).await?;
                all_accounts.extend(response.records);
            } else {
                break;
            }
        }

        Ok(all_accounts)
    }

    /// Get a single account by ID
    pub async fn get_account(&self, account_id: &str) -> Result<SalesforceAccount> {
        self.get(&format!("sobjects/Account/{}", account_id)).await
    }

    /// Query all accounts (no type filter)
    pub async fn query_all_accounts(&self) -> Result<Vec<SalesforceAccount>> {
        self.query_vendor_accounts(Some("Name != null")).await
    }

    // ──────────────────────────── Contact Operations ────────────────────────────

    /// Query contacts for a given account (vendor contacts)
    pub async fn query_contacts_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<SalesforceContact>> {
        let soql = format!(
            "SELECT Id, FirstName, LastName, Email, Phone, Title, AccountId, Department \
             FROM Contact \
             WHERE AccountId = '{}' \
             ORDER BY LastName",
            account_id
        );

        let response: SalesforceQueryResponse<SalesforceContact> = self.query(&soql).await?;
        Ok(response.records)
    }

    /// Query all contacts matching vendor accounts
    pub async fn query_vendor_contacts(&self) -> Result<Vec<SalesforceContact>> {
        let soql = "SELECT Id, FirstName, LastName, Email, Phone, Title, AccountId, Department \
             FROM Contact \
             WHERE Account.Type IN ('Vendor', 'Partner', 'Supplier') \
             ORDER BY Account.Name, LastName";

        let mut all_contacts = Vec::new();
        let mut response: SalesforceQueryResponse<SalesforceContact> = self.query(soql).await?;
        all_contacts.extend(response.records);

        while !response.done {
            if let Some(next_url) = &response.next_records_url {
                response = self.query_more(next_url).await?;
                all_contacts.extend(response.records);
            } else {
                break;
            }
        }

        Ok(all_contacts)
    }

    // ──────────────────────────── Opportunity Operations (PO Linkage) ────────────────────────────

    /// Query opportunities with PO numbers for invoice matching
    pub async fn query_opportunities_with_po(
        &self,
        account_id: Option<&str>,
    ) -> Result<Vec<SalesforceOpportunity>> {
        let where_clause = if let Some(acct_id) = account_id {
            format!("AccountId = '{}' AND StageName = 'Closed Won'", acct_id)
        } else {
            "StageName = 'Closed Won'".to_string()
        };

        let soql = format!(
            "SELECT Id, Name, AccountId, StageName, Amount, CloseDate, IsWon, IsClosed \
             FROM Opportunity \
             WHERE {} \
             ORDER BY CloseDate DESC \
             LIMIT 200",
            where_clause
        );

        let response: SalesforceQueryResponse<SalesforceOpportunity> = self.query(&soql).await?;
        Ok(response.records)
    }

    // ──────────────────────────── Utility Methods ────────────────────────────

    /// Get organization info (for connection verification)
    pub async fn get_org_info(&self) -> Result<serde_json::Value> {
        self.get("sobjects").await
    }

    /// Get current user info
    pub async fn get_user_info(&self) -> Result<serde_json::Value> {
        let url = format!("{}/services/oauth2/userinfo", self.instance_url);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Failed to get Salesforce user info")?;

        response
            .json()
            .await
            .context("Failed to parse Salesforce user info")
    }

    /// Describe an SObject (get field metadata)
    pub async fn describe_object(&self, object_name: &str) -> Result<serde_json::Value> {
        self.get(&format!("sobjects/{}/describe", object_name))
            .await
    }

    /// Update a record's custom field (e.g., push payment status to Account)
    pub async fn update_account_field(
        &self,
        account_id: &str,
        field_name: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let body = serde_json::json!({ field_name: value });
        self.patch(&format!("sobjects/Account/{}", account_id), &body)
            .await
    }
}
