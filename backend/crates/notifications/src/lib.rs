//! Notification system for BillForge
//!
//! Provides multi-channel notifications (Slack, Microsoft Teams, Email)
//! with actionable buttons and user preference management.

mod notification_router;
mod slack;
mod teams;

pub use notification_router::{NotificationPreference, NotificationRouter};
pub use slack::{SlackClient, SlackConfig, SlackError};
pub use teams::{TeamsClient, TeamsConfig, TeamsError};

use async_trait::async_trait;
use billforge_core::UserId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A notification to be sent to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: UserId,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub actions: Vec<NotificationAction>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub priority: NotificationPriority,
}

impl Notification {
    /// Create a new notification
    pub fn new(
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            notification_type,
            title,
            message,
            actions: Vec::new(),
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            priority: NotificationPriority::Normal,
        }
    }

    /// Add an action button
    pub fn with_action(mut self, action: NotificationAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// Types of notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    ApprovalRequest,
    ApprovalReminder,
    ApprovalCompleted,
    InvoiceUploaded,
    InvoiceRejected,
    PaymentDue,
    BudgetAlert,
    WeeklyDigest,
    SystemAlert,
}

/// Action button in a notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub label: String,
    pub action_type: ActionType,
    pub url: Option<String>,
    pub payload: Option<serde_json::Value>,
}

impl NotificationAction {
    pub fn new(label: String, action_type: ActionType) -> Self {
        Self {
            label,
            action_type,
            url: None,
            payload: None,
        }
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Types of actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Approve,
    Reject,
    View,
    Snooze,
    Delegate,
    Dismiss,
}

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

/// Notification channels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Slack,
    Teams,
    Email,
    Push,
    InApp,
}

/// Result of sending a notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub success: bool,
    pub channel: NotificationChannel,
    pub external_id: Option<String>,
    pub error_message: Option<String>,
}

/// Trait for notification providers
#[async_trait]
pub trait NotificationProvider: Send + Sync {
    /// Send a notification
    async fn send(
        &self,
        notification: &Notification,
    ) -> Result<NotificationResult, NotificationError>;

    /// Get the provider name
    fn provider_name(&self) -> &'static str;

    /// Check if the provider is configured for a user
    async fn is_configured(&self, user_id: &UserId) -> Result<bool, NotificationError>;
}

/// Errors from notification operations
#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Slack error: {0}")]
    Slack(#[from] SlackError),

    #[error("Teams error: {0}")]
    Teams(#[from] TeamsError),

    #[error("Provider not configured for user {0}")]
    NotConfigured(UserId),

    #[error("Invalid notification: {0}")]
    InvalidNotification(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_builder() {
        let user_id = UserId(Uuid::new_v4());
        let notification = Notification::new(
            user_id.clone(),
            NotificationType::ApprovalRequest,
            "Invoice Approval Required".to_string(),
            "Invoice #12345 from Acme Corp requires your approval".to_string(),
        )
        .with_action(NotificationAction::new(
            "Approve".to_string(),
            ActionType::Approve,
        ))
        .with_action(NotificationAction::new(
            "Reject".to_string(),
            ActionType::Reject,
        ))
        .with_priority(NotificationPriority::High);

        assert_eq!(notification.user_id, user_id);
        assert_eq!(notification.actions.len(), 2);
        assert_eq!(notification.priority, NotificationPriority::High);
    }
}
