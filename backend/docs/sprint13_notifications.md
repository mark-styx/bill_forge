# Sprint 13 Feature #5: Slack/Teams Notification Integration

## Overview

The Slack/Teams Notification Integration transforms email-only approval workflows into real-time, actionable notifications via Slack and Microsoft Teams, dramatically reducing approval time.

## Problem Statement

Email-only notifications cause significant delays in approval workflows:
- Inboxes overflow, causing important approvals to get buried
- No real-time awareness of pending approvals
- Requires clicking through to web interface to take action
- Average approval time: 48 hours (2 days)

This results in:
- Delayed vendor payments and strained relationships
- Cash flow unpredictability
- Missed early payment discounts
- Frustrated approvers and requesters

## Solution

Multi-channel notification system with:
1. **Slack Integration** (Day 1)
   - OAuth 2.0 installation per tenant
   - Interactive Block Kit messages with actionable buttons
   - DM notifications for individual approvers
   - Channel broadcasts for team awareness

2. **Microsoft Teams Integration** (Day 2)
   - Webhook-based notifications
   - Adaptive Cards for rich formatting
   - Actionable buttons via Power Automate
   - Per-user webhook configuration

3. **Notification Router** (Both Days)
   - User preference management
   - Multi-channel delivery with fallback
   - Quiet hours support
   - Priority-based routing

## Architecture

### Core Components

#### 1. Notification System (`crates/notifications/`)

**Main Types:**
```rust
pub struct Notification {
    pub id: Uuid,
    pub user_id: UserId,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub actions: Vec<NotificationAction>,
    pub metadata: serde_json::Value,
    pub priority: NotificationPriority,
}

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

pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}
```

**Key Features:**
- Builder pattern for easy construction
- Actionable buttons (Approve, Reject, View, Snooze, Delegate)
- Priority levels with visual indicators
- Rich metadata support

#### 2. Slack Client (`slack.rs`)

```rust
pub struct SlackClient {
    config: SlackConfig,
    http_client: Client,
}

impl SlackClient {
    pub fn get_authorize_url(&self, state: &SlackOAuthState) -> String;
    pub async fn exchange_code(&self, code: &str) -> Result<SlackOAuthResponse, SlackError>;
    pub async fn send_message(&self, access_token: &str, channel: &str, notification: &Notification) -> Result<(String, String), SlackError>;
    pub async fn open_im_channel(&self, access_token: &str, slack_user_id: &str) -> Result<String, SlackError>;
}
```

**Key Features:**
- OAuth 2.0 flow with state validation
- Block Kit message builder
- Interactive action buttons
- DM channel support
- Team-wide installation

#### 3. Teams Client (`teams.rs`)

```rust
pub struct TeamsClient {
    config: TeamsConfig,
    http_client: Client,
}

impl TeamsClient {
    pub async fn send_webhook(&self, webhook_url: &str, notification: &Notification) -> Result<String, TeamsError>;
    pub async fn send_adaptive_card(&self, webhook_url: &str, notification: &Notification) -> Result<String, TeamsError>;
    pub fn validate_webhook_url(url: &str) -> Result<(), TeamsError>;
}
```

**Key Features:**
- Webhook-based notifications
- Office 365 Connector Cards
- Adaptive Cards (richer format)
- Actionable buttons via HttpPOST
- Per-user webhook configuration

#### 4. Notification Router (`notification_router.rs`)

```rust
pub struct NotificationRouter {
    slack_client: Option<Arc<SlackClient>>,
    teams_client: Option<Arc<TeamsClient>>,
    providers: HashMap<String, Arc<dyn NotificationProvider>>,
}

impl NotificationRouter {
    pub async fn route(&self, notification: &Notification, preferences: &NotificationPreference) -> Result<DeliveryResult, NotificationError>;
}
```

**Key Features:**
- Multi-channel routing
- User preference enforcement
- Quiet hours support
- Fallback logic (Slack → Email → In-App)
- Priority-based filtering

### Database Schema

#### `slack_connections`
OAuth tokens and workspace information.

**Key Columns:**
- `slack_team_id`: Slack workspace ID
- `slack_user_id`: User who installed the integration
- `access_token`: Bot token for posting messages
- `bot_access_token`: User token for user-specific actions
- `scope`: Granted OAuth scopes

#### `teams_webhooks`
Per-user Teams webhook configurations.

**Key Columns:**
- `webhook_url`: Teams incoming webhook URL
- `channel_name`: Human-readable channel name
- `is_active`: Enable/disable flag

#### `user_notification_preferences`
User preferences for each channel.

**Key Columns:**
- `channel`: NotificationChannel (slack/teams/email/push/in_app)
- `enabled`: Is this channel active?
- `notification_types`: Which notification types to send
- `priority_filter`: Only send notifications >= this priority
- `quiet_hours_start/end`: Time window to suppress notifications

#### `notification_templates`
Customizable message templates per tenant.

**Key Columns:**
- `notification_type`: Type of notification
- `channel`: Target channel
- `title_template`: Handlebars template for title
- `body_template`: Handlebars template for body
- `is_default`: Use as default for new tenants

#### `notification_deliveries`
Delivery tracking and analytics.

**Key Columns:**
- `notification_id`: Reference to approval request
- `channel`: Which channel was used
- `external_id`: Platform-specific message ID
- `success`: Delivery status
- `delivered_at`: Timestamp
- `clicked_at`: When user clicked notification
- `action_taken`: Which button was clicked

## API Endpoints

### 1. Install Slack
```http
POST /api/v1/notifications/slack/install?redirect_url={url}
```

Initiates Slack OAuth flow.

**Response:**
```json
{
  "authorize_url": "https://slack.com/oauth/v2/authorize?...",
  "state": "uuid-state-token"
}
```

### 2. Slack OAuth Callback
```http
GET /api/v1/notifications/slack/callback?code={code}&state={state}
```

Completes OAuth flow and stores tokens.

**Response:**
```json
{
  "success": true,
  "slack_team_name": "Acme Corp Workspace"
}
```

### 3. Configure Teams Webhook
```http
POST /api/v1/notifications/teams/configure
```

Sets up Teams webhook for user.

**Request:**
```json
{
  "webhook_url": "https://outlook.office.com/webhook/...",
  "channel_name": "Invoice Approvals"
}
```

**Response:**
```json
{
  "success": true,
  "webhook_id": "uuid"
}
```

### 4. Update Preferences
```http
PUT /api/v1/notifications/preferences
```

Update user notification preferences.

**Request:**
```json
{
  "channel": "slack",
  "enabled": true,
  "notification_types": ["approval_request", "approval_reminder"],
  "priority_filter": "normal",
  "quiet_hours_start": "22:00:00",
  "quiet_hours_end": "07:00:00",
  "quiet_hours_timezone": "America/New_York"
}
```

### 5. Get Notification Status
```http
GET /api/v1/notifications/slack/status
GET /api/v1/notifications/teams/status
```

Check if integration is configured.

## Message Formats

### Slack Block Kit Message

```json
{
  "channel": "U12345",
  "text": "Invoice #12345 from Acme Corp requires your approval",
  "blocks": [
    {
      "type": "section",
      "text": {
        "type": "mrkdwn",
        "text": "*Invoice Approval Required*\nInvoice #12345 from Acme Corp for $5,000.00 requires your approval"
      }
    },
    {
      "type": "divider"
    },
    {
      "type": "actions",
      "elements": [
        {
          "type": "button",
          "text": { "type": "plain_text", "text": "Approve" },
          "action_id": "action_uuid_0",
          "value": "{\"notification_id\":\"...\",\"action_type\":\"approve\"}"
        },
        {
          "type": "button",
          "text": { "type": "plain_text", "text": "Reject" },
          "action_id": "action_uuid_1",
          "value": "{\"notification_id\":\"...\",\"action_type\":\"reject\"}"
        },
        {
          "type": "button",
          "text": { "type": "plain_text", "text": "View Details" },
          "action_id": "action_uuid_2",
          "url": "https://app.billforge.com/invoices/12345"
        }
      ]
    }
  ]
}
```

### Teams Adaptive Card

```json
{
  "type": "AdaptiveCard",
  "version": "1.2",
  "body": [
    {
      "type": "TextBlock",
      "text": "Invoice Approval Required",
      "size": "Medium",
      "weight": "Bolder"
    },
    {
      "type": "TextBlock",
      "text": "Invoice #12345 from Acme Corp for $5,000.00 requires your approval",
      "wrap": true
    },
    {
      "type": "FactSet",
      "facts": [
        { "title": "Vendor", "value": "Acme Corp" },
        { "title": "Amount", "value": "$5,000.00" },
        { "title": "Due Date", "value": "2026-03-15" }
      ]
    }
  ],
  "actions": [
    {
      "type": "ActionSubmit",
      "title": "Approve",
      "data": { "notification_id": "...", "action_type": "approve" }
    },
    {
      "type": "ActionSubmit",
      "title": "Reject",
      "data": { "notification_id": "...", "action_type": "reject" }
    },
    {
      "type": "ActionOpenUrl",
      "title": "View Details",
      "url": "https://app.billforge.com/invoices/12345"
    }
  ]
}
```

## Integration Points

### 1. Workflow Service Integration

When an approval request is created:

```rust
// In crates/invoice-processing/src/workflow_service.rs
async fn create_approval_request(...) -> Result<ApprovalRequest> {
    // Create approval request
    let request = sqlx::query!(...).fetch_one(...).await?;

    // Send notification
    let notification = Notification::new(
        approver_id.clone(),
        NotificationType::ApprovalRequest,
        "Invoice Approval Required".to_string(),
        format!("Invoice #{} from {} for {} requires your approval",
            invoice_number, vendor_name, amount),
    )
    .with_action(NotificationAction::new("Approve".to_string(), ActionType::Approve))
    .with_action(NotificationAction::new("Reject".to_string(), ActionType::Reject))
    .with_action(NotificationAction::new("View Details".to_string(), ActionType::View)
        .with_url(format!("{}/invoices/{}", base_url, invoice_id)))
    .with_metadata(serde_json::json!({
        "invoice_id": invoice_id,
        "vendor_name": vendor_name,
        "amount": amount,
    }))
    .with_priority(NotificationPriority::High);

    notification_router.route(&notification, &preferences).await?;

    Ok(request)
}
```

### 2. Approval Actions

When a Slack/Teams action button is clicked:

1. **Slack**: Webhook receives interaction payload
2. **Teams**: Power Automate or custom endpoint receives action
3. **Backend**: Validates and executes action

```rust
// New endpoint: POST /api/v1/notifications/actions
async fn handle_notification_action(
    payload: NotificationActionPayload,
) -> Result<()> {
    match payload.action_type {
        ActionType::Approve => {
            workflow_service.approve_invoice(...).await?;
        }
        ActionType::Reject => {
            workflow_service.reject_invoice(...).await?;
        }
        ActionType::View => {
            // No action - already opened URL
        }
        _ => {}
    }

    // Update original message to show action taken
    update_slack_message(payload.message_ts, "Approved ✓").await?;

    Ok(())
}
```

## Success Metrics

### Target Goals (Sprint 13)
- **40% reduction in average approval time** (from 48 hours to 29 hours)
- **60% of users enable Slack/Teams notifications** within 30 days
- **90% delivery success rate** across all channels
- **25% action rate** (users clicking action buttons)

### Measurement
- Track delivery timestamps via `notification_deliveries` table
- Monitor approval times before/after notification delivery
- A/B test Slack vs Teams vs Email for speed
- Survey user satisfaction with notification quality

## Configuration

### Slack App Setup

1. **Create Slack App** at https://api.slack.com/apps
2. **Configure OAuth scopes:**
   - `chat:write`: Send messages
   - `users:read`: Get user info
   - `im:write`: Open DM channels

3. **Set environment variables:**
   ```bash
   SLACK_CLIENT_ID=x...
   SLACK_CLIENT_SECRET=x...
   SLACK_REDIRECT_URI=https://api.billforge.com/api/v1/notifications/slack/callback
   SLACK_SIGNING_SECRET=x...
   ```

### Teams Webhook Setup

1. **Navigate to Teams channel**
2. **Click "..." → "Connectors"**
3. **Add "Incoming Webhook"**
4. **Copy webhook URL**

### Default Preferences

```rust
// New users get these defaults:
NotificationPreference {
    channels: vec![
        ChannelPreference {
            channel: NotificationChannel::Email,
            enabled: true,
            notification_types: vec![], // All types
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
}
```

## Future Enhancements

### Sprint 14+
1. **Rich Notifications**
   - Invoice thumbnail images
   - Line item previews
   - Approval history timeline

2. **Smart Notification Batching**
   - Group similar notifications
   - Daily digest option
   - "Snooze" to remind later

3. **Mobile Push Notifications**
   - iOS APNS integration
   - Android FCM integration
   - Deep links to invoice details

4. **Interactive Message Updates**
   - Real-time status updates in Slack
   - "Approved by John 5 minutes ago"
   - Animated progress indicators

## Testing

### Unit Tests
All core modules include comprehensive unit tests:
- `lib.rs`: 1 test (notification builder)
- `slack.rs`: 2 tests (message builder, OAuth URL generation)
- `teams.rs`: 3 tests (webhook payload, adaptive card, URL validation)
- `notification_router.rs`: 3 tests (preferences, channel selection, routing)

### Integration Tests
Run with: `cargo test --lib`

### Manual Testing

```bash
# Install Slack for tenant
curl -X POST http://localhost:8080/api/v1/notifications/slack/install \
  -H "Authorization: Bearer $TOKEN"

# Configure Teams webhook
curl -X POST http://localhost:8080/api/v1/notifications/teams/configure \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"webhook_url":"https://outlook.office.com/webhook/..."}'

# Update preferences
curl -X PUT http://localhost:8080/api/v1/notifications/preferences \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"channel":"slack","enabled":true}'
```

## Migration

### Database Migration
```bash
# Run migration
sqlx migrate run

# Verify tables created
psql -d billforge -c "\dt slack_*"
psql -d billforge -c "\dt teams_*"
psql -d billforge -c "\dt user_notification_*"
```

### Backfill Data
```sql
-- Initialize preferences for existing users
INSERT INTO user_notification_preferences (id, tenant_id, user_id, channel, enabled)
SELECT
    uuid_generate_v4(),
    tenant_id,
    id,
    'email',
    true
FROM users
WHERE role = 'approver';

INSERT INTO user_notification_preferences (id, tenant_id, user_id, channel, enabled)
SELECT
    uuid_generate_v4(),
    tenant_id,
    id,
    'in_app',
    true
FROM users
WHERE role = 'approver';
```

## Monitoring

### Key Metrics
- `notification_deliveries_total{channel, success}`: Delivery count by channel
- `notification_delivery_time_ms`: Histogram of delivery latency
- `notification_action_rate{action_type}`: Button click rates
- `approval_time_hours{notification_channel}`: Approval speed by channel

### Alerts
- Delivery success rate < 85%
- Average delivery latency > 2 seconds
- Slack/Teams API errors > 5% of requests
- OAuth token expiry warnings

## Dependencies

### New Dependencies
- `reqwest = "0.11"`: HTTP client for Slack/Teams APIs
- `urlencoding = "2.1"`: URL encoding for OAuth

### Existing Dependencies
- `serde`, `serde_json`: Serialization
- `uuid`: ID generation
- `chrono`: Timestamps
- `sqlx`: Database operations
- `tracing`: Logging
- `async-trait`: Async trait support

## Files Changed

### New Files
- `crates/notifications/Cargo.toml` (20 lines)
- `crates/notifications/src/lib.rs` (210 lines)
- `crates/notifications/src/slack.rs` (450 lines)
- `crates/notifications/src/teams.rs` (460 lines)
- `crates/notifications/src/notification_router.rs` (330 lines)
- `crates/api/src/routes/notifications.rs` (340 lines)
- `migrations/052_add_notifications.sql` (180 lines)

### Modified Files
- `backend/Cargo.toml`: Added notifications crate to workspace
- `crates/api/Cargo.toml`: Added reqwest and urlencoding dependencies
- `crates/api/src/routes/mod.rs`: Added notifications routes

## References

- [Sprint 13 Feature Plan](../sprint_13_feature_plan.md#p1-5-slackteams-notification-integration-2-days)
- [Slack API Documentation](https://api.slack.com/)
- [Microsoft Teams Webhooks](https://docs.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/what-are-webhooks-and-connectors)
- [Adaptive Cards Documentation](https://adaptivecards.io/)
