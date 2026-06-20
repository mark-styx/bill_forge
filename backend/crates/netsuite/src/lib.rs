//! NetSuite REST API integration service
//!
//! The NetSuite connect path is intentionally disabled: `authenticate()` returns
//! the typed `ClientError::JwtNotImplemented` variant instead of attempting any
//! HTTP exchange. Real NetSuite OAuth 2.0 M2M requires JWT client_assertion
//! signing (RS256 against a NetSuite-issued private key), which is tracked as
//! separate work. The crate keeps `NetSuiteClient`, the credentials/vendor wire
//! types, and the vendor-bill request shape so the API layer, connections table,
//! and read paths can remain wired without pretending a real auth flow exists.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// NetSuite integration configuration.
#[derive(Debug, Clone)]
pub struct NetSuiteConfig {
    /// NetSuite account ID (e.g. "TSTDRV1234567").
    pub account_id: String,
    /// OAuth 2.0 client ID.
    pub client_id: String,
    /// OAuth 2.0 client secret.
    pub client_secret: String,
    /// Optional base URL override (useful for tests pointing at a mock server).
    pub base_url: Option<String>,
}

impl NetSuiteConfig {
    /// Build the default NetSuite REST API base URL from the account ID.
    ///
    /// Returns the explicit `base_url` override if set, otherwise constructs
    /// `https://{account_id}.suitetalk.api.netsuite.com`.
    pub fn base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| format!("https://{}.suitetalk.api.netsuite.com", self.account_id))
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Deserialization failed: {0}")]
    Deserialization(String),

    #[error("Missing access token – call authenticate() first")]
    MissingToken,

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error(
        "NetSuite OAuth 2.0 M2M requires JWT client_assertion signing, which is not yet implemented. \
         Connect is disabled until JWT support ships."
    )]
    JwtNotImplemented,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Async client for the NetSuite REST API.
pub struct NetSuiteClient {
    http: reqwest::Client,
    config: NetSuiteConfig,
    access_token: Option<String>,
}

impl NetSuiteClient {
    /// Create a new unauthenticated client.
    pub fn new(config: NetSuiteConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            access_token: None,
        }
    }

    /// Authenticate against NetSuite, intentionally unimplemented.
    ///
    /// Returns `ClientError::JwtNotImplemented` without making any network
    /// request. Real NetSuite OAuth 2.0 M2M requires JWT `client_assertion`
    /// signing (RS256 against a NetSuite-issued private key), which is tracked
    /// as separate work. Callers should treat this as a hard failure surface
    /// rather than a transient error.
    pub async fn authenticate(&mut self) -> Result<(), ClientError> {
        Err(ClientError::JwtNotImplemented)
    }

    /// List vendors from NetSuite.
    ///
    /// GETs `/services/rest/record/v1/vendor?limit=100` and returns the
    /// deserialized vendor records. Requires `authenticate()` to have been
    /// called first.
    pub async fn list_vendors(&self) -> Result<Vec<NetSuiteVendor>, ClientError> {
        let token = self
            .access_token
            .as_ref()
            .ok_or(ClientError::MissingToken)?;

        let url = format!(
            "{}/services/rest/record/v1/vendor?limit=100",
            self.config.base_url()
        );

        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ClientError::Http(format!(
                "list_vendors returned {}: {}",
                status, text
            )));
        }

        let vendor_resp: VendorListResponse = resp
            .json()
            .await
            .map_err(|e| ClientError::Deserialization(e.to_string()))?;

        Ok(vendor_resp.items)
    }

    /// Create (POST) a vendor bill in NetSuite.
    ///
    /// Validates the request locally, then POSTs to
    /// `/services/rest/record/v1/vendorBill`. Returns the created record id
    /// parsed from the JSON body, falling back to the `Location` header.
    pub async fn create_vendor_bill(
        &self,
        req: NetSuiteVendorBillRequest,
    ) -> Result<NetSuiteVendorBillResponse, ClientError> {
        if req.item_list.is_empty() {
            return Err(ClientError::Validation(
                "item_list must not be empty".to_string(),
            ));
        }
        for (i, line) in req.item_list.iter().enumerate() {
            if line.amount < 0.0 {
                return Err(ClientError::Validation(format!(
                    "line item {} has negative amount ({})",
                    i, line.amount
                )));
            }
        }

        let token = self
            .access_token
            .as_ref()
            .ok_or(ClientError::MissingToken)?;

        let url = format!(
            "{}/services/rest/record/v1/vendorBill",
            self.config.base_url()
        );

        let body =
            serde_json::to_string(&req).map_err(|e| ClientError::Deserialization(e.to_string()))?;

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ClientError::Http(format!(
                "create_vendor_bill returned {}: {}",
                status, text
            )));
        }

        // Save Location header before consuming the body.
        let location_id = resp
            .headers()
            .get("Location")
            .and_then(|v| v.to_str().ok())
            .and_then(|loc| loc.rsplit('/').next())
            .map(|s| s.to_string());

        let resp_text = resp.text().await.unwrap_or_default();

        // Try to parse id from JSON body first.
        if let Ok(parsed) = serde_json::from_str::<NetSuiteVendorBillResponse>(&resp_text) {
            return Ok(parsed);
        }

        // Fallback: extract id from Location header.
        if let Some(id) = location_id {
            return Ok(NetSuiteVendorBillResponse { id });
        }

        Err(ClientError::Deserialization(
            "could not extract vendor bill id from response".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// A vendor record from the NetSuite REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetSuiteVendor {
    pub id: String,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VendorListResponse {
    items: Vec<NetSuiteVendor>,
}

// ---------------------------------------------------------------------------
// Vendor Bill wire types
// ---------------------------------------------------------------------------

/// A reference to a NetSuite record by id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetSuiteRecordRef {
    pub id: String,
}

/// A single line item on a vendor bill.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetSuiteBillLineItem {
    pub amount: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<NetSuiteRecordRef>,
}

/// Request body for creating a vendor bill in NetSuite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetSuiteVendorBillRequest {
    pub entity: NetSuiteRecordRef,
    pub tran_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub item_list: Vec<NetSuiteBillLineItem>,
}

/// Response from creating a vendor bill in NetSuite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetSuiteVendorBillResponse {
    pub id: String,
}
