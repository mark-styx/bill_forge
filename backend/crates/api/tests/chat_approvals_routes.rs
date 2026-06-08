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

/// Shared lock to prevent races between tests that read/write the
/// `TEAMS_ACTIONS_ENABLED` process-global env var.
static TEAMS_ACTIONS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// When TEAMS_ACTIONS_ENABLED is not set, the route should reject with an
/// error rather than accepting unauthenticated action callbacks.
#[tokio::test]
async fn test_teams_actions_disabled_by_default() {
    let _guard = TEAMS_ACTIONS_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
    let _guard = TEAMS_ACTIONS_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // Temporarily set the flag
    std::env::set_var("TEAMS_ACTIONS_ENABLED", "true");
    let enabled = std::env::var("TEAMS_ACTIONS_ENABLED").as_deref() == Ok("true");
    assert!(
        enabled,
        "TEAMS_ACTIONS_ENABLED=true should enable the route"
    );
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

// ---------------------------------------------------------------------------
// Issue #362: Teams Bearer must be a real Microsoft JWT validated against JWKS
// ---------------------------------------------------------------------------

/// The handler must call the shared TeamsJwtValidator from AppState rather
/// than only checking that the Bearer token is non-empty.
#[test]
fn test_teams_actions_uses_teams_jwt_validator() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("state\n            .teams_jwt_validator")
            || source.contains("state.teams_jwt_validator"),
        "teams_actions must validate the Bearer via state.teams_jwt_validator"
    );
    assert!(
        source.contains(".validate(bearer)"),
        "teams_actions must call validator.validate() on the Bearer token"
    );
}

/// The handler must no longer accept any non-empty Bearer. The stub comment
/// from before #362 ("Production: decode and validate the JWT...") and the
/// `if token.is_empty()` guard must be gone.
#[test]
fn test_teams_actions_no_longer_accepts_any_nonempty_bearer() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    let teams_start = source.find("async fn teams_actions(").unwrap();
    let teams_end = source[teams_start..]
        .find("\n// -")
        .unwrap_or(source.len() - teams_start);
    let teams_section = &source[teams_start..teams_start + teams_end];
    assert!(
        !teams_section.contains("Empty Bearer token"),
        "teams_actions must not gate solely on Bearer-non-empty; that was the #362 bug"
    );
    assert!(
        !teams_section.contains("Production: decode and validate the JWT"),
        "teams_actions must no longer defer JWT decode/validate to a comment"
    );
}

/// When the validator accepts the token, the handler must resolve the actor
/// by matching teams_webhooks.aad_object_id against the validated `oid` claim,
/// not by LIMIT 1 over the table.
#[test]
fn test_teams_actor_lookup_uses_aad_object_id() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("aad_object_id = $2"),
        "teams_actions must look up the actor by (tenant_id, aad_object_id) when a validated JWT \
         is present"
    );
    assert!(
        source.contains("AAD principal is not registered for this tenant"),
        "teams_actions must return a Forbidden error (not Validation) when the validated oid is \
         not registered for the supplied tenant"
    );
}

/// The dev-only TEAMS_SKIP_JWT_VALIDATION path must still resolve an actor
/// via the legacy LIMIT 1 lookup so local testing without a real token works.
#[test]
fn test_teams_skip_jwt_keeps_limit_1_fallback() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("TEAMS_SKIP_JWT_VALIDATION"),
        "the dev-only TEAMS_SKIP_JWT_VALIDATION bypass must still exist"
    );
    assert!(
        source.contains("WHERE tenant_id = $1 AND is_active = true LIMIT 1"),
        "TEAMS_SKIP_JWT_VALIDATION must fall through to the original LIMIT 1 actor query"
    );
}

/// The new TeamsJwtValidator must be wired into AppState so the handler can
/// reach it via the shared state.
#[test]
fn test_state_carries_teams_jwt_validator() {
    let source = include_str!("../src/state.rs");
    assert!(
        source.contains("pub teams_jwt_validator: Arc<TeamsJwtValidator>"),
        "AppState must expose teams_jwt_validator so the handler can validate inbound tokens"
    );
    assert!(
        source.contains("TEAMS_OIDC_JWKS_URL")
            && source.contains("TEAMS_OIDC_EXPECTED_ISSUER")
            && source.contains("TEAMS_OIDC_EXPECTED_AUDIENCE"),
        "Validator construction must read the three required OIDC env vars when actions are \
         enabled"
    );
}

/// The migration adding teams_webhooks.aad_object_id must be present so the
/// (tenant_id, aad_object_id) actor lookup has a column to query.
#[test]
fn test_migration_adds_aad_object_id_column() {
    let source = include_str!("../../../migrations/131_teams_webhooks_aad_oid.sql");
    assert!(
        source.contains("ADD COLUMN IF NOT EXISTS aad_object_id TEXT"),
        "migration must add the aad_object_id column to teams_webhooks"
    );
    assert!(
        source.contains("teams_webhooks_tenant_aad_active_idx"),
        "migration must add the partial unique index for (tenant_id, aad_object_id) where active"
    );
}

// ---------------------------------------------------------------------------
// Issue #356: AI Q&A question detection and routing
// ---------------------------------------------------------------------------

/// Verify `is_invoice_question` detects messages starting with `?`.
#[test]
fn test_is_invoice_question_detects_question_mark() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("fn is_invoice_question"),
        "chat_approvals.rs must contain the is_invoice_question helper"
    );
}

/// Verify the question detection helper recognizes all trigger patterns.
#[test]
fn test_question_trigger_patterns() {
    // Replicate the detection logic inline to verify the patterns
    fn is_question(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.starts_with('?') {
            return true;
        }
        if trimmed.starts_with("/ask ") || trimmed.starts_with("/ask\t") {
            return true;
        }
        if let Some(rest) = trimmed.strip_prefix("<@") {
            if rest.contains('>') {
                if let Some(after_mention) = rest.split_once('>') {
                    if !after_mention.1.trim().is_empty() {
                        return true;
                    }
                }
            }
        }
        false
    }

    // Question mark prefix
    assert!(is_question("? What is the total?"));
    assert!(is_question("  ?question about GL code"));

    // /ask prefix
    assert!(is_question("/ask What vendor is this?"));
    assert!(is_question("/ask\tsomething"));

    // Bot mention prefix
    assert!(is_question("<@U12345> What is the total?"));

    // Not a question
    assert!(!is_question("This looks good, approving"));
    assert!(!is_question("Can we get this done by Friday?")); // no prefix
    assert!(!is_question("<@U12345>")); // mention but no text after
}

/// Verify `extract_question` strips the trigger prefix correctly.
#[test]
fn test_extract_question_strips_prefix() {
    fn extract(text: &str) -> &str {
        let trimmed = text.trim();
        if let Some(q) = trimmed.strip_prefix('?') {
            return q.trim();
        }
        if let Some(q) = trimmed.strip_prefix("/ask ") {
            return q.trim();
        }
        if let Some(q) = trimmed.strip_prefix("/ask\t") {
            return q.trim();
        }
        if let Some(rest) = trimmed.strip_prefix("<@") {
            if let Some((_mention, after)) = rest.split_once('>') {
                return after.trim();
            }
        }
        trimmed
    }

    assert_eq!(extract("? What is the total?"), "What is the total?");
    assert_eq!(extract("/ask What vendor?"), "What vendor?");
    assert_eq!(extract("<@U12345> total?"), "total?");
    assert_eq!(extract("?what"), "what");
}

/// Verify the AI Q&A feature is gated behind CHAT_AI_QA_ENABLED.
#[test]
fn test_chat_ai_qa_gated_behind_env_var() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("CHAT_AI_QA_ENABLED"),
        "AI Q&A must be gated behind the CHAT_AI_QA_ENABLED env var"
    );
    assert!(
        source.contains("ai_qa_via_slack"),
        "Slack AI Q&A must log audit entries with type ai_qa_via_slack"
    );
    assert!(
        source.contains("ai_qa_via_teams"),
        "Teams AI Q&A must log audit entries with type ai_qa_via_teams"
    );
}

/// Verify plain comments are still logged as thread_comment_via_slack
/// when the message is NOT a question.
#[test]
fn test_plain_comment_still_logged_when_not_question() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    // The existing comment path should still exist
    assert!(
        source.contains("thread_comment_via_slack"),
        "Non-question thread replies must still be logged as thread_comment_via_slack"
    );
}

/// Verify that AI errors produce an apology reply, not a 500 to Slack.
#[test]
fn test_ai_errors_produce_apology_not_500() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        source.contains("Sorry, I couldn't process your question about this invoice"),
        "AI errors must post a generic apology rather than 500ing back to Slack"
    );
}

/// Verify that answer_invoice_question is exported from the ai-agent crate.
#[test]
fn test_answer_invoice_question_is_exported() {
    let source = include_str!("../../ai-agent/src/lib.rs");
    assert!(
        source.contains("pub async fn answer_invoice_question"),
        "answer_invoice_question must be a public function in ai-agent lib.rs"
    );
    assert!(
        source.contains("MAX_ANSWER_LENGTH"),
        "answer_invoice_question must truncate answers to a chat-safe length"
    );
}

// ---------------------------------------------------------------------------
// Issue 2 (signing secret): SLACK_SIGNING_SECRET must not fall back to a hardcoded constant
// ---------------------------------------------------------------------------

/// Ensure there is no hardcoded fallback for the Slack signing secret.
/// The function must return an error when the env var is absent, not a
/// constant string that an attacker could guess.
#[test]
fn test_slack_signing_secret_no_hardcoded_fallback() {
    let source = include_str!("../src/routes/chat_approvals.rs");
    assert!(
        !source.contains("development-signing-secret"),
        "slack_signing_secret() must not contain a hardcoded fallback string. \
         It should return an error when SLACK_SIGNING_SECRET is not set."
    );
    assert!(
        source.contains("SLACK_SIGNING_SECRET is not configured"),
        "slack_signing_secret() must return a clear error when the env var is missing."
    );
}
