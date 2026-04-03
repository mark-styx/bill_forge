//! Microsoft Teams notification integration
//!
//! Provides webhook-based Teams notifications with Adaptive Cards
//! for rich formatting and actionable buttons.

use crate::{
    ActionType, Notification, NotificationAction, NotificationChannel, NotificationError,
    NotificationProvider, NotificationResult,
};
use async_trait::async_trait;
use billforge_core::UserId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// Teams webhook client
pub struct TeamsClient {
    config: TeamsConfig,
    http_client: Client,
}

/// Teams configuration
#[derive(Debug, Clone)]
pub struct TeamsConfig {
    pub tenant_id: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

/// Teams webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsWebhookPayload {
    #[serde(rename = "@type")]
    type_: String,
    #[serde(rename = "@context")]
    context: String,
    theme_color: String,
    summary: String,
    sections: Vec<TeamsSection>,
    potential_action: Vec<TeamsAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsSection {
    activity_title: String,
    activity_subtitle: Option<String>,
    activity_image: Option<String>,
    facts: Vec<TeamsFact>,
    markdown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsFact {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsAction {
    #[serde(rename = "@type")]
    type_: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inputs: Option<Vec<TeamsInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<Vec<TeamsHeader>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsInput {
    #[serde(rename = "@type")]
    type_: String,
    id: String,
    is_multiline: Option<bool>,
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamsHeader {
    name: String,
    value: String,
}

/// Adaptive Card for richer Teams messages
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptiveCard {
    #[serde(rename = "type")]
    type_: String,
    version: String,
    body: Vec<AdaptiveCardElement>,
    actions: Vec<AdaptiveCardAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AdaptiveCardElement {
    TextBlock {
        text: String,
        size: Option<String>,
        weight: Option<String>,
        wrap: Option<bool>,
    },
    Container {
        items: Vec<AdaptiveCardElement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        style: Option<String>,
    },
    FactSet {
        facts: Vec<AdaptiveCardFact>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptiveCardFact {
    title: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AdaptiveCardAction {
    ActionSet {
        actions: Vec<AdaptiveCardActionButton>,
    },
    ActionSubmit {
        title: String,
        data: serde_json::Value,
    },
    ActionOpenUrl {
        title: String,
        url: String,
    },
    ActionShowCard {
        title: String,
        card: AdaptiveCard,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptiveCardActionButton {
    #[serde(rename = "type")]
    type_: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// Teams webhook response
#[derive(Debug, Clone, Deserialize)]
struct TeamsWebhookResponse {
    success: Option<bool>,
    error: Option<String>,
}

/// Teams user preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsUserPreferences {
    pub user_id: UserId,
    pub webhook_url: String,
    pub channel_name: Option<String>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Errors from Teams operations
#[derive(Debug, thiserror::Error)]
pub enum TeamsError {
    #[error("Webhook error: {0}")]
    Webhook(String),

    #[error("Invalid webhook URL")]
    InvalidWebhookUrl,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("User not configured: {0}")]
    NotConfigured(UserId),

    #[error("API error: {0}")]
    Api(String),
}

impl TeamsClient {
    /// Create a new Teams client
    pub fn new(config: TeamsConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    /// Send notification via webhook
    pub async fn send_webhook(
        &self,
        webhook_url: &str,
        notification: &Notification,
    ) -> Result<String, TeamsError> {
        let payload = self.build_webhook_payload(notification);

        let response = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!("Teams webhook failed with status {}: {}", status, body);
            return Err(TeamsError::Webhook(format!("Status {}: {}", status, body)));
        }

        // Teams webhook returns 202 Accepted with no body
        Ok(notification.id.to_string())
    }

    /// Send Adaptive Card (richer format)
    pub async fn send_adaptive_card(
        &self,
        webhook_url: &str,
        notification: &Notification,
    ) -> Result<String, TeamsError> {
        let card = self.build_adaptive_card(notification);

        let response = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.card.adaptive",
                    "contentUrl": null,
                    "content": card
                }]
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!(
                "Teams adaptive card failed with status {}: {}",
                status, body
            );
            return Err(TeamsError::Webhook(format!("Status {}: {}", status, body)));
        }

        Ok(notification.id.to_string())
    }

    /// Build Office 365 connector card payload
    fn build_webhook_payload(&self, notification: &Notification) -> TeamsWebhookPayload {
        let color = match notification.priority {
            crate::NotificationPriority::Urgent => "FF0000",
            crate::NotificationPriority::High => "FFA500",
            crate::NotificationPriority::Normal => "0078D7",
            crate::NotificationPriority::Low => "808080",
        };

        let mut sections = vec![TeamsSection {
            activity_title: notification.title.clone(),
            activity_subtitle: Some(notification.message.clone()),
            activity_image: None,
            facts: vec![],
            markdown: true,
        }];

        // Add metadata as facts
        if let Some(obj) = notification.metadata.as_object() {
            let facts: Vec<TeamsFact> = obj
                .iter()
                .map(|(k, v)| TeamsFact {
                    name: k.clone(),
                    value: v.to_string(),
                })
                .collect();

            if !facts.is_empty() {
                sections[0].facts = facts;
            }
        }

        let actions: Vec<TeamsAction> = notification
            .actions
            .iter()
            .filter_map(|action| {
                match action.action_type {
                    ActionType::View => {
                        if let Some(url) = &action.url {
                            Some(TeamsAction {
                                type_: "OpenUri".to_string(),
                                name: action.label.clone(),
                                target: Some(vec![url.clone()]),
                                inputs: None,
                                body: None,
                                headers: None,
                            })
                        } else {
                            None
                        }
                    }
                    ActionType::Approve | ActionType::Reject => {
                        // For actions that need server-side processing,
                        // we'd use Action.Http with a backend endpoint
                        action.url.as_ref().map(|url| TeamsAction {
                            type_: "HttpPOST".to_string(),
                            name: action.label.clone(),
                            target: None,
                            inputs: None,
                            body: serde_json::to_string(&serde_json::json!({
                                "notification_id": notification.id,
                                "action_type": action.action_type,
                                "payload": action.payload,
                            }))
                            .ok(),
                            headers: Some(vec![TeamsHeader {
                                name: "Content-Type".to_string(),
                                value: "application/json".to_string(),
                            }]),
                        })
                    }
                    _ => None,
                }
            })
            .collect();

        TeamsWebhookPayload {
            type_: "MessageCard".to_string(),
            context: "http://schema.org/extensions".to_string(),
            theme_color: color.to_string(),
            summary: notification.title.clone(),
            sections,
            potential_action: actions,
        }
    }

    /// Build Adaptive Card payload
    fn build_adaptive_card(&self, notification: &Notification) -> AdaptiveCard {
        let mut body = vec![
            AdaptiveCardElement::TextBlock {
                text: notification.title.clone(),
                size: Some("Medium".to_string()),
                weight: Some("Bolder".to_string()),
                wrap: Some(true),
            },
            AdaptiveCardElement::TextBlock {
                text: notification.message.clone(),
                size: None,
                weight: None,
                wrap: Some(true),
            },
        ];

        // Add metadata as facts
        if let Some(obj) = notification.metadata.as_object() {
            if !obj.is_empty() {
                let facts: Vec<AdaptiveCardFact> = obj
                    .iter()
                    .map(|(k, v)| AdaptiveCardFact {
                        title: k.clone(),
                        value: v.to_string(),
                    })
                    .collect();

                body.push(AdaptiveCardElement::FactSet { facts });
            }
        }

        let actions: Vec<AdaptiveCardAction> = notification
            .actions
            .iter()
            .filter_map(|action| match action.action_type {
                ActionType::View => {
                    action
                        .url
                        .as_ref()
                        .map(|url| AdaptiveCardAction::ActionOpenUrl {
                            title: action.label.clone(),
                            url: url.clone(),
                        })
                }
                ActionType::Approve | ActionType::Reject => {
                    Some(AdaptiveCardAction::ActionSubmit {
                        title: action.label.clone(),
                        data: serde_json::json!({
                            "notification_id": notification.id,
                            "action_type": action.action_type,
                            "payload": action.payload,
                        }),
                    })
                }
                _ => None,
            })
            .collect();

        AdaptiveCard {
            type_: "AdaptiveCard".to_string(),
            version: "1.2".to_string(),
            body,
            actions,
        }
    }

    /// Validate webhook URL format
    pub fn validate_webhook_url(url: &str) -> Result<(), TeamsError> {
        if !url.starts_with("https://outlook.office.com/webhook/") {
            return Err(TeamsError::InvalidWebhookUrl);
        }
        Ok(())
    }
}

#[async_trait]
impl NotificationProvider for TeamsClient {
    async fn send(
        &self,
        notification: &Notification,
    ) -> Result<NotificationResult, NotificationError> {
        // Placeholder - actual implementation would:
        // 1. Fetch user's Teams webhook URL from database
        // 2. Send using send_adaptive_card()
        // 3. Return result

        Err(NotificationError::NotConfigured(
            notification.user_id.clone(),
        ))
    }

    fn provider_name(&self) -> &'static str {
        "teams"
    }

    async fn is_configured(&self, user_id: &UserId) -> Result<bool, NotificationError> {
        // Placeholder - would check database
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_webhook_payload() {
        let client = TeamsClient::new(TeamsConfig {
            tenant_id: None,
            client_id: None,
            client_secret: None,
        });

        let notification = Notification::new(
            UserId(Uuid::new_v4()),
            crate::NotificationType::ApprovalRequest,
            "Invoice Approval".to_string(),
            "Invoice #123 requires approval".to_string(),
        )
        .with_action(
            NotificationAction::new("View".to_string(), ActionType::View)
                .with_url("https://example.com/invoice/123".to_string()),
        );

        let payload = client.build_webhook_payload(&notification);

        assert_eq!(payload.summary, "Invoice Approval");
        assert!(!payload.sections.is_empty());
    }

    #[test]
    fn test_build_adaptive_card() {
        let client = TeamsClient::new(TeamsConfig {
            tenant_id: None,
            client_id: None,
            client_secret: None,
        });

        let notification = Notification::new(
            UserId(Uuid::new_v4()),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        )
        .with_action(NotificationAction::new(
            "Approve".to_string(),
            ActionType::Approve,
        ));

        let card = client.build_adaptive_card(&notification);

        assert!(!card.body.is_empty());
    }

    #[test]
    fn test_validate_webhook_url() {
        assert!(
            TeamsClient::validate_webhook_url("https://outlook.office.com/webhook/abc123").is_ok()
        );

        assert!(TeamsClient::validate_webhook_url("https://example.com/webhook").is_err());
    }
}
