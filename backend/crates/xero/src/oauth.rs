//! Xero OAuth 2.0 authentication

use crate::types::{XeroTenant, XeroTokens};
use anyhow::{Context, Result};

/// Xero OAuth configuration
#[derive(Debug, Clone)]
pub struct XeroOAuthConfig {
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// Redirect URI
    pub redirect_uri: String,
    /// Environment (production or sandbox)
    pub environment: XeroEnvironment,
}

/// Xero environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XeroEnvironment {
    Production,
    Sandbox,
}

impl XeroEnvironment {
    /// Get the base URL for this environment
    pub fn base_url(&self) -> &'static str {
        match self {
            // Xero uses the same API URL for both, but different app credentials
            XeroEnvironment::Production => "https://api.xero.com",
            XeroEnvironment::Sandbox => "https://api.xero.com",
        }
    }

    /// Get the OAuth authorization URL
    pub fn auth_url(&self) -> &'static str {
        "https://login.xero.com/identity/connect/authorize"
    }

    /// Get the OAuth token URL
    pub fn token_url(&self) -> &'static str {
        "https://identity.xero.com/connect/token"
    }

    /// Get the connections URL (to get tenant info)
    pub fn connections_url(&self) -> &'static str {
        "https://api.xero.com/connections"
    }
}

/// Xero OAuth client
pub struct XeroOAuth {
    config: XeroOAuthConfig,
    http_client: reqwest::Client,
}

impl XeroOAuth {
    /// Create a new Xero OAuth client
    pub fn new(config: XeroOAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Generate OAuth authorization URL
    pub fn authorization_url(&self, state: &str) -> String {
        // Xero scopes for accounting API
        let scope = "openid profile email accounting.transactions accounting.contacts accounting.settings offline_access";

        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.config.environment.auth_url(),
            self.config.client_id,
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(scope),
            state
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str) -> Result<XeroTokens> {
        let response = self
            .http_client
            .post(self.config.environment.token_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &self.config.redirect_uri),
            ])
            .send()
            .await
            .context("Failed to send token request to Xero")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero token exchange failed: {}", error_text);
        }

        let tokens: XeroTokens = response
            .json()
            .await
            .context("Failed to parse Xero token response")?;

        Ok(tokens)
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<XeroTokens> {
        let response = self
            .http_client
            .post(self.config.environment.token_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("Failed to send refresh token request to Xero")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero token refresh failed: {}", error_text);
        }

        let tokens: XeroTokens = response
            .json()
            .await
            .context("Failed to parse Xero token response")?;

        Ok(tokens)
    }

    /// Revoke tokens (disconnect)
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let revoke_url = "https://identity.xero.com/connect/revocation";

        let response = self
            .http_client
            .post(revoke_url)
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("token", token)])
            .send()
            .await
            .context("Failed to send revoke request to Xero")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero token revocation failed: {}", error_text);
        }

        Ok(())
    }

    /// Get connected tenants (organizations)
    pub async fn get_connections(&self, access_token: &str) -> Result<Vec<XeroTenant>> {
        let response = self
            .http_client
            .get(self.config.environment.connections_url())
            .bearer_auth(access_token)
            .send()
            .await
            .context("Failed to get Xero connections")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Xero get connections failed: {}", error_text);
        }

        let tenants: Vec<XeroTenant> = response
            .json()
            .await
            .context("Failed to parse Xero connections response")?;

        Ok(tenants)
    }
}

/// Generate state token for CSRF protection
pub fn generate_state_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
