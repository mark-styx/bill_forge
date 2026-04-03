//! Workday OAuth 2.0 authentication
//!
//! Workday uses OAuth 2.0 (Authorization Code Grant) with Bearer tokens:
//! 1. Register API Client in Workday → Client ID + Client Secret
//! 2. User authorizes → Authorization Code
//! 3. Exchange code for Access Token + Refresh Token
//! 4. Use Access Token as Bearer token for REST API calls
//!
//! Token URL pattern: `https://{tenant_url}/ccx/oauth2/{tenant_name}/token`
//! Auth URL pattern: `https://{tenant_url}/authorize`

use crate::types::WorkdayTokens;
use anyhow::{Context, Result};

/// Workday OAuth configuration
#[derive(Debug, Clone)]
pub struct WorkdayOAuthConfig {
    /// OAuth client ID (from Workday API Client registration)
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// Refresh token (long-lived, used to obtain access tokens)
    pub refresh_token: String,
    /// Tenant URL (e.g. "https://impl.workday.com")
    pub tenant_url: String,
    /// Tenant name (e.g. "acme_corp")
    pub tenant_name: String,
}

/// Workday OAuth client
pub struct WorkdayOAuth {
    config: WorkdayOAuthConfig,
    http_client: reqwest::Client,
}

impl WorkdayOAuth {
    /// Create a new Workday OAuth client
    pub fn new(config: WorkdayOAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Get the OAuth token endpoint URL
    fn token_url(&self) -> String {
        format!(
            "{}/ccx/oauth2/{}/token",
            self.config.tenant_url, self.config.tenant_name
        )
    }

    /// Get the OAuth authorization endpoint URL
    fn auth_url(&self) -> String {
        format!("{}/authorize", self.config.tenant_url)
    }

    /// Get the OAuth revoke endpoint URL
    fn revoke_url(&self) -> String {
        format!(
            "{}/ccx/oauth2/{}/revoke",
            self.config.tenant_url, self.config.tenant_name
        )
    }

    /// Generate OAuth authorization URL
    pub fn authorization_url(&self, state: &str) -> String {
        let scope = "Financial_Management";

        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.auth_url(),
            self.config.client_id,
            urlencoding::encode("https://localhost/callback"),
            scope,
            state
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str) -> Result<WorkdayTokens> {
        let response = self
            .http_client
            .post(&self.token_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("grant_type", "authorization_code"), ("code", code)])
            .send()
            .await
            .context("Failed to send token request to Workday")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Workday token response")?;

        if !status.is_success() {
            anyhow::bail!("Workday token exchange failed: {}", body);
        }

        let tokens: WorkdayTokens =
            serde_json::from_str(&body).context("Failed to parse Workday token response")?;

        Ok(tokens)
    }

    /// Refresh access token using a refresh token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<WorkdayTokens> {
        let response = self
            .http_client
            .post(&self.token_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("Failed to send refresh token request to Workday")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Workday refresh token response")?;

        if !status.is_success() {
            anyhow::bail!("Workday token refresh failed: {}", body);
        }

        let tokens: WorkdayTokens =
            serde_json::from_str(&body).context("Failed to parse Workday token response")?;

        Ok(tokens)
    }

    /// Revoke a token (access or refresh)
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let response = self
            .http_client
            .post(&self.revoke_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("token", token)])
            .send()
            .await
            .context("Failed to send revoke request to Workday")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Workday revoke response")?;

        if !status.is_success() {
            anyhow::bail!("Workday token revocation failed: {}", body);
        }

        Ok(())
    }
}

/// Generate state token for CSRF protection
pub fn generate_state_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
