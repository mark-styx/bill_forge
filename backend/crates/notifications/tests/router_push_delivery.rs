//! Integration tests for the notification router's Push delivery chain.
//!
//! Covers: Android→FCM / iOS→APNS dispatch, token-store fan-out,
//! partial-success aggregation, missing-provider branches, missing-token-store,
//! empty tokens, quiet-hours blocking, and channel-disabled skipping.

use async_trait::async_trait;
use billforge_core::UserId;
use billforge_mobile_push::{DevicePlatform, PushNotificationProvider, PushResult};
use billforge_notifications::{
    ChannelPreference, InAppNotificationStore, Notification, NotificationChannel,
    NotificationError, NotificationPreference, NotificationRouter, NotificationType,
    PushDeviceToken, PushDeviceTokenStore,
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock push provider
// ---------------------------------------------------------------------------

/// Records every `send` call and returns scripted responses in order.
struct MockPushProvider {
    calls: Mutex<Vec<PushCall>>,
    responses: Mutex<Vec<PushResult>>,
    provider_name: &'static str,
}

#[allow(dead_code)]
#[derive(Clone)]
struct PushCall {
    device_token: String,
    title: String,
    message: String,
}

impl MockPushProvider {
    fn new(name: &'static str, responses: Vec<PushResult>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
            provider_name: name,
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl PushNotificationProvider for MockPushProvider {
    async fn send(
        &self,
        device_token: &str,
        title: &str,
        message: &str,
        _data: Option<serde_json::Value>,
    ) -> Result<PushResult, billforge_mobile_push::PushError> {
        self.calls.lock().unwrap().push(PushCall {
            device_token: device_token.to_string(),
            title: title.to_string(),
            message: message.to_string(),
        });
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            panic!("MockPushProvider ran out of scripted responses");
        }
        Ok(responses.remove(0))
    }

    fn provider_name(&self) -> &'static str {
        self.provider_name
    }
}

// ---------------------------------------------------------------------------
// Mock token store
// ---------------------------------------------------------------------------

struct MockTokenStore {
    tokens: Vec<PushDeviceToken>,
}

impl MockTokenStore {
    fn new(tokens: Vec<PushDeviceToken>) -> Self {
        Self { tokens }
    }
}

#[async_trait]
impl PushDeviceTokenStore for MockTokenStore {
    async fn tokens_for_user(
        &self,
        _user_id: &UserId,
    ) -> Result<Vec<PushDeviceToken>, NotificationError> {
        Ok(self.tokens.clone())
    }
}

// ---------------------------------------------------------------------------
// Mock in-app store
// ---------------------------------------------------------------------------

struct MockInAppStore {
    persisted: Mutex<Vec<Uuid>>,
}

impl MockInAppStore {
    fn new() -> Self {
        Self {
            persisted: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl InAppNotificationStore for MockInAppStore {
    async fn persist(&self, notification: &Notification) -> Result<Uuid, NotificationError> {
        self.persisted.lock().unwrap().push(notification.id);
        Ok(notification.id)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_user_id() -> UserId {
    UserId(Uuid::new_v4())
}

fn test_notification(user_id: UserId) -> Notification {
    Notification::new(
        user_id,
        NotificationType::ApprovalRequest,
        "Approval needed".to_string(),
        "Invoice #123 requires your approval".to_string(),
    )
}

fn push_channel_prefs() -> Vec<ChannelPreference> {
    vec![ChannelPreference {
        channel: NotificationChannel::Push,
        enabled: true,
        notification_types: vec![],
        priority_filter: None,
    }]
}

fn enabled_prefs(user_id: UserId, channels: Vec<ChannelPreference>) -> NotificationPreference {
    NotificationPreference {
        user_id,
        channels,
        enabled: true,
        quiet_hours_start: None,
        quiet_hours_end: None,
        quiet_hours_timezone: None,
    }
}

fn fcm_ok(msg_id: &str) -> PushResult {
    PushResult {
        success: true,
        message_id: Some(msg_id.to_string()),
        error_message: None,
    }
}

fn fcm_fail(err: &str) -> PushResult {
    PushResult {
        success: false,
        message_id: None,
        error_message: Some(err.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_routes_android_token_to_fcm_only() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![fcm_ok("fcm_msg_1")]));
    let apns = Arc::new(MockPushProvider::new("apns", vec![]));
    // Clone the Arc (ref-counted) so we keep a handle for assertions after
    // handing ownership to the router.
    let fcm_handle = fcm.clone();
    let apns_handle = apns.clone();

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-android-token-1".to_string(),
        platform: DevicePlatform::Android,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_apns_provider(apns)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(result.delivered);
    assert_eq!(result.results.len(), 1);
    let push_result = &result.results[0];
    assert!(push_result.success);
    assert_eq!(push_result.external_id.as_deref(), Some("fcm_msg_1"));

    assert_eq!(fcm_handle.call_count(), 1);
    assert_eq!(apns_handle.call_count(), 0);
}

#[tokio::test]
async fn push_routes_ios_token_to_apns_only() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![]));
    let apns = Arc::new(MockPushProvider::new("apns", vec![fcm_ok("apns_msg_1")]));
    let fcm_handle = fcm.clone();
    let apns_handle = apns.clone();

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-ios-token-1".to_string(),
        platform: DevicePlatform::Ios,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_apns_provider(apns)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(result.results[0].success);
    assert_eq!(result.results[0].external_id.as_deref(), Some("apns_msg_1"));

    assert_eq!(fcm_handle.call_count(), 0);
    assert_eq!(apns_handle.call_count(), 1);
}

#[tokio::test]
async fn push_fans_out_to_both_platforms() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![fcm_ok("fcm_msg_1")]));
    let apns = Arc::new(MockPushProvider::new("apns", vec![fcm_ok("apns_msg_1")]));
    let fcm_handle = fcm.clone();
    let apns_handle = apns.clone();

    let store = MockTokenStore::new(vec![
        PushDeviceToken {
            token: "test-android-token-1".to_string(),
            platform: DevicePlatform::Android,
        },
        PushDeviceToken {
            token: "test-ios-token-1".to_string(),
            platform: DevicePlatform::Ios,
        },
    ]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_apns_provider(apns)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(result.delivered);
    assert_eq!(result.results.len(), 1);
    let push_result = &result.results[0];
    assert!(push_result.success);
    // Both message ids should appear comma-joined
    let ext = push_result.external_id.as_deref().unwrap();
    assert!(ext.contains("fcm_msg_1"));
    assert!(ext.contains("apns_msg_1"));

    assert_eq!(fcm_handle.call_count(), 1);
    assert_eq!(apns_handle.call_count(), 1);
}

#[tokio::test]
async fn push_partial_success_aggregates_as_success() {
    // Two Android tokens: first succeeds, second fails
    let fcm = Arc::new(MockPushProvider::new(
        "fcm",
        vec![fcm_ok("fcm_msg_1"), fcm_fail("device unregistered")],
    ));
    let fcm_handle = fcm.clone();

    let store = MockTokenStore::new(vec![
        PushDeviceToken {
            token: "test-android-1".to_string(),
            platform: DevicePlatform::Android,
        },
        PushDeviceToken {
            token: "test-android-2".to_string(),
            platform: DevicePlatform::Android,
        },
    ]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(result.results[0].success);
    assert!(result.results[0].external_id.is_some());

    assert_eq!(fcm_handle.call_count(), 2);
}

#[tokio::test]
async fn push_all_failures_aggregate_as_failure_with_combined_messages() {
    let fcm = Arc::new(MockPushProvider::new(
        "fcm",
        vec![fcm_fail("timeout"), fcm_fail("invalid token")],
    ));
    let fcm_handle = fcm.clone();

    let store = MockTokenStore::new(vec![
        PushDeviceToken {
            token: "test-android-1".to_string(),
            platform: DevicePlatform::Android,
        },
        PushDeviceToken {
            token: "test-android-2".to_string(),
            platform: DevicePlatform::Android,
        },
    ]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    let push_result = &result.results[0];
    assert!(!push_result.success);
    let err = push_result.error_message.as_deref().unwrap();
    assert!(err.contains("timeout"));
    assert!(err.contains("invalid token"));

    assert_eq!(fcm_handle.call_count(), 2);
}

#[tokio::test]
async fn push_missing_fcm_provider_with_android_token_records_error() {
    let apns = Arc::new(MockPushProvider::new("apns", vec![]));

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-android-1".to_string(),
        platform: DevicePlatform::Android,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_apns_provider(apns.clone())
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].success);
    let err = result.results[0].error_message.as_deref().unwrap();
    assert!(err.contains("FCM provider not configured"));
}

#[tokio::test]
async fn push_missing_apns_provider_with_ios_token_records_error() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![]));

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-ios-1".to_string(),
        platform: DevicePlatform::Ios,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm.clone())
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].success);
    let err = result.results[0].error_message.as_deref().unwrap();
    assert!(err.contains("APNS provider not configured"));
}

#[tokio::test]
async fn push_no_providers_configured_returns_explicit_error() {
    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-android-1".to_string(),
        platform: DevicePlatform::Android,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new().with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].success);
    let err = result.results[0].error_message.as_deref().unwrap();
    assert_eq!(err, "No push notification providers configured");
}

#[tokio::test]
async fn push_no_token_store_returns_explicit_error() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![]));

    let user_id = test_user_id();
    let router = NotificationRouter::new().with_fcm_provider(fcm);

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].success);
    let err = result.results[0].error_message.as_deref().unwrap();
    assert_eq!(err, "No push device token store configured");
}

#[tokio::test]
async fn push_empty_tokens_returns_explicit_error() {
    let fcm = Arc::new(MockPushProvider::new("fcm", vec![]));
    let fcm_handle = fcm.clone();

    let store = MockTokenStore::new(vec![]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_push_token_store(Arc::new(store));

    let result = router
        .route(
            &test_notification(user_id.clone()),
            &enabled_prefs(user_id, push_channel_prefs()),
        )
        .await
        .unwrap();

    assert!(!result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].success);
    let err = result.results[0].error_message.as_deref().unwrap();
    assert_eq!(err, "No push device tokens for user");

    // Provider never called
    assert_eq!(fcm_handle.call_count(), 0);
}

#[tokio::test]
async fn push_blocked_by_quiet_hours() {
    let fcm = Arc::new(MockPushProvider::new(
        "fcm",
        vec![fcm_ok("should_not_be_called")],
    ));
    let fcm_handle = fcm.clone();

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-android-1".to_string(),
        platform: DevicePlatform::Android,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_push_token_store(Arc::new(store));

    // Set quiet hours spanning the current UTC time so the notification is blocked.
    // Use a 3-hour window (start-1 → end+1) wide enough to absorb any clock motion
    // between the test sampling Utc::now() and the router doing the same, and to
    // cover the entire current hour (end is exclusive in is_quiet_hours_at).
    let now = chrono::Utc::now();
    let current_hour: u32 = now.format("%H").to_string().parse().unwrap();
    let start_hour = (current_hour + 23) % 24; // 1 hour before now
    let end_hour = (current_hour + 2) % 24; // 2 hours after now
    let start = format!("{start_hour:02}:00");
    let end = format!("{end_hour:02}:00");

    let prefs = NotificationPreference {
        user_id: user_id.clone(),
        channels: push_channel_prefs(),
        enabled: true,
        quiet_hours_start: Some(start),
        quiet_hours_end: Some(end),
        quiet_hours_timezone: Some("UTC".to_string()),
    };

    let result = router
        .route(&test_notification(user_id), &prefs)
        .await
        .unwrap();

    assert!(!result.delivered);
    assert!(result.results.is_empty());

    // Provider never called
    assert_eq!(fcm_handle.call_count(), 0);
}

#[tokio::test]
async fn push_channel_disabled_in_prefs_skips_push() {
    let fcm = Arc::new(MockPushProvider::new(
        "fcm",
        vec![fcm_ok("should_not_be_called")],
    ));
    let fcm_handle = fcm.clone();
    let in_app = Arc::new(MockInAppStore::new());
    let in_app_handle = in_app.clone();

    let store = MockTokenStore::new(vec![PushDeviceToken {
        token: "test-android-1".to_string(),
        platform: DevicePlatform::Android,
    }]);

    let user_id = test_user_id();
    let router = NotificationRouter::new()
        .with_fcm_provider(fcm)
        .with_push_token_store(Arc::new(store))
        .with_in_app_store(in_app);

    // Push disabled, InApp enabled
    let prefs = enabled_prefs(
        user_id.clone(),
        vec![
            ChannelPreference {
                channel: NotificationChannel::Push,
                enabled: false,
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
    );

    let result = router
        .route(&test_notification(user_id), &prefs)
        .await
        .unwrap();

    // In-App delivers successfully
    assert!(result.delivered);
    assert_eq!(result.results.len(), 1);
    assert!(result.results[0].success);
    assert_eq!(result.results[0].channel, NotificationChannel::InApp);

    // Push provider never called
    assert_eq!(fcm_handle.call_count(), 0);

    // In-App store was called
    assert_eq!(in_app_handle.persisted.lock().unwrap().len(), 1);
}
