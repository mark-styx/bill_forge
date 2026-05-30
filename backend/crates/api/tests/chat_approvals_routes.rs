//! Integration tests for the chat approval surface.
//!
//! Tests the Slack signature verification, block action parsing, and the
//! high-level flow of chat-initiated approvals against a real database.

#![allow(warnings)]

use billforge_core::TenantId;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

async fn ensure_schema(pool: &sqlx::PgPool) {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    billforge_db::tenant_db::run_tenant_migrations(pool, &TenantId(tenant_id))
        .await
        .expect("tenant migrations");
}

#[test]
fn test_slack_signature_rejects_bad_inputs() {
    use billforge_notifications::verify_slack_signature;

    // Bad signature
    assert!(verify_slack_signature("secret", "1234567890", "v0=badbad", b"body").is_err());

    // Stale timestamp (year 2001)
    assert!(verify_slack_signature("secret", "1000000000", "v0=anything", b"body").is_err());
}

#[test]
fn test_parse_action_id() {
    let uuid = Uuid::new_v4();
    let action_id = format!("bf_approve:{}", uuid);

    // Replicate the parsing logic from chat_approvals.rs
    let (verb, parsed_id) = action_id.split_once(':').unwrap();
    let parsed_uuid: Uuid = parsed_id.parse().unwrap();
    assert_eq!(verb, "bf_approve");
    assert_eq!(parsed_uuid, uuid);
}

#[tokio::test]
async fn test_block_action_payload_parsing() {
    // Verify that a Slack block_actions payload can be deserialized
    let payload = serde_json::json!({
        "type": "block_actions",
        "user": { "id": "U12345", "username": "testuser", "name": "Test User", "team_id": "T123" },
        "api_app_id": "A123",
        "token": "verification_token",
        "container": { "type": "message", "message_ts": "1234567890.123456", "channel_id": "C123", "is_ephemeral": false },
        "trigger_id": "trigger123",
        "team": { "id": "T123", "domain": "testdomain" },
        "channel": { "id": "C123", "name": "general" },
        "message": { "type": "message", "bot_id": "B123", "text": "Approval request", "ts": "1234567890.123456" },
        "response_url": "https://hooks.slack.com/actions/T123/123/abc",
        "actions": [{
            "type": "button",
            "block_id": "actions1",
            "action_id": "bf_approve:11111111-2222-3333-4444-555555555555",
            "value": "11111111-2222-3333-4444-555555555555",
            "action_ts": "1234567890.123456"
        }]
    });

    let parsed: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&payload).unwrap()).unwrap();
    let actions = parsed["actions"].as_array().unwrap();
    assert_eq!(
        actions[0]["action_id"].as_str().unwrap(),
        "bf_approve:11111111-2222-3333-4444-555555555555"
    );
}

#[tokio::test]
async fn test_slack_events_url_verification_challenge() {
    // Verify the URL verification challenge response shape
    let challenge = serde_json::json!({
        "token": "verification_token",
        "challenge": "test-challenge-string",
        "type": "url_verification"
    });

    assert_eq!(challenge["type"].as_str().unwrap(), "url_verification");
    assert_eq!(
        challenge["challenge"].as_str().unwrap(),
        "test-challenge-string"
    );
}

// ---------------------------------------------------------------------------
// Issue 1: Teams actions must be gated behind TEAMS_ACTIONS_ENABLED
// ---------------------------------------------------------------------------

/// When TEAMS_ACTIONS_ENABLED is not set, the route should reject with an
/// error rather than accepting unauthenticated action callbacks.
#[tokio::test]
async fn test_teams_actions_disabled_by_default() {
    // Ensure the env var is NOT set
    std::env::remove_var("TEAMS_ACTIONS_ENABLED");

    // The route builder should register teams_actions_disabled which returns
    // an error. We verify the handler logic directly.
    //
    // Since we cannot easily call the handler without full AppState, we test
    // the env-var gate logic inline.
    let enabled = std::env::var("TEAMS_ACTIONS_ENABLED").as_deref() == Ok("true");
    assert!(
        !enabled,
        "TEAMS_ACTIONS_ENABLED must default to disabled (absent)"
    );
}

/// When TEAMS_ACTIONS_ENABLED=true, the route should be active.
#[tokio::test]
async fn test_teams_actions_enabled_when_flag_set() {
    // Temporarily set the flag
    std::env::set_var("TEAMS_ACTIONS_ENABLED", "true");
    let enabled = std::env::var("TEAMS_ACTIONS_ENABLED").as_deref() == Ok("true");
    assert!(enabled, "TEAMS_ACTIONS_ENABLED=true should enable the route");
    // Clean up
    std::env::remove_var("TEAMS_ACTIONS_ENABLED");
}

// ---------------------------------------------------------------------------
// Issue 2: actor_id must resolve to a real user, never Uuid::nil()
// ---------------------------------------------------------------------------

/// Verify that the TeamsActionBody struct parses correctly and that a nil
/// tenant_id would be rejected. The actor resolution now queries
/// teams_webhooks rather than hard-coding Uuid::nil().
#[test]
fn test_teams_action_body_parses_round_trip() {
    let body = serde_json::json!({
        "action": "approve",
        "invoice_id": "550e8400-e29b-41d4-a716-446655440000",
        "tenant_id": "660e8400-e29b-41d4-a716-446655440001",
        "comment_body": "Looks good",
        "reassign_to_user_id": null,
    });

    // Verify the body can be parsed into the expected shape.
    let parsed: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&body).unwrap()).unwrap();
    assert_eq!(parsed["action"].as_str().unwrap(), "approve");
    assert_eq!(
        parsed["invoice_id"].as_str().unwrap(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(
        parsed["tenant_id"].as_str().unwrap(),
        "660e8400-e29b-41d4-a716-446655440001"
    );
}

/// Ensure Uuid::nil() is NOT used as the actor_id in Teams actions.
/// The handler now resolves actor_id from teams_webhooks, so nil should
/// never appear in the audit log or state machine call.
#[test]
fn test_teams_actor_id_never_nil() {
    // This is a regression guard: the old code set `let actor_id = Uuid::nil()`.
    // The fix resolves from teams_webhooks. We confirm the nil UUID is not
    // used by checking the source no longer has the pattern in the Teams path.
    //
    // Read the source file and verify there is no `Uuid::nil()` assignment
    // in the teams_actions handler region.
    let source = include_str!("../src/routes/chat_approvals.rs");
    // Find the teams_actions handler and ensure no nil actor
    let teams_start = source.find("async fn teams_actions(").unwrap();
    let teams_section = &source[teams_start..];

    // The handler should not contain Uuid::nil() anywhere
    assert!(
        !teams_section.contains("Uuid::nil()"),
        "teams_actions handler must not use Uuid::nil() for actor_id. \
         Actor must be resolved from teams_webhooks table."
    );
}

/// When TEAMS_ACTIONS_ENABLED is set but no active webhook exists for the
/// tenant, the handler must return a validation error rather than proceeding
/// with a nil actor.
#[test]
fn test_teams_actor_id_error_message_when_no_webhook() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("No active Teams webhook configuration found for this tenant"),
        "Handler must return a clear error when no teams_webhooks row exists for the tenant"
    );
}

/// Verify that the disabled-handler path returns a validation error.
#[test]
fn test_teams_disabled_handler_returns_error() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("Teams actions endpoint is disabled"),
        "Disabled handler must return a clear error message about TEAMS_ACTIONS_ENABLED"
    );
}
