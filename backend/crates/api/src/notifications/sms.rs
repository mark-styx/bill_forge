//! SMS / WhatsApp provider abstraction used by the mobile approval-link surface.
//!
//! Provides a pluggable [`SmsProvider`] trait so the API can deliver signed
//! approval deep links over SMS or WhatsApp. The default implementation
//! ([`NoopSmsProvider`]) records sends in memory and is used in dev/CI; a live
//! [`TwilioSmsProvider`] is constructed when `TWILIO_ACCOUNT_SID` is configured.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Channel over which an SMS-shaped message is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmsChannel {
    /// Plain SMS text message.
    Sms,
    /// WhatsApp Business message.
    WhatsApp,
}

impl SmsChannel {
    /// Stable lowercase string used in audit metadata and JWT claims.
    pub fn as_str(&self) -> &'static str {
        match self {
            SmsChannel::Sms => "sms",
            SmsChannel::WhatsApp => "whatsapp",
        }
    }
}

impl std::fmt::Display for SmsChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provider error type.
#[derive(Debug, thiserror::Error)]
pub enum SmsError {
    #[error("SMS provider error: {0}")]
    Provider(String),
    #[error("SMS transport error: {0}")]
    Http(String),
}

/// Result alias for SMS delivery.
pub type SmsResult<T> = std::result::Result<T, SmsError>;

/// Pluggable SMS / WhatsApp delivery provider.
///
/// Implementations must be cheaply cloneable behind an `Arc` and safe to share
/// across request handlers. Returning the provider-issued message id enables
/// audit-log correlation with downstream delivery receipts.
#[async_trait]
pub trait SmsProvider: Send + Sync {
    /// Send `body` to `to_e164` over `channel`. Returns the provider message id.
    async fn send(&self, to_e164: &str, body: &str, channel: SmsChannel) -> SmsResult<String>;
}

/// A message captured by [`NoopSmsProvider`] for test/dev inspection.
#[derive(Debug, Clone)]
pub struct RecordedMessage {
    pub to: String,
    pub body: String,
    pub channel: SmsChannel,
}

/// Default provider for dev/test environments: records every send to an
/// in-memory `Vec` exposed via [`NoopSmsProvider::recorded`]. Makes no network
/// calls, so it is safe to use in CI and unit tests.
pub struct NoopSmsProvider {
    sent: Mutex<Vec<RecordedMessage>>,
}

impl NoopSmsProvider {
    pub fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of every message captured by this provider.
    pub fn recorded(&self) -> Vec<RecordedMessage> {
        self.sent.lock().expect("noop sms lock poisoned").clone()
    }
}

impl Default for NoopSmsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SmsProvider for NoopSmsProvider {
    async fn send(&self, to_e164: &str, body: &str, channel: SmsChannel) -> SmsResult<String> {
        let id = format!("noop-{}", uuid::Uuid::new_v4());
        self.sent
            .lock()
            .expect("noop sms lock poisoned")
            .push(RecordedMessage {
                to: to_e164.to_string(),
                body: body.to_string(),
                channel,
            });
        Ok(id)
    }
}

/// Live provider backed by the Twilio Messages API.
///
/// Constructed only when `TWILIO_ACCOUNT_SID` is configured. Holds no live
/// network state between requests, so it is safe to share via `Arc`.
pub struct TwilioSmsProvider {
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
    pub whatsapp_from: String,
    pub client: reqwest::Client,
}

impl TwilioSmsProvider {
    pub fn new(
        account_sid: String,
        auth_token: String,
        from_number: String,
        whatsapp_from: String,
    ) -> Self {
        Self {
            account_sid,
            auth_token,
            from_number,
            whatsapp_from,
            client: reqwest::Client::new(),
        }
    }

    fn endpoint(&self) -> String {
        format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        )
    }

    /// Address pair (`from`, `to`) for a given channel. WhatsApp prefixes both
    /// sides with the `whatsapp:` scheme per Twilio's API contract.
    fn addresses(&self, to_e164: &str, channel: SmsChannel) -> (String, String) {
        match channel {
            SmsChannel::Sms => (self.from_number.clone(), to_e164.to_string()),
            SmsChannel::WhatsApp => (
                format!("whatsapp:{}", self.whatsapp_from),
                format!("whatsapp:{}", to_e164),
            ),
        }
    }
}

#[async_trait]
impl SmsProvider for TwilioSmsProvider {
    async fn send(&self, to_e164: &str, body: &str, channel: SmsChannel) -> SmsResult<String> {
        let (from, to) = self.addresses(to_e164, channel);

        let resp = self
            .client
            .post(self.endpoint())
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&[
                ("To", to.as_str()),
                ("From", from.as_str()),
                ("Body", body),
            ])
            .send()
            .await
            .map_err(|e| SmsError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SmsError::Provider(format!(
                "twilio {} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("error"),
                text
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SmsError::Http(e.to_string()))?;
        json.get("sid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| SmsError::Provider("twilio response missing 'sid'".to_string()))
    }
}

/// Factory: returns a [`NoopSmsProvider`] unless `TWILIO_ACCOUNT_SID` is set,
/// in which case it returns a [`TwilioSmsProvider`] wired from env vars.
///
/// Kept env-driven so CI and unit tests always hit the noop path without
/// touching a live carrier.
pub fn provider_from_env() -> Arc<dyn SmsProvider> {
    match std::env::var("TWILIO_ACCOUNT_SID") {
        Ok(sid) if !sid.trim().is_empty() => {
            let auth_token = std::env::var("TWILIO_AUTH_TOKEN").unwrap_or_default();
            let from_number = std::env::var("TWILIO_FROM_NUMBER").unwrap_or_default();
            let whatsapp_from = std::env::var("TWILIO_WHATSAPP_FROM")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| from_number.clone());
            Arc::new(TwilioSmsProvider::new(
                sid,
                auth_token,
                from_number,
                whatsapp_from,
            ))
        }
        _ => Arc::new(NoopSmsProvider::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_provider_records_each_send() {
        let provider = NoopSmsProvider::new();
        let id = provider
            .send("+15551234567", "hello world", SmsChannel::Sms)
            .await
            .expect("noop send");
        assert!(id.starts_with("noop-"), "noop id prefix: {id}");

        let whatsapp_id = provider
            .send("+15557654321", "approve me", SmsChannel::WhatsApp)
            .await
            .expect("noop whatsapp send");
        assert!(whatsapp_id.starts_with("noop-"));

        let recorded = provider.recorded();
        assert_eq!(recorded.len(), 2, "should record both sends");
        assert_eq!(recorded[0].to, "+15551234567");
        assert_eq!(recorded[0].body, "hello world");
        assert_eq!(recorded[0].channel, SmsChannel::Sms);
        assert_eq!(recorded[1].channel, SmsChannel::WhatsApp);
    }

    #[test]
    fn sms_channel_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&SmsChannel::Sms).unwrap(),
            "\"sms\""
        );
        assert_eq!(
            serde_json::to_string(&SmsChannel::WhatsApp).unwrap(),
            "\"whatsapp\""
        );
        let parsed: SmsChannel = serde_json::from_str("\"whatsapp\"").unwrap();
        assert_eq!(parsed, SmsChannel::WhatsApp);
    }

    #[test]
    fn twilio_addresses_prefix_whatsapp() {
        let provider = TwilioSmsProvider::new(
            "ACtest".to_string(),
            "token".to_string(),
            "+15550000000".to_string(),
            "+15551111111".to_string(),
        );
        let (from, to) = provider.addresses("+15552222222", SmsChannel::Sms);
        assert_eq!(from, "+15550000000");
        assert_eq!(to, "+15552222222");

        let (from, to) = provider.addresses("+15552222222", SmsChannel::WhatsApp);
        assert_eq!(from, "whatsapp:+15551111111");
        assert_eq!(to, "whatsapp:+15552222222");
    }
}
