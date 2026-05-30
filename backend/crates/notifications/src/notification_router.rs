//! Notification router
//!
//! Routes notifications to the appropriate channels based on user preferences.
//! Supports multi-channel delivery (Slack, Teams, Email, Push) with fallback logic.

use crate::{
    slack::{SlackClient, SlackConfig, SlackError},
    teams::{TeamsClient, TeamsConfig},
    Notification, NotificationChannel, NotificationError, NotificationPriority,
    NotificationProvider, NotificationResult,
};
use async_trait::async_trait;
use billforge_core::UserId;
use billforge_mobile_push::{
    ApnsClient, ApnsConfig, ApnsError, DevicePlatform, FcmClient, FcmConfig, FcmError,
    PushNotificationProvider,
};
use chrono::{NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Routes notifications to appropriate channels
pub struct NotificationRouter {
    slack_client: Option<Arc<SlackClient>>,
    teams_client: Option<Arc<TeamsClient>>,
    fcm_client: Option<Arc<dyn PushNotificationProvider>>,
    apns_client: Option<Arc<dyn PushNotificationProvider>>,
    push_token_store: Option<Arc<dyn PushDeviceTokenStore>>,
    in_app_store: Option<Arc<dyn InAppNotificationStore>>,
    providers: HashMap<String, Arc<dyn NotificationProvider>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushDeviceToken {
    pub token: String,
    pub platform: DevicePlatform,
}

#[async_trait]
pub trait PushDeviceTokenStore: Send + Sync {
    async fn tokens_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<PushDeviceToken>, NotificationError>;
}

#[async_trait]
pub trait InAppNotificationStore: Send + Sync {
    async fn persist(&self, notification: &Notification) -> Result<Uuid, NotificationError>;
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
            push_token_store: None,
            in_app_store: None,
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
    pub fn with_fcm(self, config: FcmConfig) -> Result<Self, FcmError> {
        let client: Arc<dyn PushNotificationProvider> =
            Arc::new(FcmClient::new(config)?);
        Ok(self.with_fcm_provider(client))
    }

    /// Configure APNS (Apple Push Notification Service) integration
    pub fn with_apns(self, config: ApnsConfig) -> Result<Self, ApnsError> {
        let client: Arc<dyn PushNotificationProvider> =
            Arc::new(ApnsClient::new(config)?);
        Ok(self.with_apns_provider(client))
    }

    /// Configure FCM using a custom push provider (for testing)
    pub fn with_fcm_provider(
        mut self,
        provider: Arc<dyn PushNotificationProvider>,
    ) -> Self {
        self.fcm_client = Some(provider);
        self
    }

    /// Configure APNS using a custom push provider (for testing)
    pub fn with_apns_provider(
        mut self,
        provider: Arc<dyn PushNotificationProvider>,
    ) -> Self {
        self.apns_client = Some(provider);
        self
    }

    pub fn with_push_token_store(mut self, store: Arc<dyn PushDeviceTokenStore>) -> Self {
        self.push_token_store = Some(store);
        self
    }

    pub fn with_in_app_store(mut self, store: Arc<dyn InAppNotificationStore>) -> Self {
        self.in_app_store = Some(store);
        self
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
                return self.send_push(notification, channel).await;
            }
            NotificationChannel::InApp => {
                return self.persist_in_app(notification, channel).await;
            }
        };

        let provider = self.providers.get(provider_key).ok_or_else(|| {
            NotificationError::Unknown(format!("Provider {} not configured", provider_key))
        })?;

        provider.send(notification).await
    }

    async fn send_push(
        &self,
        notification: &Notification,
        channel: NotificationChannel,
    ) -> Result<NotificationResult, NotificationError> {
        if self.fcm_client.is_none() && self.apns_client.is_none() {
            return Ok(NotificationResult {
                success: false,
                channel,
                external_id: None,
                error_message: Some("No push notification providers configured".to_string()),
            });
        }

        let Some(token_store) = &self.push_token_store else {
            return Ok(NotificationResult {
                success: false,
                channel,
                external_id: None,
                error_message: Some("No push device token store configured".to_string()),
            });
        };

        let tokens = token_store.tokens_for_user(&notification.user_id).await?;
        if tokens.is_empty() {
            return Ok(NotificationResult {
                success: false,
                channel,
                external_id: None,
                error_message: Some("No push device tokens for user".to_string()),
            });
        }

        let mut successes = Vec::new();
        let mut errors = Vec::new();

        for device in tokens {
            let result = match device.platform {
                DevicePlatform::Android => match &self.fcm_client {
                    Some(client) => client
                        .send(
                            &device.token,
                            &notification.title,
                            &notification.message,
                            Some(notification.metadata.clone()),
                        )
                        .await
                        .map_err(|e| NotificationError::Unknown(e.to_string())),
                    None => {
                        errors.push("FCM provider not configured".to_string());
                        continue;
                    }
                },
                DevicePlatform::Ios => match &self.apns_client {
                    Some(client) => client
                        .send(
                            &device.token,
                            &notification.title,
                            &notification.message,
                            Some(notification.metadata.clone()),
                        )
                        .await
                        .map_err(|e| NotificationError::Unknown(e.to_string())),
                    None => {
                        errors.push("APNS provider not configured".to_string());
                        continue;
                    }
                },
            };

            match result {
                Ok(push_result) if push_result.success => {
                    successes.push(
                        push_result
                            .message_id
                            .unwrap_or_else(|| notification.id.to_string()),
                    );
                }
                Ok(push_result) => {
                    errors.push(push_result.error_message.unwrap_or_else(|| {
                        format!(
                            "{} push provider returned unsuccessful result",
                            device.platform
                        )
                    }));
                }
                Err(e) => errors.push(e.to_string()),
            }
        }

        if successes.is_empty() {
            Ok(NotificationResult {
                success: false,
                channel,
                external_id: None,
                error_message: Some(errors.join("; ")),
            })
        } else {
            Ok(NotificationResult {
                success: true,
                channel,
                external_id: Some(successes.join(",")),
                error_message: None,
            })
        }
    }

    async fn persist_in_app(
        &self,
        notification: &Notification,
        channel: NotificationChannel,
    ) -> Result<NotificationResult, NotificationError> {
        let Some(store) = &self.in_app_store else {
            return Ok(NotificationResult {
                success: false,
                channel,
                external_id: None,
                error_message: Some("No in-app notification store configured".to_string()),
            });
        };

        let persisted_id = store.persist(notification).await?;
        Ok(NotificationResult {
            success: true,
            channel,
            external_id: Some(persisted_id.to_string()),
            error_message: None,
        })
    }

    /// Check if current time is in quiet hours
    fn is_quiet_hours(&self, preferences: &NotificationPreference) -> bool {
        self.is_quiet_hours_at(preferences, chrono::Utc::now())
    }

    fn is_quiet_hours_at(
        &self,
        preferences: &NotificationPreference,
        now_utc: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        let (Some(start), Some(end)) = (
            preferences.quiet_hours_start.as_deref(),
            preferences.quiet_hours_end.as_deref(),
        ) else {
            return false;
        };

        let Some(start) = parse_quiet_time(start) else {
            return false;
        };
        let Some(end) = parse_quiet_time(end) else {
            return false;
        };

        let local_time = preferences
            .quiet_hours_timezone
            .as_deref()
            .and_then(|timezone| timezone.parse::<chrono_tz::Tz>().ok())
            .map(|timezone| now_utc.with_timezone(&timezone).time())
            .unwrap_or_else(|| now_utc.time());

        if start <= end {
            local_time >= start && local_time < end
        } else {
            local_time >= start || local_time < end
        }
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

fn parse_quiet_time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .ok()
        .map(|time| {
            NaiveTime::from_hms_opt(time.hour(), time.minute(), time.second())
                .expect("parsed NaiveTime components are valid")
        })
}

impl Default for NotificationRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MemoryInAppStore {
        persisted: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl InAppNotificationStore for MemoryInAppStore {
        async fn persist(&self, notification: &Notification) -> Result<Uuid, NotificationError> {
            self.persisted.lock().unwrap().push(notification.id);
            Ok(notification.id)
        }
    }

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

    #[tokio::test]
    async fn test_in_app_requires_store() {
        let router = NotificationRouter::new();
        let user_id = UserId(Uuid::new_v4());
        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );
        let mut preferences = NotificationRouter::default_preferences(user_id);
        preferences.channels = vec![ChannelPreference {
            channel: NotificationChannel::InApp,
            enabled: true,
            notification_types: vec![],
            priority_filter: None,
        }];

        let result = router.route(&notification, &preferences).await.unwrap();

        assert!(!result.delivered);
        assert_eq!(result.results.len(), 1);
        assert!(!result.results[0].success);
    }

    #[tokio::test]
    async fn test_in_app_success_requires_persistence() {
        let store = Arc::new(MemoryInAppStore {
            persisted: Mutex::new(vec![]),
        });
        let router = NotificationRouter::new().with_in_app_store(store.clone());
        let user_id = UserId(Uuid::new_v4());
        let notification = Notification::new(
            user_id.clone(),
            crate::NotificationType::ApprovalRequest,
            "Test".to_string(),
            "Test message".to_string(),
        );
        let mut preferences = NotificationRouter::default_preferences(user_id);
        preferences.channels = vec![ChannelPreference {
            channel: NotificationChannel::InApp,
            enabled: true,
            notification_types: vec![],
            priority_filter: None,
        }];

        let result = router.route(&notification, &preferences).await.unwrap();

        assert!(result.delivered);
        assert_eq!(
            store.persisted.lock().unwrap().as_slice(),
            &[notification.id]
        );
    }

    #[test]
    fn test_quiet_hours_supports_overnight_window() {
        let router = NotificationRouter::new();
        let user_id = UserId(Uuid::new_v4());
        let mut preferences = NotificationRouter::default_preferences(user_id);
        preferences.quiet_hours_start = Some("22:00".to_string());
        preferences.quiet_hours_end = Some("07:00".to_string());
        preferences.quiet_hours_timezone = Some("UTC".to_string());

        let quiet_now = chrono::DateTime::parse_from_rfc3339("2026-05-29T23:30:00Z").unwrap();
        let active_now = chrono::DateTime::parse_from_rfc3339("2026-05-29T12:30:00Z").unwrap();

        assert!(router.is_quiet_hours_at(&preferences, quiet_now.into()));
        assert!(!router.is_quiet_hours_at(&preferences, active_now.into()));
    }
}
