//! Mobile Push Notification Infrastructure for BillForge
//!
//! Provides push notification support via Firebase Cloud Messaging (FCM) for Android
//! and Apple Push Notification Service (APNS) for iOS.

mod apns;
mod device_token;
mod fcm;

pub use apns::{ApnsClient, ApnsConfig, ApnsEnvironment, ApnsError};
pub use device_token::{validate_token, DevicePlatform, DeviceToken, TokenValidationError};
pub use fcm::{FcmClient, FcmConfig, FcmError};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Trait for push notification providers
#[async_trait]
pub trait PushNotificationProvider: Send + Sync {
    /// Send a push notification
    async fn send(
        &self,
        device_token: &str,
        title: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) -> Result<PushResult, PushError>;

    /// Get the provider name
    fn provider_name(&self) -> &'static str;
}

/// Result of sending a push notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    pub success: bool,
    pub message_id: Option<String>,
    pub error_message: Option<String>,
}

/// Errors from push notification operations
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("FCM error: {0}")]
    Fcm(#[from] FcmError),

    #[error("APNS error: {0}")]
    Apns(#[from] ApnsError),

    #[error("Invalid device token: {0}")]
    InvalidToken(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Provider not configured")]
    NotConfigured,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<reqwest::Error> for PushError {
    fn from(err: reqwest::Error) -> Self {
        PushError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for PushError {
    fn from(err: serde_json::Error) -> Self {
        PushError::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_result_success() {
        let result = PushResult {
            success: true,
            message_id: Some("msg_123".to_string()),
            error_message: None,
        };

        assert!(result.success);
        assert_eq!(result.message_id, Some("msg_123".to_string()));
    }

    #[test]
    fn test_push_error_from_network() {
        let error = PushError::Network("Connection refused".to_string());
        assert!(error.to_string().contains("Network error"));
    }
}
