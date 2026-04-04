//! REST client for EDI middleware API
//!
//! Communicates with the EDI middleware (Stedi, Orderful, etc.) to:
//! - List trading partners
//! - Check document status
//! - Send outbound documents
//! - Query transaction history

use crate::config::EdiConfig;
use crate::types::*;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// EDI middleware REST API client
pub struct EdiClient {
    http_client: reqwest::Client,
    config: EdiConfig,
}

impl EdiClient {
    /// Create a new EDI client
    pub fn new(config: EdiConfig) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            config,
        }
    }

    /// Build API URL
    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.config.api_base_url.trim_end_matches('/'), path.trim_start_matches('/'))
    }

    /// Make an authenticated GET request
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self
            .http_client
            .get(&self.url(path))
            .header("Authorization", format!("Key {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to send request to EDI middleware")?;

        let status = response.status();
        let body = response.text().await.context("Failed to read response")?;

        if !status.is_success() {
            anyhow::bail!("EDI middleware API error (HTTP {}): {}", status, body);
        }

        serde_json::from_str(&body).context("Failed to parse EDI middleware response")
    }

    /// Make an authenticated POST request
    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let response = self
            .http_client
            .post(&self.url(path))
            .header("Authorization", format!("Key {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to EDI middleware")?;

        let status = response.status();
        let resp_body = response.text().await.context("Failed to read response")?;

        if !status.is_success() {
            anyhow::bail!("EDI middleware API error (HTTP {}): {}", status, resp_body);
        }

        serde_json::from_str(&resp_body).context("Failed to parse EDI middleware response")
    }

    /// Test the connection to the EDI middleware
    pub async fn test_connection(&self) -> Result<bool> {
        let result: serde_json::Value = self.get("/health").await?;
        Ok(result.get("status").and_then(|s| s.as_str()) == Some("ok"))
    }

    /// Send a document to a trading partner via the middleware
    pub async fn send_document(&self, document: &EdiOutboundRequest) -> Result<EdiSendResult> {
        self.post("/documents/send", document).await
    }

    /// Get document status from the middleware
    pub async fn get_document_status(&self, middleware_id: &str) -> Result<EdiDocumentStatusResponse> {
        self.get(&format!("/documents/{}", middleware_id)).await
    }
}

/// Request to send an outbound EDI document
#[derive(Debug, Clone, Serialize)]
pub struct EdiOutboundRequest {
    pub document_type: EdiDocumentType,
    pub receiver_id: String,
    pub payload: serde_json::Value,
}

/// Response after sending a document
#[derive(Debug, Clone, Deserialize)]
pub struct EdiSendResult {
    pub middleware_id: String,
    pub status: String,
}

/// Document status response from middleware
#[derive(Debug, Clone, Deserialize)]
pub struct EdiDocumentStatusResponse {
    pub id: String,
    pub status: String,
    pub ack_status: Option<String>,
    pub error: Option<String>,
}
