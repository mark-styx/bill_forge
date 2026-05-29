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
use billforge_core::http_retry::{self, RetryConfig};
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::sleep;

/// Salesforce API version
const API_VERSION: &str = "v59.0";

/// Validate a Salesforce record ID: 15 or 18 alphanumeric chars, nothing else.
/// Rejects quotes, whitespace, SOQL metacharacters.
pub fn validate_sf_id(id: &str) -> Result<&str> {
    let len = id.len();
    if (len == 15 || len == 18) && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        anyhow::bail!("invalid Salesforce ID: {:?}", id)
    }
}

/// Escape a string literal for inclusion in a SOQL single-quoted value.
/// Per Salesforce docs: backslash-escape `\`, `'`, `"`, newline, carriage return.
pub fn escape_soql_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Validate a custom SOQL filter fragment for obvious injection patterns.
/// Rejects strings containing `;`, `--`, or unescaped single quotes.
fn validate_custom_filter(filter: &str) -> Result<()> {
    if filter.contains(';') {
        anyhow::bail!("custom_filter contains semicolon: rejected");
    }
    if filter.contains("--") {
        anyhow::bail!("custom_filter contains comment marker '--': rejected");
    }
    // Check for unescaped single quotes: a bare ' not preceded by backslash
    let mut prev_was_backslash = false;
    for c in filter.chars() {
        if c == '\'' && !prev_was_backslash {
            anyhow::bail!("custom_filter contains unescaped single quote: rejected");
        }
        prev_was_backslash = c == '\\';
    }
    Ok(())
}

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
                        tracing::warn!(attempt, error = %err, "Salesforce transport error, retrying once");
                        attempt += 1;
                        continue;
                    }
                    anyhow::bail!("Salesforce transport error after retry: {}", err);
                }
            };

            let status = response.status();
            let status_code = status.as_u16();

            if status.is_success() {
                return Ok(response);
            }

            if http_retry::is_retryable_status(status_code) {
                let retry_after = http_retry::parse_retry_after(
                    response
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok()),
                );
                attempt += 1;
                if attempt >= config.max_retries {
                    let body = response.text().await.unwrap_or_default();
                    anyhow::bail!(
                        "Salesforce API request failed after {} retries ({}): {}",
                        attempt,
                        status_code,
                        body
                    );
                }
                let backoff = http_retry::compute_backoff(&config, attempt, retry_after);
                tracing::warn!(
                    attempt,
                    status_code,
                    ?backoff,
                    "Salesforce retryable error, backing off"
                );
                sleep(backoff).await;
                continue;
            }

            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce API request failed ({}): {}", status, error_text);
        }
    }

    /// Make a GET request to Salesforce API
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

        response
            .json()
            .await
            .context("Failed to parse Salesforce API response")
    }

    /// Make a POST request to Salesforce API
    async fn _post<T: DeserializeOwned, B: Serialize>(
        &self,
        resource: &str,
        body: &B,
    ) -> Result<T> {
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

        response
            .json()
            .await
            .context("Failed to parse Salesforce API response")
    }

    /// Make a PATCH request to Salesforce API (for updates)
    async fn patch<B: Serialize>(&self, resource: &str, body: &B) -> Result<()> {
        let url = self.build_url(resource);
        let body_bytes = serde_json::to_vec(body).context("Failed to serialize PATCH body")?;

        self.send_with_retry(|| {
            self.http_client
                .patch(&url)
                .bearer_auth(&self.access_token)
                .header(CONTENT_TYPE, "application/json")
                .body(reqwest::Body::from(body_bytes.clone()))
        })
        .await?;

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
            .send_with_retry(|| {
                self.http_client
                    .get(&url)
                    .bearer_auth(&self.access_token)
                    .header(CONTENT_TYPE, "application/json")
            })
            .await?;

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
        let where_clause = if let Some(filter) = custom_filter {
            validate_custom_filter(filter)?;
            filter
        } else {
            "Type IN ('Vendor', 'Partner', 'Supplier')"
        };

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
        validate_sf_id(account_id)?;
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
        validate_sf_id(account_id)?;
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
            validate_sf_id(acct_id)?;
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
            .send_with_retry(|| self.http_client.get(&url).bearer_auth(&self.access_token))
            .await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── validate_sf_id ───

    #[test]
    fn accepts_15_char_id() {
        let id = "001A000000BcdEf";
        assert_eq!(validate_sf_id(id).unwrap(), id);
    }

    #[test]
    fn accepts_18_char_id() {
        let id = "001A000000BcdEfGHI";
        assert_eq!(validate_sf_id(id).unwrap(), id);
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_sf_id("").is_err());
    }

    #[test]
    fn rejects_14_chars() {
        assert!(validate_sf_id("001A000000BcdE").is_err());
    }

    #[test]
    fn rejects_16_chars() {
        assert!(validate_sf_id("001A000000BcdEf0").is_err());
    }

    #[test]
    fn rejects_id_with_single_quote() {
        assert!(validate_sf_id("001A00000'BCdEf").is_err());
    }

    #[test]
    fn rejects_id_with_space() {
        assert!(validate_sf_id("001A000000 cdEf").is_err());
    }

    #[test]
    fn rejects_id_with_double_dash() {
        assert!(validate_sf_id("001A00000--cdEf").is_err());
    }

    #[test]
    fn rejects_soql_injection_payload() {
        // Exact shape from the issue: ' OR '1'='1
        let payload = "001000000000001' OR '1'='1";
        assert!(validate_sf_id(payload).is_err());
    }

    // ─── escape_soql_literal ───

    #[test]
    fn escape_soql_literal_backslash() {
        assert_eq!(escape_soql_literal(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_soql_literal_single_quote() {
        assert_eq!(escape_soql_literal("a'b"), r"a\'b");
    }

    #[test]
    fn escape_soql_literal_double_quote() {
        assert_eq!(escape_soql_literal(r#"a"b"#), r#"a\"b"#);
    }

    #[test]
    fn escape_soql_literal_newline() {
        assert_eq!(escape_soql_literal("a\nb"), r"a\nb");
    }

    #[test]
    fn escape_soql_literal_no_escape_needed() {
        assert_eq!(escape_soql_literal("hello"), "hello");
    }

    // ─── validate_custom_filter ───

    #[test]
    fn filter_allows_normal_clause() {
        assert!(validate_custom_filter("Name != null").is_ok());
    }

    #[test]
    fn filter_rejects_semicolon() {
        assert!(validate_custom_filter("Name != null; DROP TABLE").is_err());
    }

    #[test]
    fn filter_rejects_double_dash() {
        assert!(validate_custom_filter("Name != null -- comment").is_err());
    }

    #[test]
    fn filter_rejects_unescaped_quote() {
        assert!(validate_custom_filter("Name = 'evil").is_err());
    }

    // ─── SalesforceClient constructor ───

    #[test]
    fn constructor_trims_trailing_slash() {
        let client = SalesforceClient::new("token".into(), "https://na1.salesforce.com/".into());
        assert_eq!(client.instance_url, "https://na1.salesforce.com");
        assert_eq!(client.access_token, "token");
    }

    #[test]
    fn constructor_keeps_url_without_trailing_slash() {
        let client = SalesforceClient::new("token".into(), "https://na1.salesforce.com".into());
        assert_eq!(client.instance_url, "https://na1.salesforce.com");
    }

    #[test]
    fn build_url_formats_correctly() {
        let client = SalesforceClient::new("tok".into(), "https://na1.salesforce.com".into());
        let url = client.build_url("sobjects/Account");
        assert_eq!(
            url,
            "https://na1.salesforce.com/services/data/v59.0/sobjects/Account"
        );
    }
}
