//! Slack notification integration
//!
//! Provides OAuth-based Slack integration with interactive message buttons
//! for approval workflows.

use crate::{
    InvoiceContext, InvoiceLineItem, Notification, NotificationChannel, NotificationError,
    NotificationProvider, NotificationResult,
};
use async_trait::async_trait;
use billforge_core::UserId;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

/// Per-user Slack credentials stored after OAuth completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUserConfig {
    pub access_token: String,
    pub slack_user_id: Option<String>,
    pub channel_id: Option<String>,
}

/// Slack API client
pub struct SlackClient {
    config: SlackConfig,
    http_client: Client,
    /// Runtime user configs - in production this would be a database-backed store.
    /// Uses `Arc<dyn …>` so callers can inject a real persistence layer.
    user_configs: Arc<dyn SlackUserStore + Send + Sync>,
}

/// Abstraction over per-user Slack credential storage.
pub trait SlackUserStore: Send + Sync {
    fn get(&self, user_id: &UserId) -> Option<SlackUserConfig>;
}

/// Simple in-memory store used by default and in tests.
pub struct InMemorySlackUserStore {
    configs: HashMap<UserId, SlackUserConfig>,
}

impl InMemorySlackUserStore {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    pub fn add(&mut self, user_id: UserId, config: SlackUserConfig) {
        self.configs.insert(user_id, config);
    }
}

impl Default for InMemorySlackUserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SlackUserStore for InMemorySlackUserStore {
    fn get(&self, user_id: &UserId) -> Option<SlackUserConfig> {
        self.configs.get(user_id).cloned()
    }
}

/// Slack configuration
#[derive(Debug, Clone)]
pub struct SlackConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub signing_secret: String,
}

/// Slack OAuth state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackOAuthState {
    pub tenant_id: Uuid,
    pub user_id: UserId,
    pub state_nonce: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Slack OAuth token response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackOAuthResponse {
    ok: bool,
    access_token: String,
    token_type: String,
    scope: String,
    bot_user_id: String,
    app_id: String,
    team: SlackTeam,
    authed_user: SlackAuthedUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackTeam {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackAuthedUser {
    id: String,
    scope: String,
    access_token: Option<String>,
    token_type: Option<String>,
}

/// Slack message response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackMessageResponse {
    ok: bool,
    channel: Option<String>,
    ts: Option<String>,
    message: Option<SlackMessage>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackMessage {
    bot_id: String,
    type_: String,
    text: String,
    user: String,
    ts: String,
    team: String,
    bot_profile: Option<SlackBotProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackBotProfile {
    id: String,
    app_id: String,
    name: String,
    icons: serde_json::Value,
}

/// Slack interactive message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackInteractionPayload {
    #[serde(rename = "type")]
    pub type_: String,
    pub user: SlackUser,
    pub api_app_id: String,
    pub token: String,
    pub container: SlackContainer,
    pub trigger_id: String,
    pub team: SlackTeamBasic,
    pub enterprise: Option<String>,
    pub is_enterprise_install: bool,
    pub channel: SlackChannel,
    pub message: SlackMessageMetadata,
    pub response_url: String,
    pub actions: Vec<SlackAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackUser {
    pub id: String,
    pub username: String,
    pub name: String,
    pub team_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackContainer {
    #[serde(rename = "type")]
    pub type_: String,
    pub message_ts: String,
    pub channel_id: String,
    pub is_ephemeral: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackTeamBasic {
    pub id: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackMessageMetadata {
    #[serde(rename = "type")]
    pub type_: String,
    pub subtype: Option<String>,
    pub bot_id: String,
    pub text: String,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SlackAction {
    #[serde(rename = "type")]
    pub type_: String,
    pub block_id: String,
    pub action_id: String,
    pub value: String,
    pub action_ts: String,
}

/// Slack Block Kit message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
struct SlackBlockMessage {
    channel: String,
    text: String,
    blocks: Vec<SlackBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SlackBlock {
    Section {
        #[serde(rename = "type")]
        type_: String,
        text: SlackTextObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        accessory: Option<SlackAccessory>,
    },
    Fields {
        #[serde(rename = "type")]
        type_: String,
        fields: Vec<SlackTextObject>,
    },
    Context {
        #[serde(rename = "type")]
        type_: String,
        elements: Vec<SlackTextObject>,
    },
    Actions {
        #[serde(rename = "type")]
        type_: String,
        elements: Vec<SlackButtonElement>,
    },
    Divider {
        #[serde(rename = "type")]
        type_: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackTextObject {
    #[serde(rename = "type")]
    type_: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackAccessory {
    #[serde(rename = "type")]
    type_: String,
    image_url: String,
    alt_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackButtonElement {
    #[serde(rename = "type")]
    type_: String,
    text: SlackTextObject,
    action_id: String,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

/// Errors from Slack operations
#[derive(Debug, thiserror::Error)]
pub enum SlackError {
    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("User not found: {0}")]
    UserNotFound(UserId),
}

impl SlackClient {
    /// Create a new Slack client with default in-memory user store
    pub fn new(config: SlackConfig) -> Result<Self, SlackError> {
        Ok(Self {
            config,
            http_client: Client::new(),
            user_configs: Arc::new(InMemorySlackUserStore::new()),
        })
    }

    /// Create a new Slack client with a custom user store (e.g. database-backed)
    pub fn new_with_store(
        config: SlackConfig,
        store: Arc<dyn SlackUserStore + Send + Sync>,
    ) -> Result<Self, SlackError> {
        Ok(Self {
            config,
            http_client: Client::new(),
            user_configs: store,
        })
    }

    /// Generate OAuth authorization URL
    pub fn get_authorize_url(&self, state: &SlackOAuthState) -> String {
        format!(
            "https://slack.com/oauth/v2/authorize?client_id={}&scope=chat:write,users:read,im:write&redirect_uri={}&state={}",
            self.config.client_id,
            urlencoding::encode(&self.config.redirect_uri),
            state.state_nonce
        )
    }

    /// Exchange OAuth code for access token
    pub async fn exchange_code(&self, code: &str) -> Result<SlackOAuthResponse, SlackError> {
        let response = self
            .http_client
            .post("https://slack.com/api/oauth.v2.access")
            .query(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", self.config.redirect_uri.as_str()),
            ])
            .send()
            .await?;

        let oauth_response: SlackOAuthResponse = response.json().await?;

        if !oauth_response.ok {
            return Err(SlackError::OAuth("OAuth exchange failed".to_string()));
        }

        Ok(oauth_response)
    }

    /// Send a message to a Slack channel/user
    pub async fn send_message(
        &self,
        access_token: &str,
        channel: &str,
        notification: &Notification,
    ) -> Result<(String, String), SlackError> {
        // Use rich approval blocks when the notification carries invoice context.
        let fallback_text = notification.message.clone();
        let blocks_json: Vec<serde_json::Value> = notification
            .invoice_context()
            .map(|ctx| build_invoice_approval_blocks(&ctx))
            .unwrap_or_default();

        let body = if blocks_json.is_empty() {
            // Legacy path: use the typed SlackBlock builder
            serde_json::json!({
                "channel": channel,
                "text": fallback_text,
                "blocks": self.build_block_message(notification),
            })
        } else {
            // Rich approval surface: raw JSON blocks from build_invoice_approval_blocks
            serde_json::json!({
                "channel": channel,
                "text": fallback_text,
                "blocks": blocks_json,
            })
        };

        let response = self
            .http_client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let msg_response: SlackMessageResponse = response.json().await?;

        if !msg_response.ok {
            let error = msg_response
                .error
                .unwrap_or_else(|| "Unknown error".to_string());
            warn!("Slack API error: {}", error);
            return Err(SlackError::Api(error));
        }

        let ts = msg_response
            .ts
            .ok_or_else(|| SlackError::Api("No timestamp in response".to_string()))?;
        let channel_id = msg_response
            .channel
            .ok_or_else(|| SlackError::Api("No channel in response".to_string()))?;

        Ok((channel_id, ts))
    }

    /// Build Slack Block Kit message from notification
    fn build_block_message(&self, notification: &Notification) -> Vec<SlackBlock> {
        let mut blocks = Vec::new();

        // Header section
        blocks.push(SlackBlock::Section {
            type_: "section".to_string(),
            text: SlackTextObject {
                type_: "mrkdwn".to_string(),
                text: format!("*{}*\n{}", notification.title, notification.message),
                emoji: None,
            },
            accessory: None,
        });

        // Divider
        blocks.push(SlackBlock::Divider {
            type_: "divider".to_string(),
        });

        // Action buttons
        if !notification.actions.is_empty() {
            let elements: Vec<SlackButtonElement> = notification
                .actions
                .iter()
                .enumerate()
                .map(|(idx, action)| {
                    let action_id = format!("action_{}_{}", notification.id, idx);
                    SlackButtonElement {
                        type_: "button".to_string(),
                        text: SlackTextObject {
                            type_: "plain_text".to_string(),
                            text: action.label.clone(),
                            emoji: Some(true),
                        },
                        action_id,
                        value: serde_json::to_string(&serde_json::json!({
                            "notification_id": notification.id,
                            "action_type": action.action_type,
                            "payload": action.payload,
                        }))
                        .unwrap_or_else(|_| "{}".to_string()),
                        url: action.url.clone(),
                    }
                })
                .collect();

            blocks.push(SlackBlock::Actions {
                type_: "actions".to_string(),
                elements,
            });
        }

        blocks
    }

    /// Open a DM channel with a user
    pub async fn open_im_channel(
        &self,
        access_token: &str,
        slack_user_id: &str,
    ) -> Result<String, SlackError> {
        #[derive(Serialize)]
        #[serde(rename_all = "snake_case")]
        struct OpenImRequest {
            users: String,
        }

        #[derive(Deserialize)]
        struct OpenImResponse {
            ok: bool,
            channel: SlackChannel,
            error: Option<String>,
        }

        let response = self
            .http_client
            .post("https://slack.com/api/conversations.open")
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&OpenImRequest {
                users: slack_user_id.to_string(),
            })
            .send()
            .await?;

        let im_response: OpenImResponse = response.json().await?;

        if !im_response.ok {
            let error = im_response
                .error
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(SlackError::Api(error));
        }

        Ok(im_response.channel.id)
    }

    /// Expose the configured signing secret for HMAC verification in the
    /// interactions endpoint.
    pub fn slack_signing_secret(&self) -> &str {
        &self.config.signing_secret
    }
}

// ---------------------------------------------------------------------------
// Slack request-signature verification
// ---------------------------------------------------------------------------

/// Maximum age (seconds) for a Slack request timestamp before we reject it.
const SLACK_MAX_TIMESTAMP_AGE_SECS: i64 = 300; // 5 minutes

type HmacSha256 = Hmac<Sha256>;

/// Verify that a Slack request carries a valid HMAC-SHA256 signature.
///
/// `timestamp` comes from the `X-Slack-Request-Timestamp` header and `signature`
/// from `X-Slack-Signature`.  `body` is the raw request body bytes.
///
/// The signature base string is `v0:{timestamp}:{body}`.
pub fn verify_slack_signature(
    signing_secret: &str,
    timestamp: &str,
    signature: &str,
    body: &[u8],
) -> Result<(), SlackError> {
    // Reject stale timestamps to prevent replay attacks.
    let ts: i64 = timestamp
        .parse()
        .map_err(|_| SlackError::InvalidSignature)?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > SLACK_MAX_TIMESTAMP_AGE_SECS {
        return Err(SlackError::InvalidSignature);
    }

    let basestring = format!("v0:{}", timestamp);
    let mut mac =
        HmacSha256::new_from_slice(signing_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(basestring.as_bytes());
    mac.update(body);
    let result = mac.finalize().into_bytes();
    let computed = format!("v0={}", hex::encode(result));

    // Constant-time comparison.
    use std::time::Instant;
    let _ = Instant::now(); // ensure constant-time via hmac comparison
    if computed.len() != signature.len()
        || !crypto_common_eq(computed.as_bytes(), signature.as_bytes())
    {
        return Err(SlackError::InvalidSignature);
    }

    Ok(())
}

/// Constant-time byte equality check.
fn crypto_common_eq(a: &[u8], b: &[u8]) -> bool {
    let mut eq = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        eq |= x ^ y;
    }
    eq == 0
}

// ---------------------------------------------------------------------------
// Rich approval Block Kit builder
// ---------------------------------------------------------------------------

/// Build a rich Slack Block Kit message for an invoice approval. Returns the
/// list of Block Kit blocks that should be wrapped in a `chat.postMessage`
/// call.  Each action button carries `action_id = bf_{verb}:{invoice_id}` and
/// `value = {invoice_id}` so the interactions handler can route them.
pub fn build_invoice_approval_blocks(ctx: &InvoiceContext) -> Vec<serde_json::Value> {
    let mut blocks: Vec<serde_json::Value> = Vec::new();

    // Header: vendor + amount
    let amount_dollars = ctx.total_amount_cents as f64 / 100.0;
    blocks.push(serde_json::json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": format!("*Invoice Approval Request*\n*{}* — {} {:.*}",
                ctx.vendor_name, ctx.currency, 2, amount_dollars)
        }
    }));

    // Divider
    blocks.push(serde_json::json!({ "type": "divider" }));

    // Fields section: invoice #, due date, GL code, cost center
    let mut fields: Vec<serde_json::Value> = Vec::new();
    fields.push(serde_json::json!({
        "type": "mrkdwn",
        "text": format!("*Invoice #:*\n{}", ctx.invoice_number)
    }));
    if let Some(ref due) = ctx.due_date {
        fields.push(serde_json::json!({
            "type": "mrkdwn",
            "text": format!("*Due Date:*\n{}", due)
        }));
    }
    if let Some(ref gl) = ctx.gl_code {
        fields.push(serde_json::json!({
            "type": "mrkdwn",
            "text": format!("*GL Code:*\n{}", gl)
        }));
    }
    if let Some(ref cc) = ctx.cost_center {
        fields.push(serde_json::json!({
            "type": "mrkdwn",
            "text": format!("*Cost Center:*\n{}", cc)
        }));
    }
    if !fields.is_empty() {
        blocks.push(serde_json::json!({
            "type": "section",
            "fields": fields
        }));
    }

    // Line items table (up to 5 rows + overflow indicator)
    if !ctx.line_items.is_empty() {
        let display_items: Vec<&InvoiceLineItem> = ctx.line_items.iter().take(5).collect();
        let overflow = ctx.line_items.len().saturating_sub(5);
        let mut li_text = String::from("*Line Items:*\n");
        for item in &display_items {
            let qty_str = item
                .quantity
                .map(|q| format!(" x{}", q))
                .unwrap_or_default();
            let total_str = item
                .total_cents
                .map(|c| format!(" ${:.2}", c as f64 / 100.0))
                .unwrap_or_default();
            li_text.push_str(&format!("• {}{}{}\n", item.description, qty_str, total_str));
        }
        if overflow > 0 {
            li_text.push_str(&format!("… and {} more", overflow));
        }
        blocks.push(serde_json::json!({
            "type": "section",
            "text": { "type": "mrkdwn", "text": li_text.trim_end() }
        }));
    }

    // Context block with PDF preview link
    if let Some(ref pdf_url) = ctx.pdf_preview_url {
        blocks.push(serde_json::json!({
            "type": "context",
            "elements": [
                { "type": "mrkdwn", "text": format!("<{}|📄 View PDF>", pdf_url) }
            ]
        }));
    }

    // Divider before actions
    blocks.push(serde_json::json!({ "type": "divider" }));

    // Action buttons: Approve, Reject, Request Changes, Reassign, Comment
    let invoice_id = ctx.invoice_id.to_string();
    let actions = serde_json::json!({
        "type": "actions",
        "elements": [
            {
                "type": "button",
                "text": { "type": "plain_text", "text": "Approve", "emoji": true },
                "style": "primary",
                "action_id": format!("bf_approve:{}", invoice_id),
                "value": invoice_id
            },
            {
                "type": "button",
                "text": { "type": "plain_text", "text": "Reject", "emoji": true },
                "style": "danger",
                "action_id": format!("bf_reject:{}", invoice_id),
                "value": invoice_id
            },
            {
                "type": "button",
                "text": { "type": "plain_text", "text": "Request Changes" },
                "action_id": format!("bf_request_changes:{}", invoice_id),
                "value": invoice_id
            },
            {
                "type": "button",
                "text": { "type": "plain_text", "text": "Reassign" },
                "action_id": format!("bf_reassign:{}", invoice_id),
                "value": invoice_id
            },
            {
                "type": "button",
                "text": { "type": "plain_text", "text": "Comment" },
                "action_id": format!("bf_comment:{}", invoice_id),
                "value": invoice_id
            }
        ]
    });
    blocks.push(actions);

    blocks
}

#[async_trait]
impl NotificationProvider for SlackClient {
    async fn send(
        &self,
        notification: &Notification,
    ) -> Result<NotificationResult, NotificationError> {
        let user_config = self
            .user_configs
            .get(&notification.user_id)
            .ok_or_else(|| NotificationError::NotConfigured(notification.user_id.clone()))?;

        // Determine channel: prefer explicit channel_id, otherwise open DM via slack_user_id
        let channel = if let Some(ref ch) = user_config.channel_id {
            ch.clone()
        } else if let Some(ref slack_uid) = user_config.slack_user_id {
            self.open_im_channel(&user_config.access_token, slack_uid)
                .await?
        } else {
            return Err(NotificationError::NotConfigured(
                notification.user_id.clone(),
            ));
        };

        match self
            .send_message(&user_config.access_token, &channel, notification)
            .await
        {
            Ok((_channel_id, ts)) => Ok(NotificationResult {
                success: true,
                channel: NotificationChannel::Slack,
                external_id: Some(ts),
                error_message: None,
            }),
            Err(e) => Ok(NotificationResult {
                success: false,
                channel: NotificationChannel::Slack,
                external_id: None,
                error_message: Some(e.to_string()),
            }),
        }
    }

    fn provider_name(&self) -> &'static str {
        "slack"
    }

    async fn is_configured(&self, user_id: &UserId) -> Result<bool, NotificationError> {
        Ok(self.user_configs.get(user_id).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SlackConfig {
        SlackConfig {
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            signing_secret: "secret".to_string(),
        }
    }

    #[test]
    fn test_build_block_message() {
        let client = SlackClient::new(test_config()).unwrap();

        let notification = Notification::new(
            UserId(Uuid::new_v4()),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );

        let blocks = client.build_block_message(&notification);
        assert!(blocks.len() >= 2); // At least section + divider
    }

    #[test]
    fn test_oauth_url_generation() {
        let client = SlackClient::new(test_config()).unwrap();

        let state = SlackOAuthState {
            tenant_id: Uuid::new_v4(),
            user_id: UserId(Uuid::new_v4()),
            state_nonce: "test_state".to_string(),
            created_at: chrono::Utc::now(),
        };

        let url = client.get_authorize_url(&state);
        assert!(url.contains("client_id=test"));
        assert!(url.contains("test_state"));
    }

    #[tokio::test]
    async fn test_is_configured_false_when_no_user_config() {
        let client = SlackClient::new(test_config()).unwrap();
        let user_id = UserId(Uuid::new_v4());
        assert!(!client.is_configured(&user_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_is_configured_true_when_user_config_present() {
        let user_id = UserId(Uuid::new_v4());
        let store = Arc::new({
            let mut s = InMemorySlackUserStore::new();
            s.add(
                user_id.clone(),
                SlackUserConfig {
                    access_token: "xoxb-test".to_string(),
                    slack_user_id: Some("U123".to_string()),
                    channel_id: None,
                },
            );
            s
        });

        let client = SlackClient::new_with_store(test_config(), store).unwrap();
        assert!(client.is_configured(&user_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_send_returns_not_configured_when_no_user_config() {
        let client = SlackClient::new(test_config()).unwrap();
        let user_id = UserId(Uuid::new_v4());
        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );

        let result = client.send(&notification).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            NotificationError::NotConfigured(uid) => assert_eq!(uid, user_id),
            other => panic!("Expected NotConfigured, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_send_returns_not_configured_when_no_channel_or_slack_uid() {
        let user_id = UserId(Uuid::new_v4());
        let store = Arc::new({
            let mut s = InMemorySlackUserStore::new();
            s.add(
                user_id.clone(),
                SlackUserConfig {
                    access_token: "xoxb-test".to_string(),
                    slack_user_id: None,
                    channel_id: None,
                },
            );
            s
        });

        let client = SlackClient::new_with_store(test_config(), store).unwrap();

        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );

        let result = client.send(&notification).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NotificationError::NotConfigured(_) => {}
            other => panic!("Expected NotConfigured, got {:?}", other),
        }
    }

    #[test]
    fn test_in_memory_store_default() {
        let store = InMemorySlackUserStore::default();
        let uid = UserId(Uuid::new_v4());
        assert!(store.get(&uid).is_none());
    }
}
