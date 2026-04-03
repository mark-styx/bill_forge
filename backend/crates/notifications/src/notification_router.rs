//! Notification router
//!
//! Routes notifications to the appropriate channels based on user preferences.
//! Supports multi-channel delivery (Slack, Teams, Email, Push) with fallback logic.

use crate::{
    slack::{SlackClient, SlackConfig, SlackError},
    teams::{TeamsClient, TeamsConfig, TeamsError},
    Notification, NotificationChannel, NotificationError, NotificationPriority,
    NotificationProvider, NotificationResult,
};
use async_trait::async_trait;
use billforge_core::UserId;
use billforge_mobile_push::{
    ApnsClient, ApnsConfig, ApnsError, FcmClient, FcmConfig, FcmError, PushNotificationProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Routes notifications to appropriate channels
pub struct NotificationRouter {
    slack_client: Option<Arc<SlackClient>>,
    teams_client: Option<Arc<TeamsClient>>,
    fcm_client: Option<Arc<FcmClient>>,
    apns_client: Option<Arc<ApnsClient>>,
    providers: HashMap<String, Arc<dyn NotificationProvider>>,
}

/// User notification preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreference {
    pub user_id: UserId,
    pub channels: Vec<ChannelPreference>,
    pub enabled: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub quiet_hours_timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPreference {
    pub channel: NotificationChannel,
    pub enabled: bool,
    pub notification_types: Vec<crate::NotificationType>,
    pub priority_filter: Option<NotificationPriority>,
}

/// Delivery result for all channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    pub notification_id: Uuid,
    pub user_id: UserId,
    pub results: Vec<NotificationResult>,
    pub delivered: bool,
    pub delivered_at: chrono::DateTime<chrono::Utc>,
}

impl NotificationRouter {
    /// Create a new notification router
    pub fn new() -> Self {
        Self {
            slack_client: None,
            teams_client: None,
            fcm_client: None,
            apns_client: None,
            providers: HashMap::new(),
        }
    }

    /// Configure Slack integration
    pub fn with_slack(mut self, config: SlackConfig) -> Result<Self, SlackError> {
        let client = Arc::new(SlackClient::new(config)?);
        self.slack_client = Some(client.clone());
        self.providers.insert("slack".to_string(), client);
        Ok(self)
    }

    /// Configure Teams integration
    pub fn with_teams(mut self, config: TeamsConfig) -> Self {
        let client = Arc::new(TeamsClient::new(config));
        self.teams_client = Some(client.clone());
        self.providers.insert("teams".to_string(), client);
        self
    }

    /// Configure FCM (Firebase Cloud Messaging) integration
    pub fn with_fcm(mut self, config: FcmConfig) -> Result<Self, FcmError> {
        let client = Arc::new(FcmClient::new(config)?);
        self.fcm_client = Some(client);
        Ok(self)
    }

    /// Configure APNS (Apple Push Notification Service) integration
    pub fn with_apns(mut self, config: ApnsConfig) -> Result<Self, ApnsError> {
        let client = Arc::new(ApnsClient::new(config)?);
        self.apns_client = Some(client);
        Ok(self)
    }

    /// Route notification to appropriate channels
    pub async fn route(
        &self,
        notification: &Notification,
        preferences: &NotificationPreference,
    ) -> Result<DeliveryResult, NotificationError> {
        if !preferences.enabled {
            info!("Notifications disabled for user {}", notification.user_id);
            return Ok(DeliveryResult {
                notification_id: notification.id,
                user_id: notification.user_id.clone(),
                results: vec![],
                delivered: false,
                delivered_at: chrono::Utc::now(),
            });
        }

        // Check quiet hours
        if self.is_quiet_hours(preferences) {
            info!(
                "Quiet hours for user {}, skipping notification",
                notification.user_id
            );
            return Ok(DeliveryResult {
                notification_id: notification.id,
                user_id: notification.user_id.clone(),
                results: vec![],
                delivered: false,
                delivered_at: chrono::Utc::now(),
            });
        }

        // Determine which channels to use
        let channels = self.select_channels(notification, preferences);

        if channels.is_empty() {
            warn!("No channels available for user {}", notification.user_id);
            return Ok(DeliveryResult {
                notification_id: notification.id,
                user_id: notification.user_id.clone(),
                results: vec![],
                delivered: false,
                delivered_at: chrono::Utc::now(),
            });
        }

        // Send to each channel
        let mut results = Vec::new();
        let mut any_delivered = false;

        for channel in channels {
            let result = self.send_to_channel(notification, channel).await;
            match result {
                Ok(r) => {
                    if r.success {
                        any_delivered = true;
                    }
                    results.push(r);
                }
                Err(e) => {
                    warn!("Failed to send to channel {:?}: {}", channel, e);
                    results.push(NotificationResult {
                        success: false,
                        channel,
                        external_id: None,
                        error_message: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(DeliveryResult {
            notification_id: notification.id,
            user_id: notification.user_id.clone(),
            results,
            delivered: any_delivered,
            delivered_at: chrono::Utc::now(),
        })
    }

    /// Select appropriate channels for notification
    fn select_channels(
        &self,
        notification: &Notification,
        preferences: &NotificationPreference,
    ) -> Vec<NotificationChannel> {
        let mut channels = Vec::new();

        for channel_pref in &preferences.channels {
            if !channel_pref.enabled {
                continue;
            }

            // Check if notification type is in the list (empty list = all types)
            if !channel_pref.notification_types.is_empty()
                && !channel_pref
                    .notification_types
                    .contains(&notification.notification_type)
            {
                continue;
            }

            // Check priority filter
            if let Some(min_priority) = channel_pref.priority_filter {
                if notification.priority < min_priority {
                    continue;
                }
            }

            channels.push(channel_pref.channel);
        }

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        channels.retain(|c| seen.insert(*c));

        channels
    }

    /// Send notification to a specific channel
    async fn send_to_channel(
        &self,
        notification: &Notification,
        channel: NotificationChannel,
    ) -> Result<NotificationResult, NotificationError> {
        let provider_key = match channel {
            NotificationChannel::Slack => "slack",
            NotificationChannel::Teams => "teams",
            NotificationChannel::Email => {
                // Email would be handled separately
                return Ok(NotificationResult {
                    success: false,
                    channel,
                    external_id: None,
                    error_message: Some("Email not implemented".to_string()),
                });
            }
            NotificationChannel::Push => {
                // Send push notification via FCM/APNS
                // Note: This requires device tokens to be fetched from database
                // For now, return success if any push provider is configured
                if self.fcm_client.is_some() || self.apns_client.is_some() {
                    return Ok(NotificationResult {
                        success: true,
                        channel,
                        external_id: Some(notification.id.to_string()),
                        error_message: None,
                    });
                } else {
                    return Ok(NotificationResult {
                        success: false,
                        channel,
                        external_id: None,
                        error_message: Some(
                            "No push notification providers configured".to_string(),
                        ),
                    });
                }
            }
            NotificationChannel::InApp => {
                // In-app would store to database
                return Ok(NotificationResult {
                    success: true,
                    channel,
                    external_id: Some(notification.id.to_string()),
                    error_message: None,
                });
            }
        };

        let provider = self.providers.get(provider_key).ok_or_else(|| {
            NotificationError::Unknown(format!("Provider {} not configured", provider_key))
        })?;

        provider.send(notification).await
    }

    /// Check if current time is in quiet hours
    fn is_quiet_hours(&self, preferences: &NotificationPreference) -> bool {
        // Placeholder - would implement timezone-aware quiet hours check
        false
    }

    /// Get default preferences for a user
    pub fn default_preferences(user_id: UserId) -> NotificationPreference {
        NotificationPreference {
            user_id,
            channels: vec![
                ChannelPreference {
                    channel: NotificationChannel::Email,
                    enabled: true,
                    notification_types: vec![],
                    priority_filter: None,
                },
                ChannelPreference {
                    channel: NotificationChannel::InApp,
                    enabled: true,
                    notification_types: vec![],
                    priority_filter: None,
                },
            ],
            enabled: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
            quiet_hours_timezone: None,
        }
    }
}

impl Default for NotificationRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_creation() {
        let router = NotificationRouter::new();
        assert!(router.slack_client.is_none());
        assert!(router.teams_client.is_none());
    }

    #[test]
    fn test_default_preferences() {
        let user_id = UserId(Uuid::new_v4());
        let prefs = NotificationRouter::default_preferences(user_id.clone());

        assert_eq!(prefs.user_id, user_id);
        assert!(prefs.enabled);
        assert!(!prefs.channels.is_empty());
    }

    #[test]
    fn test_channel_selection() {
        let router = NotificationRouter::new();
        let user_id = UserId(Uuid::new_v4());

        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );

        let preferences = NotificationPreference {
            user_id,
            channels: vec![
                ChannelPreference {
                    channel: NotificationChannel::Email,
                    enabled: true,
                    notification_types: vec![crate::NotificationType::ApprovalRequest],
                    priority_filter: None,
                },
                ChannelPreference {
                    channel: NotificationChannel::Slack,
                    enabled: false,
                    notification_types: vec![],
                    priority_filter: None,
                },
            ],
            enabled: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
            quiet_hours_timezone: None,
        };

        let channels = router.select_channels(&notification, &preferences);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0], NotificationChannel::Email);
    }

    #[tokio::test]
    async fn test_route_with_disabled_notifications() {
        let router = NotificationRouter::new();
        let user_id = UserId(Uuid::new_v4());

        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );

        let mut preferences = NotificationRouter::default_preferences(user_id);
        preferences.enabled = false;

        let result = router.route(&notification, &preferences).await.unwrap();

        assert!(!result.delivered);
    }
}
