//! Firebase Cloud Messaging (FCM) client for Android push notifications

use crate::{PushError, PushNotificationProvider, PushResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// FCM configuration
#[derive(Debug, Clone)]
pub struct FcmConfig {
    pub api_key: String,
}

/// FCM client
#[derive(Debug)]
pub struct FcmClient {
    config: FcmConfig,
    client: Client,
    endpoint: String,
}

impl FcmClient {
    /// Create a new FCM client
    pub fn new(config: FcmConfig) -> Result<Self, FcmError> {
        if config.api_key.is_empty() {
            return Err(FcmError::InvalidConfig(
                "API key cannot be empty".to_string(),
            ));
        }

        Ok(Self {
            config,
            client: Client::new(),
            endpoint: "https://fcm.googleapis.com/fcm/send".to_string(),
        })
    }

    /// Send a notification to a specific device
    async fn send_notification(
        &self,
        device_token: &str,
        notification: &FcmNotification,
    ) -> Result<FcmResponse, FcmError> {
        let message = FcmMessage {
            to: device_token.to_string(),
            notification: notification.clone(),
            data: None,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("key={}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&message)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(FcmError::ApiError(format!(
                "FCM API error: {} - {}",
                status, body
            )));
        }

        let fcm_response: FcmResponse = response.json().await?;
        Ok(fcm_response)
    }
}

#[async_trait]
impl PushNotificationProvider for FcmClient {
    async fn send(
        &self,
        device_token: &str,
        title: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) -> Result<PushResult, PushError> {
        let notification = FcmNotification {
            title: title.to_string(),
            body: message.to_string(),
        };

        let mut fcm_message = FcmMessage {
            to: device_token.to_string(),
            notification,
            data: None,
        };

        if let Some(d) = data {
            fcm_message.data = Some(d);
        }

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("key={}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&fcm_message)
            .send()
            .await
            .map_err(|e| PushError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("FCM API error: {} - {}", status, body);
            return Ok(PushResult {
                success: false,
                message_id: None,
                error_message: Some(format!("FCM API error: {} - {}", status, body)),
            });
        }

        let fcm_response: FcmResponse = response.json().await?;
        info!(
            "FCM notification sent successfully: {:?}",
            fcm_response.message_id
        );

        Ok(PushResult {
            success: fcm_response.success > 0,
            message_id: fcm_response.message_id,
            error_message: if fcm_response.success == 0 {
                fcm_response
                    .results
                    .and_then(|r| r.first().map(|r| r.error.clone()))
                    .unwrap_or(None)
            } else {
                None
            },
        })
    }

    fn provider_name(&self) -> &'static str {
        "fcm"
    }
}

/// FCM message payload
#[derive(Debug, Serialize)]
struct FcmMessage {
    to: String,
    notification: FcmNotification,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// FCM notification payload
#[derive(Debug, Clone, Serialize)]
struct FcmNotification {
    title: String,
    body: String,
}

/// FCM API response
#[derive(Debug, Deserialize)]
struct FcmResponse {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    success: u32,
    #[serde(default)]
    failure: u32,
    #[serde(default)]
    results: Option<Vec<FcmResult>>,
}

/// Individual FCM result
#[derive(Debug, Deserialize)]
struct FcmResult {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// FCM errors
#[derive(Debug, thiserror::Error)]
pub enum FcmError {
    #[error("Invalid FCM configuration: {0}")]
    InvalidConfig(String),

    #[error("FCM API error: {0}")]
    ApiError(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcm_client_creation() {
        let config = FcmConfig {
            api_key: "test_key".to_string(),
        };
        let client = FcmClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_fcm_client_empty_key() {
        let config = FcmConfig {
            api_key: "".to_string(),
        };
        let client = FcmClient::new(config);
        assert!(client.is_err());
    }

    #[test]
    fn test_fcm_notification_serialization() {
        let notification = FcmNotification {
            title: "Test".to_string(),
            body: "Message".to_string(),
        };
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("Message"));
    }

    #[test]
    fn test_provider_name() {
        let config = FcmConfig {
            api_key: "test_key".to_string(),
        };
        let client = FcmClient::new(config).unwrap();
        assert_eq!(client.provider_name(), "fcm");
    }
}
