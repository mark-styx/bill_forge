//! Integration tests for Winston chat persistence.
//!
//! Validates that WinstonAgent::chat durably persists conversation and message
//! rows to the database, including provider telemetry for assistant messages.
//!
//! Uses FakeAiProvider and a migrated PostgreSQL test database.

use std::sync::Arc;

use billforge_ai_agent::agent::WinstonAgent;
use billforge_ai_agent::models::ChatRequest;
use billforge_ai_agent::FakeAiProvider;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create the minimal schema needed for chat persistence tests:
/// tenants (FK target), users, ai_conversations/ai_messages, and ai_usage_events.
async fn setup_minimal_schema(pool: &sqlx::PgPool) {
    // Tenants table (FK target for ai_conversations.tenant_id)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tenants (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL,
            settings JSONB NOT NULL DEFAULT '{}',
            enabled_modules JSONB NOT NULL DEFAULT '[]',
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create tenants table");

    // Users table (FK target for ai_conversations/ai_messages.user_id)
    let migration_002 = include_str!("../../../migrations/002_create_users.sql");
    sqlx::raw_sql(migration_002)
        .execute(pool)
        .await
        .expect("create users table");

    // AI conversations and messages
    let migration_082 = include_str!("../../../migrations/082_create_ai_conversations.sql");
    sqlx::raw_sql(migration_082)
        .execute(pool)
        .await
        .expect("create ai_conversations/ai_messages tables");

    // AI tool call persistence (adds unique constraint on ai_messages needed by 084)
    let migration_083 = include_str!("../../../migrations/083_create_ai_tool_call_persistence.sql");
    sqlx::raw_sql(migration_083)
        .execute(pool)
        .await
        .expect("create ai_tool_calls/ai_tool_results tables");

    // AI usage events
    let migration_084 = include_str!("../../../migrations/084_create_ai_usage_events.sql");
    sqlx::raw_sql(migration_084)
        .execute(pool)
        .await
        .expect("create ai_usage_events table");
}

/// Insert a tenant row.
async fn insert_tenant(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO tenants (id, name, slug)
           VALUES ($1, 'Test Tenant', 'test-tenant')
           ON CONFLICT DO NOTHING"#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert test tenant");
}

/// Insert a user row so inject_context and FK constraints are satisfied.
async fn insert_user(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, 'test@example.com', 'hash', 'Test User', '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Count conversations for the given tenant/user.
async fn count_conversations(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM ai_conversations WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("count conversations");
    row.0
}

/// Read message rows (role, content, provider, model, model_route,
/// prompt_tokens, completion_tokens, total_tokens, finish_reason,
/// provider_request_id, latency_ms) ordered by created_at.
async fn read_messages(
    pool: &sqlx::PgPool,
    conversation_id: Uuid,
) -> Vec<(
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<String>,
    Option<String>,
    Option<i64>,
)> {
    sqlx::query_as(
        r#"SELECT role, content, provider, model, model_route,
                  prompt_tokens, completion_tokens, total_tokens,
                  finish_reason, provider_request_id, latency_ms
           FROM ai_messages
           WHERE conversation_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .expect("read messages")
}

/// Read the title of a conversation.
async fn read_conversation_title(pool: &sqlx::PgPool, conversation_id: Uuid) -> Option<String> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT title FROM ai_conversations WHERE id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .expect("read conversation title");
    row.and_then(|t| t.0)
}

/// Read usage event rows for a given conversation, ordered by created_at.
async fn read_usage_events(
    pool: &sqlx::PgPool,
    conversation_id: Uuid,
) -> Vec<UsageEventRow> {
    sqlx::query_as::<_, UsageEventRow>(
        r#"SELECT id, tenant_id, user_id, conversation_id, message_id,
                  provider, model, model_route,
                  latency_ms, prompt_tokens, completion_tokens, total_tokens,
                  success, error_code, error_message,
                  provider_request_id, metadata, created_at
           FROM ai_usage_events
           WHERE conversation_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .expect("read usage events")
}

/// Helper row type for usage event queries.
#[derive(sqlx::FromRow)]
struct UsageEventRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation_id: Option<Uuid>,
    message_id: Option<Uuid>,
    provider: String,
    model: Option<String>,
    model_route: Option<String>,
    latency_ms: Option<i64>,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    total_tokens: Option<i32>,
    success: bool,
    error_code: Option<String>,
    error_message: Option<String>,
    provider_request_id: Option<String>,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Test 1: New chat persists conversation + 2 messages with telemetry
// ============================================================================

#[sqlx::test]
async fn test_new_chat_persists_conversation_and_messages(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant_uuid = Uuid::new_v4();
    let user_uuid = Uuid::new_v4();

    insert_tenant(&pool, tenant_uuid).await;
    insert_user(&pool, tenant_uuid, user_uuid).await;

    let provider = Arc::new(FakeAiProvider::new().with_response_text("test reply"));
    let agent = WinstonAgent::new(pool.clone(), provider);

    let request = ChatRequest {
        message: "Hello Winston".to_string(),
        conversation_id: None,
    };

    let response = agent
        .chat(request, tenant_uuid.to_string(), user_uuid)
        .await
        .expect("chat should succeed");

    // One conversation created
    assert_eq!(
        count_conversations(&pool, tenant_uuid, user_uuid).await,
        1,
        "expected exactly one conversation"
    );

    let conv_id = response.conversation_id;

    // Title is derived from user message, truncated to 80 bytes
    assert_eq!(
        read_conversation_title(&pool, conv_id).await,
        Some("Hello Winston".to_string())
    );

    // Exactly two messages: user + assistant
    let messages = read_messages(&pool, conv_id).await;
    assert_eq!(messages.len(), 2, "expected exactly 2 messages");

    // First message: user, no provider telemetry
    let msg = &messages[0];
    assert_eq!(msg.0, "user");
    assert_eq!(msg.1, "Hello Winston");
    assert!(msg.2.is_none(), "user msg should have no provider");
    assert!(msg.3.is_none(), "user msg should have no model");
    assert!(msg.4.is_none(), "user msg should have no model_route");
    assert!(msg.5.is_none(), "user msg should have no prompt_tokens");
    assert!(msg.8.is_none(), "user msg should have no finish_reason");
    assert!(msg.9.is_none(), "user msg should have no provider_request_id");
    assert!(msg.10.is_none(), "user msg should have no latency_ms");

    // Second message: assistant, with provider telemetry
    let msg = &messages[1];
    assert_eq!(msg.0, "assistant");
    assert_eq!(msg.1, "test reply");
    assert_eq!(msg.2.as_deref(), Some("fake"), "provider should be 'fake'");
    assert!(msg.3.is_some(), "assistant msg should have model");
    assert!(msg.4.is_some(), "assistant msg should have model_route");
    assert!(msg.5.is_some(), "assistant msg should have prompt_tokens");
    assert!(msg.6.is_some(), "assistant msg should have completion_tokens");
    assert!(msg.7.is_some(), "assistant msg should have total_tokens");
    assert_eq!(msg.8.as_deref(), Some("stop"), "finish_reason should be 'stop'");
    assert!(msg.9.is_some(), "assistant msg should have provider_request_id");
    assert!(msg.10.is_some(), "assistant msg should have latency_ms");
}

// ============================================================================
// Test 2: Continue existing conversation appends messages
// ============================================================================

#[sqlx::test]
async fn test_continue_conversation_appends_messages(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant_uuid = Uuid::new_v4();
    let user_uuid = Uuid::new_v4();

    insert_tenant(&pool, tenant_uuid).await;
    insert_user(&pool, tenant_uuid, user_uuid).await;

    let provider = Arc::new(FakeAiProvider::new().with_response_text("first reply"));
    let agent = WinstonAgent::new(pool.clone(), provider);

    // First chat: creates conversation
    let request1 = ChatRequest {
        message: "First message".to_string(),
        conversation_id: None,
    };
    let response1 = agent
        .chat(request1, tenant_uuid.to_string(), user_uuid)
        .await
        .expect("first chat should succeed");

    let conv_id = response1.conversation_id;

    // Second chat: continues same conversation
    let provider2 = Arc::new(FakeAiProvider::new().with_response_text("second reply"));
    let agent2 = WinstonAgent::new(pool.clone(), provider2);
    let request2 = ChatRequest {
        message: "Second message".to_string(),
        conversation_id: Some(conv_id),
    };
    let response2 = agent2
        .chat(request2, tenant_uuid.to_string(), user_uuid)
        .await
        .expect("second chat should succeed");

    // Same conversation reused
    assert_eq!(response2.conversation_id, conv_id);

    // Still only one conversation
    assert_eq!(
        count_conversations(&pool, tenant_uuid, user_uuid).await,
        1,
        "expected exactly one conversation after continuation"
    );

    // Four messages total: user+assistant for each turn
    let messages = read_messages(&pool, conv_id).await;
    assert_eq!(messages.len(), 4, "expected exactly 4 messages");

    // First turn
    assert_eq!(messages[0].0, "user");
    assert_eq!(messages[0].1, "First message");
    assert_eq!(messages[1].0, "assistant");
    assert_eq!(messages[1].1, "first reply");

    // Second turn
    assert_eq!(messages[2].0, "user");
    assert_eq!(messages[2].1, "Second message");
    assert_eq!(messages[3].0, "assistant");
    assert_eq!(messages[3].1, "second reply");
}

// ============================================================================
// Test 3: Non-existent conversation_id returns error
// ============================================================================

#[sqlx::test]
async fn test_nonexistent_conversation_returns_error(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant_uuid = Uuid::new_v4();
    let user_uuid = Uuid::new_v4();

    insert_tenant(&pool, tenant_uuid).await;
    insert_user(&pool, tenant_uuid, user_uuid).await;

    let provider = Arc::new(FakeAiProvider::new());
    let agent = WinstonAgent::new(pool.clone(), provider);

    let fake_conversation_id = Uuid::new_v4();
    let request = ChatRequest {
        message: "Hello".to_string(),
        conversation_id: Some(fake_conversation_id),
    };

    let result = agent
        .chat(request, tenant_uuid.to_string(), user_uuid)
        .await;

    assert!(
        result.is_err(),
        "expected error for non-existent conversation"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("not found") || err_msg.contains("access denied"),
        "error should mention not found or access denied: {}",
        err_msg
    );
}

// ============================================================================
// Test 4: Successful chat creates exactly one usage event
// ============================================================================

#[sqlx::test]
async fn test_successful_chat_creates_usage_event(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant_uuid = Uuid::new_v4();
    let user_uuid = Uuid::new_v4();

    insert_tenant(&pool, tenant_uuid).await;
    insert_user(&pool, tenant_uuid, user_uuid).await;

    let provider = Arc::new(FakeAiProvider::new().with_response_text("test reply"));
    let agent = WinstonAgent::new(pool.clone(), provider);

    let request = ChatRequest {
        message: "Hello Winston".to_string(),
        conversation_id: None,
    };

    let response = agent
        .chat(request, tenant_uuid.to_string(), user_uuid)
        .await
        .expect("chat should succeed");

    let conv_id = response.conversation_id;

    // Exactly one usage event for the conversation
    let events = read_usage_events(&pool, conv_id).await;
    assert_eq!(events.len(), 1, "expected exactly one usage event");

    let evt = &events[0];

    // Success fields
    assert!(evt.success, "usage event should be successful");
    assert!(evt.error_code.is_none(), "success event should have no error_code");
    assert!(evt.error_message.is_none(), "success event should have no error_message");

    // Provider/model/route
    assert_eq!(evt.provider, "fake");
    assert!(evt.model.is_some(), "should have model");
    assert!(evt.model_route.is_some(), "should have model_route");

    // Latency
    assert!(evt.latency_ms.is_some(), "should have latency_ms");
    assert!(evt.latency_ms.unwrap() >= 0, "latency should be non-negative");

    // Token counts from the fake provider
    assert!(evt.prompt_tokens.is_some(), "should have prompt_tokens");
    assert!(evt.completion_tokens.is_some(), "should have completion_tokens");
    assert!(evt.total_tokens.is_some(), "should have total_tokens");

    // Provider request ID
    assert!(evt.provider_request_id.is_some(), "should have provider_request_id");

    // Links
    assert_eq!(evt.conversation_id, Some(conv_id));
    assert!(evt.message_id.is_some(), "success event should link to assistant message");

    // Tenant/user scoping
    assert_eq!(evt.tenant_id, tenant_uuid);
    assert_eq!(evt.user_id, user_uuid);
}

// ============================================================================
// Test 5: Failed provider turn creates a failed usage event
// ============================================================================

#[sqlx::test]
async fn test_failed_provider_turn_creates_failed_usage_event(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant_uuid = Uuid::new_v4();
    let user_uuid = Uuid::new_v4();

    insert_tenant(&pool, tenant_uuid).await;
    insert_user(&pool, tenant_uuid, user_uuid).await;

    let error = billforge_ai_agent::models::ProviderChatError {
        kind: billforge_ai_agent::models::ProviderChatErrorKind::RateLimit,
        message: "quota exceeded".into(),
        status_code: Some(429),
        provider_code: Some("rate_limit".into()),
        retryable: Some(true),
    };
    let provider = Arc::new(FakeAiProvider::new().with_error(error));
    let agent = WinstonAgent::new(pool.clone(), provider);

    let request = ChatRequest {
        message: "Trigger error".to_string(),
        conversation_id: None,
    };

    let result = agent
        .chat(request, tenant_uuid.to_string(), user_uuid)
        .await;

    // The chat should fail
    assert!(result.is_err(), "chat should fail when provider errors");

    // But the conversation and user message should still exist (persistence happened first)
    assert_eq!(
        count_conversations(&pool, tenant_uuid, user_uuid).await,
        1,
        "conversation should be created before provider call"
    );

    // Find the conversation
    let conv_row: (Uuid,) = sqlx::query_as(
        "SELECT id FROM ai_conversations WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_uuid)
    .bind(user_uuid)
    .fetch_one(&pool)
    .await
    .expect("find conversation");
    let conv_id = conv_row.0;

    // One user message should exist (appended before provider call)
    let messages = read_messages(&pool, conv_id).await;
    assert_eq!(messages.len(), 1, "expected exactly 1 user message");
    assert_eq!(messages[0].0, "user");
    assert_eq!(messages[0].1, "Trigger error");

    // Exactly one usage event with success=false
    let events = read_usage_events(&pool, conv_id).await;
    assert_eq!(events.len(), 1, "expected exactly one usage event");

    let evt = &events[0];
    assert!(!evt.success, "usage event should be failed");
    assert!(evt.message_id.is_none(), "failed event should have no message_id");
    assert_eq!(evt.conversation_id, Some(conv_id));

    // Provider/model/route
    assert_eq!(evt.provider, "fake");
    assert!(evt.model.is_some(), "should have model");
    assert!(evt.model_route.is_some(), "should have model_route");

    // Latency
    assert!(evt.latency_ms.is_some(), "should have latency_ms");

    // Error fields
    assert!(
        evt.error_code.is_some(),
        "failed event should have error_code"
    );
    assert!(
        evt.error_message.is_some(),
        "failed event should have error_message"
    );
    assert_eq!(
        evt.error_message.as_deref(),
        Some("quota exceeded"),
        "error_message should match provider error"
    );

    // No tokens for failed requests
    assert!(evt.prompt_tokens.is_none(), "failed event should have no prompt_tokens");
    assert!(evt.completion_tokens.is_none(), "failed event should have no completion_tokens");
    assert!(evt.total_tokens.is_none(), "failed event should have no total_tokens");

    // Metadata should contain structured error info
    assert!(evt.metadata.is_object(), "metadata should be a JSON object");
}
