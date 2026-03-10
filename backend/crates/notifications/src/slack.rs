//! Slack notification integration
//!
//! Provides OAuth-based Slack integration with interactive message buttons
//! for approval workflows.

use crate::{
    Notification, NotificationAction, NotificationChannel, NotificationError,
    NotificationProvider, NotificationResult, ActionType,
};
use async_trait::async_trait;
use billforge_core::UserId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// Slack API client
pub struct SlackClient {
    config: SlackConfig,
    http_client: Client,
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
struct SlackOAuthResponse {
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
pub struct SlackUser {
    pub id: String,
    pub username: String,
    pub name: String,
    pub team_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackContainer {
    #[serde(rename = "type")]
    pub type_: String,
    pub message_ts: String,
    pub channel_id: String,
    pub is_ephemeral: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct SlackMessageMetadata {
    #[serde(rename = "type")]
    pub type_: String,
    pub subtype: Option<String>,
    pub bot_id: String,
    pub text: String,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Create a new Slack client
    pub fn new(config: SlackConfig) -> Result<Self, SlackError> {
        Ok(Self {
            config,
            http_client: Client::new(),
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
        let message = self.build_block_message(notification);

        let response = self
            .http_client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .json(&SlackBlockMessage {
                channel: channel.to_string(),
                text: notification.message.clone(),
                blocks: message,
            })
            .send()
            .await?;

        let msg_response: SlackMessageResponse = response.json().await?;

        if !msg_response.ok {
            let error = msg_response.error.unwrap_or_else(|| "Unknown error".to_string());
            warn!("Slack API error: {}", error);
            return Err(SlackError::Api(error));
        }

        let ts = msg_response.ts.ok_or_else(|| {
            SlackError::Api("No timestamp in response".to_string())
        })?;
        let channel_id = msg_response.channel.ok_or_else(|| {
            SlackError::Api("No channel in response".to_string())
        })?;

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
                        })).unwrap_or_else(|_| "{}".to_string()),
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
            let error = im_response.error.unwrap_or_else(|| "Unknown error".to_string());
            return Err(SlackError::Api(error));
        }

        Ok(im_response.channel.id)
    }
}

#[async_trait]
impl NotificationProvider for SlackClient {
    async fn send(&self, notification: &Notification) -> Result<NotificationResult, NotificationError> {
        // This is a placeholder - actual implementation would need to:
        // 1. Fetch user's Slack access token and channel from database
        // 2. Send message using send_message()
        // 3. Return result with external message ID

        Err(NotificationError::NotConfigured(notification.user_id.clone()))
    }

    fn provider_name(&self) -> &'static str {
        "slack"
    }

    async fn is_configured(&self, user_id: &UserId) -> Result<bool, NotificationError> {
        // Placeholder - would check database for Slack connection
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_block_message() {
        let config = SlackConfig {
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            signing_secret: "secret".to_string(),
        };

        let client = SlackClient::new(config).unwrap();

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
        let config = SlackConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            signing_secret: "secret".to_string(),
        };

        let client = SlackClient::new(config).unwrap();

        let state = SlackOAuthState {
            tenant_id: Uuid::new_v4(),
            user_id: UserId(Uuid::new_v4()),
            state_nonce: "test_state".to_string(),
            created_at: chrono::Utc::now(),
        };

        let url = client.get_authorize_url(&state);
        assert!(url.contains("test_client_id"));
        assert!(url.contains("test_state"));
    }
}
