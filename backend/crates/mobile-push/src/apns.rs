//! Apple Push Notification Service (APNS) client for iOS push notifications

use crate::{PushError, PushNotificationProvider, PushResult};
use async_trait::async_trait;
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::Serialize;
use std::fs;
use tracing::{info, warn};

/// APNS environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApnsEnvironment {
    Sandbox,
    Production,
}

impl ApnsEnvironment {
    fn endpoint(&self) -> &'static str {
        match self {
            ApnsEnvironment::Sandbox => "https://api.sandbox.push.apple.com",
            ApnsEnvironment::Production => "https://api.push.apple.com",
        }
    }
}

/// APNS configuration
#[derive(Debug, Clone)]
pub struct ApnsConfig {
    pub environment: ApnsEnvironment,
    pub private_key_path: String,
    pub key_id: String,
    pub team_id: String,
    pub bundle_id: String,
}

/// APNS client
#[derive(Debug)]
pub struct ApnsClient {
    config: ApnsConfig,
    client: Client,
    private_key: Vec<u8>,
}

impl ApnsClient {
    /// Create a new APNS client
    pub fn new(config: ApnsConfig) -> Result<Self, ApnsError> {
        if config.key_id.is_empty() || config.team_id.is_empty() || config.bundle_id.is_empty() {
            return Err(ApnsError::InvalidConfig(
                "key_id, team_id, and bundle_id are required".to_string(),
            ));
        }

        let private_key = fs::read(&config.private_key_path).map_err(|e| {
            ApnsError::InvalidConfig(format!("Failed to read private key: {}", e))
        })?;

        Ok(Self {
            config,
            client: Client::new(),
            private_key,
        })
    }

    /// Generate JWT token for APNS authentication
    fn generate_jwt(&self) -> Result<String, ApnsError> {
        let header = Header {
            kid: Some(self.config.key_id.clone()),
            alg: Algorithm::ES256,
            ..Default::default()
        };

        let claims = ApnsClaims {
            iss: self.config.team_id.clone(),
            iat: Utc::now().timestamp(),
        };

        let encoding_key = EncodingKey::from_ec_pem(&self.private_key)?;

        let token = encode(&header, &claims, &encoding_key)?;
        Ok(token)
    }

    /// Get the APNS endpoint URL for a device token
    fn endpoint_url(&self, device_token: &str) -> String {
        format!(
            "{}/3/device/{}",
            self.config.environment.endpoint(),
            device_token
        )
    }
}

#[async_trait]
impl PushNotificationProvider for ApnsClient {
    async fn send(
        &self,
        device_token: &str,
        title: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) -> Result<PushResult, PushError> {
        let jwt_token = self.generate_jwt().map_err(|e| {
            warn!("Failed to generate APNS JWT: {}", e);
            PushError::Apns(e)
        })?;

        let apns_payload = ApnsPayload {
            aps: ApsPayload {
                alert: AlertPayload {
                    title: title.to_string(),
                    body: message.to_string(),
                },
                badge: None,
                sound: Some("default".to_string()),
            },
            data,
        };

        let response = self
            .client
            .post(&self.endpoint_url(device_token))
            .header("authorization", format!("bearer {}", jwt_token))
            .header("apns-topic", &self.config.bundle_id)
            .header("apns-push-type", "alert")
            .header("content-type", "application/json")
            .json(&apns_payload)
            .send()
            .await
            .map_err(|e| PushError::Network(e.to_string()))?;

        let status = response.status();

        if status.is_success() {
            info!("APNS notification sent successfully to device");
            Ok(PushResult {
                success: true,
                message_id: None,
                error_message: None,
            })
        } else {
            let error_body = response.text().await.unwrap_or_default();
            warn!("APNS API error: {} - {}", status, error_body);
            Ok(PushResult {
                success: false,
                message_id: None,
                error_message: Some(format!("APNS error: {} - {}", status, error_body)),
            })
        }
    }

    fn provider_name(&self) -> &'static str {
        "apns"
    }
}

/// APNS JWT claims
#[derive(Debug, Serialize)]
struct ApnsClaims {
    iss: String,
    iat: i64,
}

/// APNS payload
#[derive(Debug, Serialize)]
struct ApnsPayload {
    aps: ApsPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// APS payload (Apple Push Service)
#[derive(Debug, Serialize)]
struct ApsPayload {
    alert: AlertPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<String>,
}

/// Alert payload
#[derive(Debug, Serialize)]
struct AlertPayload {
    title: String,
    body: String,
}

/// APNS errors
#[derive(Debug, thiserror::Error)]
pub enum ApnsError {
    #[error("Invalid APNS configuration: {0}")]
    InvalidConfig(String),

    #[error("JWT generation error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apns_environment_endpoint() {
        assert_eq!(
            ApnsEnvironment::Sandbox.endpoint(),
            "https://api.sandbox.push.apple.com"
        );
        assert_eq!(
            ApnsEnvironment::Production.endpoint(),
            "https://api.push.apple.com"
        );
    }

    #[test]
    fn test_apns_payload_serialization() {
        let payload = ApnsPayload {
            aps: ApsPayload {
                alert: AlertPayload {
                    title: "Test".to_string(),
                    body: "Message".to_string(),
                },
                badge: Some(1),
                sound: Some("default".to_string()),
            },
            data: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("Message"));
        assert!(json.contains("aps"));
    }
}
