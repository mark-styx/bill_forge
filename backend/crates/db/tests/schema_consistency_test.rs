//! Schema consistency test for workflow migrations
//!
//! Verifies that run_workflow_migrations() produces the correct table schema
//! matching the canonical migration file (005_create_workflow_tables.sql).
//! This prevents future drift between the two migration paths.
//!
//! Run with: cargo test --test schema_consistency_test --features integration
//! Or:       cargo test --test schema_consistency_test -- --ignored

use billforge_core::TenantId;
use billforge_db::PgManager;
use uuid::Uuid;

/// Helper: create a fresh tenant database with all migrations applied.
async fn setup_tenant(tag: &str) -> (PgManager, TenantId, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".to_string());
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    let tenant_id: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("schema-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager.create_tenant(&tenant_id, &format!("Schema test {tag}"))
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("tenant pool")).clone();
    manager.run_tenant_migrations(&pool).await.expect("migrations");

    (manager, tenant_id, pool)
}

/// Assert that a given column exists on a table with the expected data type (prefix match).
async fn assert_column(pool: &sqlx::PgPool, table: &str, column: &str, type_prefix: &str) {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT data_type FROM information_schema.columns WHERE table_name = $1 AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_optional(pool)
    .await
    .expect("query information_schema");

    match row {
        Some((dtype,)) => {
            assert!(
                dtype.starts_with(type_prefix),
                "Column {}.{}: expected type starting with '{}', got '{}'",
                table,
                column,
                type_prefix,
                dtype,
            );
        }
        None => panic!("Column {}.{} not found", table, column),
    }
}

/// Assert that a table exists in the current schema.
async fn assert_table_exists(pool: &sqlx::PgPool, table: &str) {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT table_name FROM information_schema.tables WHERE table_name = $1",
    )
    .bind(table)
    .fetch_optional(pool)
    .await
    .expect("query information_schema");

    assert!(row.is_some(), "Table '{}' does not exist", table);
}

/// Assert that a column has a default value containing the given substring.
async fn assert_column_default_contains(pool: &sqlx::PgPool, table: &str, column: &str, substring: &str) {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT column_default FROM information_schema.columns WHERE table_name = $1 AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_optional(pool)
    .await
    .expect("query information_schema");

    match row {
        Some((Some(default),)) => {
            assert!(
                default.contains(substring),
                "Column {}.{}: expected default containing '{}', got '{}'",
                table,
                column,
                substring,
                default,
            );
        }
        _ => panic!("Column {}.{} has no default or does not exist", table, column),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_queue_items_has_tenant_id_and_status() {
    let (_manager, _tenant_id, pool) = setup_tenant("qi-cols").await;

    // queue_items must have tenant_id (VARCHAR) and status (VARCHAR) columns
    assert_column(&pool, "queue_items", "tenant_id", "character varying").await;
    assert_column(&pool, "queue_items", "status", "character varying").await;
    assert_column_default_contains(&pool, "queue_items", "status", "pending").await;
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_approval_requests_has_tenant_id_jsonb_requested_from_and_updated_at() {
    let (_manager, _tenant_id, pool) = setup_tenant("ar-cols").await;

    // approval_requests must have tenant_id, requested_from (JSONB), and updated_at
    assert_column(&pool, "approval_requests", "tenant_id", "character varying").await;
    assert_column(&pool, "approval_requests", "requested_from", "jsonb").await;
    assert_column(&pool, "approval_requests", "updated_at", "timestamp with time zone").await;
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_email_action_tokens_table_exists() {
    let (_manager, _tenant_id, pool) = setup_tenant("eat").await;

    assert_table_exists(&pool, "email_action_tokens").await;
    assert_column(&pool, "email_action_tokens", "tenant_id", "character varying").await;
    assert_column(&pool, "email_action_tokens", "token_hash", "character varying").await;
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_workflow_audit_log_table_exists() {
    let (_manager, _tenant_id, pool) = setup_tenant("wal").await;

    assert_table_exists(&pool, "workflow_audit_log").await;
    assert_column(&pool, "workflow_audit_log", "entity_type", "character varying").await;
    assert_column(&pool, "workflow_audit_log", "ip_address", "inet").await;
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_workflow_templates_table_exists() {
    let (_manager, _tenant_id, pool) = setup_tenant("wt").await;

    assert_table_exists(&pool, "workflow_templates").await;
    assert_column(&pool, "workflow_templates", "stages", "jsonb").await;
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_workflow_tables_have_uuid_pk_defaults() {
    let (_manager, _tenant_id, pool) = setup_tenant("pk-defaults").await;

    // Verify that UUID PKs have DEFAULT gen_random_uuid()
    for table in &[
        "workflow_rules",
        "work_queues",
        "queue_items",
        "assignment_rules",
        "approval_requests",
        "email_action_tokens",
        "approval_delegations",
        "workflow_audit_log",
    ] {
        assert_column_default_contains(&pool, table, "id", "gen_random_uuid").await;
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn test_ai_conversation_tables_schema() {
    let (_manager, _tenant_id, pool) = setup_tenant("ai-conv").await;

    // ai_conversations table with required columns
    assert_table_exists(&pool, "ai_conversations").await;
    assert_column(&pool, "ai_conversations", "tenant_id", "uuid").await;
    assert_column(&pool, "ai_conversations", "user_id", "uuid").await;
    assert_column(&pool, "ai_conversations", "title", "text").await;
    assert_column(&pool, "ai_conversations", "metadata", "jsonb").await;
    assert_column(&pool, "ai_conversations", "created_at", "timestamp with time zone").await;
    assert_column(&pool, "ai_conversations", "updated_at", "timestamp with time zone").await;
    assert_column_default_contains(&pool, "ai_conversations", "id", "gen_random_uuid").await;

    // ai_messages table with required columns
    assert_table_exists(&pool, "ai_messages").await;
    assert_column(&pool, "ai_messages", "tenant_id", "uuid").await;
    assert_column(&pool, "ai_messages", "user_id", "uuid").await;
    assert_column(&pool, "ai_messages", "conversation_id", "uuid").await;
    assert_column(&pool, "ai_messages", "role", "text").await;
    assert_column(&pool, "ai_messages", "content", "text").await;

    // Provider-neutral usage columns
    assert_column(&pool, "ai_messages", "provider", "text").await;
    assert_column(&pool, "ai_messages", "model", "text").await;
    assert_column(&pool, "ai_messages", "model_route", "text").await;
    assert_column(&pool, "ai_messages", "prompt_tokens", "integer").await;
    assert_column(&pool, "ai_messages", "completion_tokens", "integer").await;
    assert_column(&pool, "ai_messages", "total_tokens", "integer").await;
    assert_column(&pool, "ai_messages", "finish_reason", "text").await;
    assert_column(&pool, "ai_messages", "provider_request_id", "text").await;
    assert_column(&pool, "ai_messages", "latency_ms", "bigint").await;
    assert_column(&pool, "ai_messages", "metadata", "jsonb").await;
    assert_column(&pool, "ai_messages", "created_at", "timestamp with time zone").await;
    assert_column_default_contains(&pool, "ai_messages", "id", "gen_random_uuid").await;

    // Verify composite FK exists on ai_messages referencing (id, tenant_id, user_id)
    // on ai_conversations, preventing cross-tenant/user message attachment.
    let fk_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.table_name = 'ai_messages'
            AND tc.constraint_type = 'FOREIGN KEY'
            AND tc.constraint_name LIKE '%tenant_id%'
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("query FK constraints");

    assert!(fk_count >= 1, "Expected composite FK on ai_messages(tenant_id, user_id, conversation_id) referencing ai_conversations");
}
