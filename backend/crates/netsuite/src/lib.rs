//! NetSuite REST API integration service
//!
//! Provides OAuth 2.0 M2M authentication and API client for NetSuite:
//! - OAuth 2.0 client_credentials token exchange
//! - Vendor list (NetSuite REST record API)
//!
//! NOTE: Real NetSuite OAuth 2.0 M2M requires JWT client assertion; this scaffold
//! models the simpler client_credentials shape to establish the crate skeleton.
//! A follow-up ticket should add JWT signing against actual NetSuite sandbox credentials.

#![allow(dead_code)]

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

    /// Exchange client credentials for an access token via OAuth 2.0 M2M.
    ///
    /// POSTs to `{base}/services/rest/auth/oauth2/v1/token` with
    /// `grant_type=client_credentials` and stores the resulting token.
    pub async fn authenticate(&mut self) -> Result<(), ClientError> {
        let url = format!(
            "{}/services/rest/auth/oauth2/v1/token",
            self.config.base_url()
        );

        let body = format!(
            "grant_type=client_credentials&client_id={}&client_secret={}",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.client_secret),
        );

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ClientError::Auth(format!(
                "token request returned {}: {}",
                status, text
            )));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| ClientError::Deserialization(e.to_string()))?;

        self.access_token = Some(token_resp.access_token);
        Ok(())
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

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}
