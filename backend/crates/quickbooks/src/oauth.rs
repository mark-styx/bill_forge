//! QuickBooks OAuth 2.0 authentication

use crate::types::QBTokens;
use anyhow::{Context, Result};

/// QuickBooks OAuth configuration
#[derive(Debug, Clone)]
pub struct QuickBooksOAuthConfig {
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// Redirect URI
    pub redirect_uri: String,
    /// Environment (production or sandbox)
    pub environment: QuickBooksEnvironment,
}

/// QuickBooks environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickBooksEnvironment {
    Production,
    Sandbox,
}

impl QuickBooksEnvironment {
    /// Get the base URL for this environment
    pub fn base_url(&self) -> &'static str {
        match self {
            QuickBooksEnvironment::Production => "https://quickbooks.api.intuit.com",
            QuickBooksEnvironment::Sandbox => "https://sandbox-quickbooks.api.intuit.com",
        }
    }

    /// Get the OAuth authorization URL
    pub fn auth_url(&self) -> &'static str {
        match self {
            QuickBooksEnvironment::Production => "https://appcenter.intuit.com/connect/oauth2",
            QuickBooksEnvironment::Sandbox => "https://appcenter.intuit.com/connect/oauth2",
        }
    }

    /// Get the OAuth token URL
    pub fn token_url(&self) -> &'static str {
        "https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer"
    }
}

/// QuickBooks OAuth client
pub struct QuickBooksOAuth {
    config: QuickBooksOAuthConfig,
    http_client: reqwest::Client,
}

impl QuickBooksOAuth {
    /// Create a new QuickBooks OAuth client
    pub fn new(config: QuickBooksOAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Generate OAuth authorization URL
    pub fn authorization_url(&self, state: &str) -> String {
        let scope = "com.intuit.quickbooks.accounting";

        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.config.environment.auth_url(),
            self.config.client_id,
            urlencoding::encode(&self.config.redirect_uri),
            scope,
            state
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        code: &str,
        realm_id: &str,
    ) -> Result<QBTokens> {
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
            .context("Failed to send token request to QuickBooks")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("QuickBooks token exchange failed: {}", error_text);
        }

        let tokens: QBTokens = response
            .json()
            .await
            .context("Failed to parse QuickBooks token response")?;

        Ok(tokens)
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<QBTokens> {
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
            .context("Failed to send refresh token request to QuickBooks")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("QuickBooks token refresh failed: {}", error_text);
        }

        let tokens: QBTokens = response
            .json()
            .await
            .context("Failed to parse QuickBooks token response")?;

        Ok(tokens)
    }

    /// Revoke tokens (disconnect)
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let revoke_url = "https://developer.api.intuit.com/v2/oauth2/tokens/revoke";

        let response = self
            .http_client
            .post(revoke_url)
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .json(&serde_json::json!({ "token": token }))
            .send()
            .await
            .context("Failed to send revoke request to QuickBooks")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("QuickBooks token revocation failed: {}", error_text);
        }

        Ok(())
    }
}

/// Generate state token for CSRF protection
pub fn generate_state_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
