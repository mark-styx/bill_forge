//! Microsoft Teams notification integration
//!
//! Provides webhook-based Teams notifications with Adaptive Cards
//! for rich formatting and actionable buttons.

use crate::{
    ActionType, InvoiceContext, InvoiceLineItem, Notification, NotificationChannel,
    NotificationError, NotificationProvider, NotificationResult,
};
use async_trait::async_trait;
use billforge_core::UserId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

/// Teams webhook client
#[allow(dead_code)]
pub struct TeamsClient {
    config: TeamsConfig,
    http_client: Client,
    /// Per-user webhook URL store. In production this is a database-backed store.
    webhook_store: Arc<dyn TeamsWebhookStore + Send + Sync>,
}

/// Abstraction over per-user Teams webhook URL storage.
pub trait TeamsWebhookStore: Send + Sync {
    fn get(&self, user_id: &UserId) -> Option<String>;
}

/// Simple in-memory store used by default and in tests.
pub struct InMemoryTeamsWebhookStore {
    webhooks: HashMap<UserId, String>,
}

impl InMemoryTeamsWebhookStore {
    pub fn new() -> Self {
        Self {
            webhooks: HashMap::new(),
        }
    }

    pub fn add(&mut self, user_id: UserId, webhook_url: String) {
        self.webhooks.insert(user_id, webhook_url);
    }
}

impl Default for InMemoryTeamsWebhookStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamsWebhookStore for InMemoryTeamsWebhookStore {
    fn get(&self, user_id: &UserId) -> Option<String> {
        self.webhooks.get(user_id).cloned()
    }
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
#[allow(dead_code)]
struct TeamsWebhookResponse {
    success: Option<bool>,
    error: Option<String>,
}

/// Teams user preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
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
    /// Create a new Teams client with default in-memory webhook store
    pub fn new(config: TeamsConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            webhook_store: Arc::new(InMemoryTeamsWebhookStore::new()),
        }
    }

    /// Create a new Teams client with a custom webhook store (e.g. database-backed)
    pub fn new_with_store(
        config: TeamsConfig,
        store: Arc<dyn TeamsWebhookStore + Send + Sync>,
    ) -> Self {
        Self {
            config,
            http_client: Client::new(),
            webhook_store: store,
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

    /// Send Adaptive Card (richer format).
    /// When the notification carries an `InvoiceContext` in metadata, the rich
    /// approval card with five action verbs is rendered instead of the legacy
    /// generic card builder.
    pub async fn send_adaptive_card(
        &self,
        webhook_url: &str,
        notification: &Notification,
    ) -> Result<String, TeamsError> {
        let payload = match notification.invoice_context() {
            Some(ctx) => build_teams_approval_card(&ctx),
            None => {
                // Legacy path
                let card = self.build_adaptive_card(notification);
                serde_json::json!({
                    "type": "message",
                    "attachments": [{
                        "contentType": "application/vnd.microsoft.card.adaptive",
                        "contentUrl": null,
                        "content": card
                    }]
                })
            }
        };

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
                    ActionType::View => action.url.as_ref().map(|url| TeamsAction {
                        type_: "OpenUri".to_string(),
                        name: action.label.clone(),
                        target: Some(vec![url.clone()]),
                        inputs: None,
                        body: None,
                        headers: None,
                    }),
                    ActionType::Approve | ActionType::Reject => {
                        // For actions that need server-side processing,
                        // we'd use Action.Http with a backend endpoint
                        action.url.as_ref().map(|_url| TeamsAction {
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
                    ActionType::RequestChanges | ActionType::Reassign | ActionType::Comment => {
                        // Additional approval actions — handled via dedicated
                        // chat approval endpoints, not legacy webhook payload.
                        None
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
                ActionType::RequestChanges | ActionType::Reassign | ActionType::Comment => {
                    // Additional approval actions — handled via dedicated
                    // chat approval endpoints, not legacy adaptive card.
                    None
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

// ---------------------------------------------------------------------------
// Standalone Teams conversation-reply helper (no TeamsClient required)
// ---------------------------------------------------------------------------

/// Post a simple text reply to a Teams channel via webhook.
///
/// Sends a plain Adaptive Card with a single TextBlock containing the
/// reply text. Useful for posting AI answers back into the conversation
/// where an approval card was sent.
pub async fn post_conversation_reply(
    webhook_url: &str,
    text: &str,
) -> Result<String, TeamsError> {
    let http_client = Client::new();
    let payload = serde_json::json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "contentUrl": null,
            "content": {
                "type": "AdaptiveCard",
                "version": "1.4",
                "body": [{
                    "type": "TextBlock",
                    "text": text,
                    "wrap": true
                }]
            }
        }]
    });

    let response = http_client
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
        warn!(
            "Teams conversation reply failed with status {}: {}",
            status, body
        );
        return Err(TeamsError::Webhook(format!("Status {}: {}", status, body)));
    }

    Ok("ok".to_string())
}

// ---------------------------------------------------------------------------
// Rich approval Adaptive Card builder for Teams
// ---------------------------------------------------------------------------

/// Base URL for the Teams action callback endpoint. Override via
/// `TEAMS_ACTIONS_URL` env var in production. A blank value (e.g. a
/// `TEAMS_ACTIONS_URL=` line copied verbatim from .env.example) is treated as
/// unset and falls back to the prod default; otherwise dotenv-loaded blanks
/// would produce cards with `url: ""` that silently 404.
fn teams_actions_base_url() -> String {
    std::env::var("TEAMS_ACTIONS_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.billforge.io/integrations/teams/actions".to_string())
}

/// Build a rich Teams Adaptive Card for invoice approval with five action verbs.
/// Returns the outer message envelope (type + attachment) ready to POST to the webhook.
pub fn build_teams_approval_card(ctx: &InvoiceContext) -> serde_json::Value {
    let mut body: Vec<serde_json::Value> = Vec::new();

    // Header
    let amount_dollars = ctx.total_amount_cents as f64 / 100.0;
    body.push(serde_json::json!({
        "type": "TextBlock",
        "text": format!("Invoice Approval Request"),
        "size": "Medium",
        "weight": "Bolder",
        "wrap": true
    }));
    body.push(serde_json::json!({
        "type": "TextBlock",
        "text": format!("**{}** — {} {:.*}", ctx.vendor_name, ctx.currency, 2, amount_dollars),
        "wrap": true
    }));

    // Fact set: invoice details
    let mut facts: Vec<serde_json::Value> = Vec::new();
    facts.push(serde_json::json!({
        "title": "Invoice #",
        "value": ctx.invoice_number
    }));
    if let Some(ref due) = ctx.due_date {
        facts.push(serde_json::json!({ "title": "Due Date", "value": due }));
    }
    if let Some(ref gl) = ctx.gl_code {
        facts.push(serde_json::json!({ "title": "GL Code", "value": gl }));
    }
    if let Some(ref cc) = ctx.cost_center {
        facts.push(serde_json::json!({ "title": "Cost Center", "value": cc }));
    }
    if !facts.is_empty() {
        body.push(serde_json::json!({ "type": "FactSet", "facts": facts }));
    }

    // Line items
    if !ctx.line_items.is_empty() {
        let display: Vec<&InvoiceLineItem> = ctx.line_items.iter().take(5).collect();
        let overflow = ctx.line_items.len().saturating_sub(5);
        let mut li_text = String::from("**Line Items:**\n");
        for item in &display {
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
        body.push(serde_json::json!({
            "type": "TextBlock",
            "text": li_text.trim_end(),
            "wrap": true
        }));
    }

    // PDF link
    if let Some(ref pdf_url) = ctx.pdf_preview_url {
        body.push(serde_json::json!({
            "type": "TextBlock",
            "text": format!("[View PDF]({})", pdf_url),
            "wrap": true
        }));
    }

    // Actions
    let base = teams_actions_base_url();
    let inv_id = ctx.invoice_id.to_string();
    let tenant_id = ctx.tenant_id.to_string();
    let actions = serde_json::json!({
        "type": "ActionSet",
        "actions": [
            {
                "type": "Action.Http",
                "title": "Approve",
                "method": "POST",
                "url": base,
                "body": serde_json::to_string(&serde_json::json!({
                    "action": "approve",
                    "invoice_id": inv_id,
                    "tenant_id": tenant_id
                })).unwrap()
            },
            {
                "type": "Action.Http",
                "title": "Reject",
                "method": "POST",
                "url": base,
                "body": serde_json::to_string(&serde_json::json!({
                    "action": "reject",
                    "invoice_id": inv_id,
                    "tenant_id": tenant_id
                })).unwrap()
            },
            {
                "type": "Action.Http",
                "title": "Request Changes",
                "method": "POST",
                "url": base,
                "body": serde_json::to_string(&serde_json::json!({
                    "action": "request_changes",
                    "invoice_id": inv_id,
                    "tenant_id": tenant_id
                })).unwrap()
            },
            {
                "type": "Action.ShowCard",
                "title": "Reassign",
                "card": {
                    "type": "AdaptiveCard",
                    "version": "1.2",
                    "body": [
                        {
                            "type": "Input.Text",
                            "id": "reassign_to_user_id",
                            "placeholder": "User ID to reassign to"
                        }
                    ],
                    "actions": [
                        {
                            "type": "Action.Http",
                            "title": "Submit",
                            "method": "POST",
                            "url": base,
                            "body": format!("{{\"action\":\"reassign\",\"invoice_id\":\"{}\",\"tenant_id\":\"{}\",\"reassign_to_user_id\":\"{{{{reassign_to_user_id.value}}}}\"}}", inv_id, tenant_id)
                        }
                    ]
                }
            },
            {
                "type": "Action.ShowCard",
                "title": "Comment",
                "card": {
                    "type": "AdaptiveCard",
                    "version": "1.2",
                    "body": [
                        {
                            "type": "Input.Text",
                            "id": "comment_body",
                            "isMultiline": true,
                            "placeholder": "Enter your comment"
                        }
                    ],
                    "actions": [
                        {
                            "type": "Action.Http",
                            "title": "Submit",
                            "method": "POST",
                            "url": base,
                            "body": format!("{{\"action\":\"comment\",\"invoice_id\":\"{}\",\"tenant_id\":\"{}\",\"comment_body\":\"{{{{comment_body.value}}}}\"}}", inv_id, tenant_id)
                        }
                    ]
                }
            }
        ]
    });

    let card = serde_json::json!({
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": body,
        "actions": [actions]
    });

    // Outer envelope
    serde_json::json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "contentUrl": null,
            "content": card
        }]
    })
}

#[async_trait]
impl NotificationProvider for TeamsClient {
    async fn send(
        &self,
        notification: &Notification,
    ) -> Result<NotificationResult, NotificationError> {
        let webhook_url = self
            .webhook_store
            .get(&notification.user_id)
            .ok_or_else(|| NotificationError::NotConfigured(notification.user_id.clone()))?;

        match self.send_adaptive_card(&webhook_url, notification).await {
            Ok(msg_id) => Ok(NotificationResult {
                success: true,
                channel: NotificationChannel::Teams,
                external_id: Some(msg_id),
                error_message: None,
            }),
            Err(e) => Ok(NotificationResult {
                success: false,
                channel: NotificationChannel::Teams,
                external_id: None,
                error_message: Some(e.to_string()),
            }),
        }
    }

    fn provider_name(&self) -> &'static str {
        "teams"
    }

    async fn is_configured(&self, _user_id: &UserId) -> Result<bool, NotificationError> {
        // Placeholder - would check database
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NotificationAction;
    use uuid::Uuid;

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
