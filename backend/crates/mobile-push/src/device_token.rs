//! Device token validation for push notifications

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Device platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DevicePlatform {
    Ios,
    Android,
}

impl fmt::Display for DevicePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DevicePlatform::Ios => write!(f, "ios"),
            DevicePlatform::Android => write!(f, "android"),
        }
    }
}

/// Validated device token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceToken {
    pub token: String,
    pub platform: DevicePlatform,
}

impl DeviceToken {
    /// Create and validate a device token
    pub fn new(token: String, platform: DevicePlatform) -> Result<Self, TokenValidationError> {
        validate_token(&token, platform)?;
        Ok(Self { token, platform })
    }
}

/// Validate device token format
///
/// FCM tokens: 152 alphanumeric characters
/// APNS tokens: 64 hexadecimal characters
pub fn validate_token(token: &str, platform: DevicePlatform) -> Result<(), TokenValidationError> {
    match platform {
        DevicePlatform::Android => validate_fcm_token(token),
        DevicePlatform::Ios => validate_apns_token(token),
    }
}

/// Validate FCM token (152 alphanumeric characters)
fn validate_fcm_token(token: &str) -> Result<(), TokenValidationError> {
    if token.is_empty() {
        return Err(TokenValidationError::EmptyToken);
    }

    if token.len() > 152 {
        return Err(TokenValidationError::InvalidLength {
            expected: 152,
            actual: token.len(),
        });
    }

    // FCM tokens are alphanumeric with colons and dashes
    let re = Regex::new(r"^[a-zA-Z0-9:_-]+$").unwrap();
    if !re.is_match(token) {
        return Err(TokenValidationError::InvalidFormat(
            "FCM token must be alphanumeric with :_- characters only".to_string(),
        ));
    }

    Ok(())
}

/// Validate APNS token (64 hexadecimal characters)
fn validate_apns_token(token: &str) -> Result<(), TokenValidationError> {
    if token.is_empty() {
        return Err(TokenValidationError::EmptyToken);
    }

    if token.len() != 64 {
        return Err(TokenValidationError::InvalidLength {
            expected: 64,
            actual: token.len(),
        });
    }

    // APNS tokens are hexadecimal
    let re = Regex::new(r"^[a-fA-F0-9]+$").unwrap();
    if !re.is_match(token) {
        return Err(TokenValidationError::InvalidFormat(
            "APNS token must be 64 hexadecimal characters".to_string(),
        ));
    }

    Ok(())
}

/// Token validation errors
#[derive(Debug, thiserror::Error)]
pub enum TokenValidationError {
    #[error("Token cannot be empty")]
    EmptyToken,

    #[error("Invalid token length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },

    #[error("Invalid token format: {0}")]
    InvalidFormat(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_fcm_token_valid() {
        let token = "a".repeat(152);
        let result = validate_token(&token, DevicePlatform::Android);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_fcm_token_with_special_chars() {
        let token = "abc123:_-XYZ";
        let result = validate_token(token, DevicePlatform::Android);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_fcm_token_too_long() {
        let token = "a".repeat(153);
        let result = validate_token(&token, DevicePlatform::Android);
        assert!(matches!(
            result,
            Err(TokenValidationError::InvalidLength { .. })
        ));
    }

    #[test]
    fn test_validate_fcm_token_invalid_chars() {
        let token = "abc@123!";
        let result = validate_token(token, DevicePlatform::Android);
        assert!(matches!(
            result,
            Err(TokenValidationError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_validate_apns_token_valid() {
        let token = "a".repeat(64);
        let result = validate_token(&token, DevicePlatform::Ios);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_apns_token_wrong_length() {
        let token = "a".repeat(63);
        let result = validate_token(&token, DevicePlatform::Ios);
        assert!(matches!(
            result,
            Err(TokenValidationError::InvalidLength { .. })
        ));
    }

    #[test]
    fn test_validate_apns_token_not_hex() {
        let token = "g".repeat(64); // 'g' is not hex
        let result = validate_token(&token, DevicePlatform::Ios);
        assert!(matches!(
            result,
            Err(TokenValidationError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_device_token_new_valid() {
        let token = "a".repeat(152);
        let result = DeviceToken::new(token, DevicePlatform::Android);
        assert!(result.is_ok());
    }

    #[test]
    fn test_device_token_new_invalid() {
        let token = "".to_string();
        let result = DeviceToken::new(token, DevicePlatform::Android);
        assert!(result.is_err());
    }
}
