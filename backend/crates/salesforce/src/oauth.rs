//! Salesforce OAuth 2.0 authentication (Web Server flow)
//!
//! Salesforce uses standard OAuth 2.0 with these endpoints:
//! - Authorization: https://login.salesforce.com/services/oauth2/authorize
//! - Token: https://login.salesforce.com/services/oauth2/token
//! - Revoke: https://login.salesforce.com/services/oauth2/revoke
//!
//! Scopes: api, refresh_token, offline_access

use crate::types::SalesforceTokens;
use anyhow::{Context, Result};

/// Salesforce OAuth configuration
#[derive(Debug, Clone)]
pub struct SalesforceOAuthConfig {
    /// Connected App client ID (Consumer Key)
    pub client_id: String,
    /// Connected App client secret (Consumer Secret)
    pub client_secret: String,
    /// OAuth redirect URI
    pub redirect_uri: String,
    /// Environment (production or sandbox)
    pub environment: SalesforceEnvironment,
}

/// Salesforce environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesforceEnvironment {
    Production,
    Sandbox,
}

impl SalesforceEnvironment {
    /// Get the login URL for this environment
    pub fn login_url(&self) -> &'static str {
        match self {
            SalesforceEnvironment::Production => "https://login.salesforce.com",
            SalesforceEnvironment::Sandbox => "https://test.salesforce.com",
        }
    }

    /// Get the OAuth authorization URL
    pub fn auth_url(&self) -> String {
        format!("{}/services/oauth2/authorize", self.login_url())
    }

    /// Get the OAuth token URL
    pub fn token_url(&self) -> String {
        format!("{}/services/oauth2/token", self.login_url())
    }

    /// Get the OAuth revoke URL
    pub fn revoke_url(&self) -> String {
        format!("{}/services/oauth2/revoke", self.login_url())
    }
}

/// Salesforce OAuth client
pub struct SalesforceOAuth {
    config: SalesforceOAuthConfig,
    http_client: reqwest::Client,
}

impl SalesforceOAuth {
    /// Create a new Salesforce OAuth client
    pub fn new(config: SalesforceOAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Generate OAuth authorization URL
    pub fn authorization_url(&self, state: &str) -> String {
        let scope = "api refresh_token offline_access";

        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&prompt=consent",
            self.config.environment.auth_url(),
            self.config.client_id,
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(scope),
            state
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str) -> Result<SalesforceTokens> {
        let response = self
            .http_client
            .post(self.config.environment.token_url())
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("redirect_uri", &self.config.redirect_uri),
            ])
            .send()
            .await
            .context("Failed to send token request to Salesforce")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce token exchange failed: {}", error_text);
        }

        let tokens: SalesforceTokens = response
            .json()
            .await
            .context("Failed to parse Salesforce token response")?;

        Ok(tokens)
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<SalesforceTokens> {
        let response = self
            .http_client
            .post(self.config.environment.token_url())
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
            ])
            .send()
            .await
            .context("Failed to send refresh token request to Salesforce")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce token refresh failed: {}", error_text);
        }

        let tokens: SalesforceTokens = response
            .json()
            .await
            .context("Failed to parse Salesforce token response")?;

        Ok(tokens)
    }

    /// Revoke tokens (disconnect)
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let response = self
            .http_client
            .post(self.config.environment.revoke_url())
            .form(&[("token", token)])
            .send()
            .await
            .context("Failed to send revoke request to Salesforce")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Salesforce token revocation failed: {}", error_text);
        }

        Ok(())
    }
}

/// Generate state token for CSRF protection
pub fn generate_state_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
