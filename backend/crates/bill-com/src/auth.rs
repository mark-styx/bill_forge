//! Bill.com session-based authentication
//!
//! Bill.com uses session-based auth with developer keys:
//! 1. POST /v3/login with devKey + orgId + userName + password
//! 2. Receive sessionId in JSON response
//! 3. Include devKey + sessionId headers in all subsequent requests
//! Sessions are long-lived but can be explicitly logged out.

use crate::types::BillComSession;
use anyhow::{Context, Result};

/// Bill.com environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillComEnvironment {
    Production,
    Sandbox,
}

impl BillComEnvironment {
    /// Get the base URL for this environment
    pub fn base_url(&self) -> &'static str {
        match self {
            BillComEnvironment::Production => "https://gateway.bill.com/connect/v3",
            BillComEnvironment::Sandbox => "https://gateway.stage.bill.com/connect/v3",
        }
    }
}

/// Bill.com authentication configuration
#[derive(Debug, Clone)]
pub struct BillComAuthConfig {
    /// Developer key (issued by Bill.com)
    pub dev_key: String,
    /// Organization ID
    pub org_id: String,
    /// User name (email)
    pub user_name: String,
    /// User password
    pub password: String,
    /// Environment (production or sandbox)
    pub environment: BillComEnvironment,
}

/// Bill.com authentication client
pub struct BillComAuth {
    config: BillComAuthConfig,
    http_client: reqwest::Client,
}

impl BillComAuth {
    /// Create a new Bill.com auth client
    pub fn new(config: BillComAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Login to Bill.com and establish a session
    pub async fn login(&self) -> Result<BillComSession> {
        let url = format!("{}/login", self.config.environment.base_url());

        let body = serde_json::json!({
            "devKey": self.config.dev_key,
            "orgId": self.config.org_id,
            "userName": self.config.user_name,
            "password": self.config.password,
        });

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send login request to Bill.com")?;

        let status = response.status();
        let response_body = response
            .text()
            .await
            .context("Failed to read Bill.com login response")?;

        if !status.is_success() {
            anyhow::bail!("Bill.com login failed (HTTP {}): {}", status, response_body);
        }

        let parsed: serde_json::Value = serde_json::from_str(&response_body)
            .context("Failed to parse Bill.com login response")?;

        let session_id = parsed["sessionId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing sessionId in Bill.com login response"))?
            .to_string();

        let org_id = parsed["orgId"]
            .as_str()
            .unwrap_or(&self.config.org_id)
            .to_string();

        let user_id = parsed["userId"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        tracing::info!(
            org_id = %org_id,
            "Bill.com session established"
        );

        Ok(BillComSession {
            session_id,
            org_id,
            user_id,
        })
    }

    /// Logout and invalidate the session
    pub async fn logout(&self, session_id: &str) -> Result<()> {
        let url = format!("{}/logout", self.config.environment.base_url());

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("devKey", &self.config.dev_key)
            .header("sessionId", session_id)
            .send()
            .await
            .context("Failed to send logout request to Bill.com")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Bill.com logout response")?;

        if !status.is_success() {
            anyhow::bail!("Bill.com logout failed (HTTP {}): {}", status, body);
        }

        tracing::info!("Bill.com session terminated");

        Ok(())
    }

    /// Test connection by attempting a login
    pub async fn test_connection(&self) -> Result<bool> {
        match self.login().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(error = %e, "Bill.com connection test failed");
                Ok(false)
            }
        }
    }
}
