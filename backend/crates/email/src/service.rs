//! Email service implementation using HTTP-based email APIs
//!
//! Supports multiple providers:
//! - Sendgrid (set EMAIL_PROVIDER=sendgrid, SENDGRID_API_KEY=...)
//! - Mailgun (set EMAIL_PROVIDER=mailgun, MAILGUN_API_KEY=..., MAILGUN_DOMAIN=...)
//! - Webhook (set EMAIL_PROVIDER=webhook, EMAIL_WEBHOOK_URL=...)
//! - Log/Mock (default for development)

use async_trait::async_trait;
use billforge_core::{Error, Result};
use serde::Serialize;

/// Email service configuration
#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub provider: EmailProvider,
    pub from_email: String,
    pub from_name: String,
    pub enabled: bool,
}

/// Supported email providers
#[derive(Debug, Clone)]
pub enum EmailProvider {
    /// SendGrid API
    Sendgrid { api_key: String },
    /// Mailgun API
    Mailgun { api_key: String, domain: String },
    /// Custom webhook endpoint
    Webhook { url: String, auth_header: Option<String> },
    /// Log only (for development/testing)
    Log,
}

impl EmailConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let provider_str = std::env::var("EMAIL_PROVIDER").unwrap_or_else(|_| "log".to_string());

        let provider = match provider_str.to_lowercase().as_str() {
            "sendgrid" => {
                let api_key = std::env::var("SENDGRID_API_KEY").ok()?;
                EmailProvider::Sendgrid { api_key }
            }
            "mailgun" => {
                let api_key = std::env::var("MAILGUN_API_KEY").ok()?;
                let domain = std::env::var("MAILGUN_DOMAIN").ok()?;
                EmailProvider::Mailgun { api_key, domain }
            }
            "webhook" => {
                let url = std::env::var("EMAIL_WEBHOOK_URL").ok()?;
                let auth_header = std::env::var("EMAIL_WEBHOOK_AUTH").ok();
                EmailProvider::Webhook { url, auth_header }
            }
            _ => EmailProvider::Log,
        };

        Some(Self {
            provider,
            from_email: std::env::var("EMAIL_FROM")
                .unwrap_or_else(|_| "noreply@billforge.app".to_string()),
            from_name: std::env::var("EMAIL_FROM_NAME")
                .unwrap_or_else(|_| "BillForge".to_string()),
            enabled: std::env::var("EMAIL_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
        })
    }
}

/// Email service trait for sending emails
#[async_trait]
pub trait EmailService: Send + Sync {
    /// Send an email
    async fn send(&self, to: &str, subject: &str, html_body: &str, text_body: &str) -> Result<()>;

    /// Check if email service is enabled
    fn is_enabled(&self) -> bool;
}

/// HTTP-based email service implementation
pub struct EmailServiceImpl {
    config: EmailConfig,
    client: reqwest::Client,
}

impl EmailServiceImpl {
    /// Create a new email service
    pub fn new(config: EmailConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::ExternalService {
                service: "Email".to_string(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self { config, client })
    }
}

#[derive(Serialize)]
struct SendgridEmail {
    personalizations: Vec<SendgridPersonalization>,
    from: SendgridAddress,
    subject: String,
    content: Vec<SendgridContent>,
}

#[derive(Serialize)]
struct SendgridPersonalization {
    to: Vec<SendgridAddress>,
}

#[derive(Serialize)]
struct SendgridAddress {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct SendgridContent {
    #[serde(rename = "type")]
    content_type: String,
    value: String,
}

#[derive(Serialize)]
struct WebhookPayload {
    to: String,
    from_email: String,
    from_name: String,
    subject: String,
    html_body: String,
    text_body: String,
}

#[async_trait]
impl EmailService for EmailServiceImpl {
    async fn send(&self, to: &str, subject: &str, html_body: &str, text_body: &str) -> Result<()> {
        if !self.config.enabled {
            tracing::debug!("Email service disabled, skipping email to {}", to);
            return Ok(());
        }

        match &self.config.provider {
            EmailProvider::Sendgrid { api_key } => {
                let email = SendgridEmail {
                    personalizations: vec![SendgridPersonalization {
                        to: vec![SendgridAddress {
                            email: to.to_string(),
                            name: None,
                        }],
                    }],
                    from: SendgridAddress {
                        email: self.config.from_email.clone(),
                        name: Some(self.config.from_name.clone()),
                    },
                    subject: subject.to_string(),
                    content: vec![
                        SendgridContent {
                            content_type: "text/plain".to_string(),
                            value: text_body.to_string(),
                        },
                        SendgridContent {
                            content_type: "text/html".to_string(),
                            value: html_body.to_string(),
                        },
                    ],
                };

                let response = self
                    .client
                    .post("https://api.sendgrid.com/v3/mail/send")
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&email)
                    .send()
                    .await
                    .map_err(|e| Error::ExternalService {
                        service: "SendGrid".to_string(),
                        message: format!("Failed to send request: {}", e),
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::ExternalService {
                        service: "SendGrid".to_string(),
                        message: format!("API error {}: {}", status, body),
                    });
                }

                tracing::info!(to = %to, subject = %subject, provider = "sendgrid", "Email sent successfully");
            }

            EmailProvider::Mailgun { api_key, domain } => {
                let url = format!("https://api.mailgun.net/v3/{}/messages", domain);

                let response = self
                    .client
                    .post(&url)
                    .basic_auth("api", Some(api_key))
                    .form(&[
                        ("from", format!("{} <{}>", self.config.from_name, self.config.from_email)),
                        ("to", to.to_string()),
                        ("subject", subject.to_string()),
                        ("text", text_body.to_string()),
                        ("html", html_body.to_string()),
                    ])
                    .send()
                    .await
                    .map_err(|e| Error::ExternalService {
                        service: "Mailgun".to_string(),
                        message: format!("Failed to send request: {}", e),
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::ExternalService {
                        service: "Mailgun".to_string(),
                        message: format!("API error {}: {}", status, body),
                    });
                }

                tracing::info!(to = %to, subject = %subject, provider = "mailgun", "Email sent successfully");
            }

            EmailProvider::Webhook { url, auth_header } => {
                let payload = WebhookPayload {
                    to: to.to_string(),
                    from_email: self.config.from_email.clone(),
                    from_name: self.config.from_name.clone(),
                    subject: subject.to_string(),
                    html_body: html_body.to_string(),
                    text_body: text_body.to_string(),
                };

                let mut request = self
                    .client
                    .post(url)
                    .header("Content-Type", "application/json")
                    .json(&payload);

                if let Some(auth) = auth_header {
                    request = request.header("Authorization", auth);
                }

                let response = request.send().await.map_err(|e| Error::ExternalService {
                    service: "EmailWebhook".to_string(),
                    message: format!("Failed to send request: {}", e),
                })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::ExternalService {
                        service: "EmailWebhook".to_string(),
                        message: format!("Webhook error {}: {}", status, body),
                    });
                }

                tracing::info!(to = %to, subject = %subject, provider = "webhook", "Email sent successfully");
            }

            EmailProvider::Log => {
                tracing::info!(
                    to = %to,
                    subject = %subject,
                    from = %self.config.from_email,
                    "[LOG] Email would be sent (logging mode)"
                );
            }
        }

        Ok(())
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Mock email service for testing
pub struct MockEmailService;

impl MockEmailService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockEmailService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailService for MockEmailService {
    async fn send(&self, to: &str, subject: &str, _html_body: &str, _text_body: &str) -> Result<()> {
        tracing::info!(
            to = %to,
            subject = %subject,
            "[MOCK] Email would be sent"
        );
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_email_service() {
        let service = MockEmailService::new();
        let result = service
            .send("test@example.com", "Test Subject", "<p>Hello</p>", "Hello")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_provider() {
        let config = EmailConfig {
            provider: EmailProvider::Log,
            from_email: "test@test.com".to_string(),
            from_name: "Test".to_string(),
            enabled: true,
        };
        let service = EmailServiceImpl::new(config).unwrap();
        let result = service
            .send("test@example.com", "Test Subject", "<p>Hello</p>", "Hello")
            .await;
        assert!(result.is_ok());
    }
}
