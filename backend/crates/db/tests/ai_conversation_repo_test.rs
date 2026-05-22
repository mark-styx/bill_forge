//! Integration tests for AI conversation and message persistence
//!
//! Verifies that `AiConversationRepositoryImpl::create_conversation` and
//! `append_message` correctly insert tenant/user-scoped rows, update
//! timestamps, persist usage telemetry, and enforce ownership scoping.
//!
//! Gated behind `#[cfg_attr(not(feature = "integration"), ignore)]` so
//! `cargo test` skips them by default; run with `--features integration`.

use billforge_core::{TenantId, UserId};
use billforge_db::repositories::{
    AiConversationRepositoryImpl, AiMessageRole, AiMessageUsage, AppendAiMessageInput,
    PersistAiToolCallInput, PersistAiToolResultInput,
};
use billforge_db::PgManager;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Insert a minimal user row for the given tenant.
async fn seed_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("user-{}@test.com", user_id))
    .bind("hash")
    .bind("Test User")
    .execute(pool)
    .await
    .expect("seed user");
}

// ---------------------------------------------------------------------------
// Shared setup helper
// ---------------------------------------------------------------------------

/// Create a fresh tenant database with migrations applied and return
/// `(manager, tenant_id, pool)`. The tag ensures test isolation.
async fn setup_single_tenant(
    tag: &str,
) -> (PgManager, TenantId, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".to_string());
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    let tenant_id: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("tag-{}", tag).as_bytes())
        .to_string()
        .parse()
        .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, &format!("AI Test Tenant {}", tag))
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();
    manager.run_tenant_migrations(&pool).await.expect("migrate");

    (manager, tenant_id, pool)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `create_conversation` inserts a row scoped to tenant/user and returns
/// generated timestamps and metadata.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn create_conversation_inserts_scoped_row() {
    let (manager, tenant_id, pool) = setup_single_tenant("create-conv").await;
    let user_id = Uuid::new_v4();
    seed_user(&pool, &tenant_id, user_id).await;

    let repo = AiConversationRepositoryImpl::new(Arc::new(pool));
    let uid = UserId(user_id);

    let record = repo
        .create_conversation(
            &tenant_id,
            &uid,
            Some("Test Title"),
            serde_json::json!({"source": "test"}),
        )
        .await
        .expect("create_conversation should succeed");

    assert_eq!(record.tenant_id, *tenant_id.as_uuid());
    assert_eq!(record.user_id, user_id);
    assert_eq!(record.title.as_deref(), Some("Test Title"));
    assert_eq!(record.metadata["source"], "test");
    assert!(record.created_at <= chrono::Utc::now());
    assert!(record.updated_at <= chrono::Utc::now());

    manager.delete_tenant(&tenant_id).await.ok();
}

/// `append_message` inserts ordered user and assistant rows, updates the
/// conversation's `updated_at`, and persists usage telemetry.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn append_message_inserts_rows_and_updates_timestamp() {
    let (manager, tenant_id, pool) = setup_single_tenant("append-msg").await;
    let user_id = Uuid::new_v4();
    seed_user(&pool, &tenant_id, user_id).await;

    let pool_ref = Arc::new(pool.clone());
    let repo = AiConversationRepositoryImpl::new(Arc::new(pool));
    let uid = UserId(user_id);

    let conv = repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let original_updated_at = conv.updated_at;

    // Small delay so updated_at is measurably different
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Append user message
    let user_msg = repo
        .append_message(
            &tenant_id,
            &uid,
            conv.id,
            AppendAiMessageInput {
                role: AiMessageRole::User,
                content: "Hello Winston".to_string(),
                provider: None,
                model: None,
                model_route: None,
                finish_reason: None,
                provider_request_id: None,
                latency_ms: None,
                usage: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect("append user message");

    assert_eq!(user_msg.conversation_id, conv.id);
    assert_eq!(user_msg.role, "user");
    assert_eq!(user_msg.content, "Hello Winston");
    assert!(user_msg.provider.is_none());

    // Append assistant message with telemetry
    let assistant_msg = repo
        .append_message(
            &tenant_id,
            &uid,
            conv.id,
            AppendAiMessageInput {
                role: AiMessageRole::Assistant,
                content: "I can help with that.".to_string(),
                provider: Some("fake".to_string()),
                model: Some("test-model".to_string()),
                model_route: Some("Default".to_string()),
                finish_reason: Some("stop".to_string()),
                provider_request_id: Some("req-123".to_string()),
                latency_ms: Some(150),
                usage: Some(AiMessageUsage {
                    prompt_tokens: Some(10),
                    completion_tokens: Some(20),
                    total_tokens: Some(30),
                }),
                metadata: serde_json::json!({"routing_reason": "default_chat_turn"}),
            },
        )
        .await
        .expect("append assistant message");

    assert_eq!(assistant_msg.role, "assistant");
    assert_eq!(assistant_msg.content, "I can help with that.");
    assert_eq!(assistant_msg.provider.as_deref(), Some("fake"));
    assert_eq!(assistant_msg.model.as_deref(), Some("test-model"));
    assert_eq!(assistant_msg.finish_reason.as_deref(), Some("stop"));
    assert_eq!(assistant_msg.provider_request_id.as_deref(), Some("req-123"));
    assert_eq!(assistant_msg.latency_ms, Some(150));
    assert_eq!(assistant_msg.prompt_tokens, Some(10));
    assert_eq!(assistant_msg.completion_tokens, Some(20));
    assert_eq!(assistant_msg.total_tokens, Some(30));

    // Verify messages are ordered by creation time
    assert!(user_msg.created_at <= assistant_msg.created_at);

    // Verify conversation updated_at was bumped
    let refreshed: (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
        "SELECT updated_at FROM ai_conversations WHERE id = $1",
    )
    .bind(conv.id)
    .fetch_one(pool_ref.as_ref())
    .await
    .expect("fetch conversation");

    assert!(refreshed.0 > original_updated_at);

    manager.delete_tenant(&tenant_id).await.ok();
}

/// `append_message` returns `Error::NotFound` when appending with the
/// wrong user or a missing conversation.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn append_message_rejects_wrong_user_or_missing_conversation() {
    let (manager, tenant_id, pool) = setup_single_tenant("append-reject").await;
    let user_id = Uuid::new_v4();
    let wrong_user_id = Uuid::new_v4();
    seed_user(&pool, &tenant_id, user_id).await;
    seed_user(&pool, &tenant_id, wrong_user_id).await;

    let repo = AiConversationRepositoryImpl::new(Arc::new(pool));
    let uid = UserId(user_id);
    let wrong_uid = UserId(wrong_user_id);

    let conv = repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    // Wrong user
    let err = repo
        .append_message(
            &tenant_id,
            &wrong_uid,
            conv.id,
            AppendAiMessageInput {
                role: AiMessageRole::User,
                content: "should fail".to_string(),
                provider: None,
                model: None,
                model_route: None,
                finish_reason: None,
                provider_request_id: None,
                latency_ms: None,
                usage: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect_err("should reject wrong user");

    match err {
        billforge_core::Error::NotFound { resource_type, id } => {
            assert_eq!(resource_type, "ai_conversation");
            assert_eq!(id, conv.id.to_string());
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    // Non-existent conversation
    let missing_id = Uuid::new_v4();
    let err = repo
        .append_message(
            &tenant_id,
            &uid,
            missing_id,
            AppendAiMessageInput {
                role: AiMessageRole::User,
                content: "should also fail".to_string(),
                provider: None,
                model: None,
                model_route: None,
                finish_reason: None,
                provider_request_id: None,
                latency_ms: None,
                usage: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect_err("should reject missing conversation");

    match err {
        billforge_core::Error::NotFound { resource_type, id } => {
            assert_eq!(resource_type, "ai_conversation");
            assert_eq!(id, missing_id.to_string());
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    manager.delete_tenant(&tenant_id).await.ok();
}

/// Persisting a tool call and tool result against an assistant message works
/// end-to-end, and cross-scope attachment is rejected.
#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn persist_tool_call_and_result_round_trip() {
    let (manager, tenant_id, pool) = setup_single_tenant("tool-call-persist").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let repo = AiConversationRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    // Create conversation + assistant message
    let conv = repo
        .create_conversation(&tenant_id, &uid, Some("Tool test"), serde_json::json!({}))
        .await
        .expect("create conversation");

    let assistant_msg = repo
        .append_message(
            &tenant_id,
            &uid,
            conv.id,
            AppendAiMessageInput {
                role: AiMessageRole::Assistant,
                content: "I will look that up.".to_string(),
                provider: Some("fake".to_string()),
                model: Some("test-model".to_string()),
                model_route: None,
                finish_reason: Some("tool_calls".to_string()),
                provider_request_id: None,
                latency_ms: None,
                usage: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect("append assistant message");

    // Persist a tool call
    let tool_call = repo
        .persist_tool_call(
            &tenant_id,
            &uid,
            conv.id,
            assistant_msg.id,
            PersistAiToolCallInput {
                provider_tool_call_id: Some("call_abc123".to_string()),
                tool_name: "get_invoice".to_string(),
                arguments: serde_json::json!({"invoice_id": "inv-001"}),
                status: Some("requested".to_string()),
                metadata: serde_json::json!({"source": "assistant_response"}),
            },
        )
        .await
        .expect("persist tool call");

    assert_eq!(tool_call.tenant_id, *tenant_id.as_uuid());
    assert_eq!(tool_call.user_id, user_id);
    assert_eq!(tool_call.conversation_id, conv.id);
    assert_eq!(tool_call.message_id, assistant_msg.id);
    assert_eq!(tool_call.provider_tool_call_id.as_deref(), Some("call_abc123"));
    assert_eq!(tool_call.tool_name, "get_invoice");
    assert_eq!(tool_call.arguments["invoice_id"], "inv-001");
    assert_eq!(tool_call.status, "requested");
    assert!(tool_call.created_at <= chrono::Utc::now());
    assert!(tool_call.updated_at <= chrono::Utc::now());

    // Persist a tool result for that call
    let tool_result = repo
        .persist_tool_result(
            &tenant_id,
            &uid,
            conv.id,
            assistant_msg.id,
            tool_call.id,
            PersistAiToolResultInput {
                success: true,
                result: Some(serde_json::json!({"amount": 10000, "currency": "USD"})),
                error: None,
                latency_ms: Some(42),
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect("persist tool result");

    assert_eq!(tool_result.tenant_id, *tenant_id.as_uuid());
    assert_eq!(tool_result.user_id, user_id);
    assert_eq!(tool_result.conversation_id, conv.id);
    assert_eq!(tool_result.message_id, assistant_msg.id);
    assert_eq!(tool_result.tool_call_id, tool_call.id);
    assert!(tool_result.success);
    assert_eq!(tool_result.result.as_ref().unwrap()["amount"], 10000);
    assert!(tool_result.error.is_none());
    assert_eq!(tool_result.latency_ms, Some(42));
    assert!(tool_result.created_at <= chrono::Utc::now());

    // List tool calls for message and verify
    let calls = repo
        .list_tool_calls_for_message(&tenant_id, &uid, conv.id, assistant_msg.id)
        .await
        .expect("list tool calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, tool_call.id);

    // --- Negative: wrong user cannot persist tool call ---
    let wrong_user_id = Uuid::new_v4();
    seed_user(pool.as_ref(), &tenant_id, wrong_user_id).await;
    let wrong_uid = UserId(wrong_user_id);

    let err = repo
        .persist_tool_call(
            &tenant_id,
            &wrong_uid,
            conv.id,
            assistant_msg.id,
            PersistAiToolCallInput {
                provider_tool_call_id: None,
                tool_name: "bad_call".to_string(),
                arguments: serde_json::json!({}),
                status: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect_err("wrong user should fail");

    match err {
        billforge_core::Error::Database(msg) => {
            assert!(
                msg.contains("Failed to persist tool call"),
                "unexpected error: {}",
                msg
            );
        }
        other => panic!("expected Database error, got {:?}", other),
    }

    // --- Negative: missing message ---
    let missing_msg_id = Uuid::new_v4();
    let err = repo
        .persist_tool_call(
            &tenant_id,
            &uid,
            conv.id,
            missing_msg_id,
            PersistAiToolCallInput {
                provider_tool_call_id: None,
                tool_name: "orphan".to_string(),
                arguments: serde_json::json!({}),
                status: None,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .expect_err("missing message should fail");

    match err {
        billforge_core::Error::Database(msg) => {
            assert!(
                msg.contains("Failed to persist tool call"),
                "unexpected error: {}",
                msg
            );
        }
        other => panic!("expected Database error, got {:?}", other),
    }

    manager.delete_tenant(&tenant_id).await.ok();
}
