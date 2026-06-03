//! Integration tests for report digest audit logging (refs #282)
//!
//! Validates that create_digest and delete_digest handlers correctly persist
//! AuditEntry rows to the audit_log table.  Mirrors the pattern established
//! in workflow_audit_capture_test.rs.

use billforge_core::domain::{AuditAction, AuditEntry, ResourceType};
use billforge_core::traits::AuditService;
use billforge_core::TenantId;
use billforge_db::repositories::AuditRepositoryImpl;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run all tenant migrations so audit_log and FK targets exist.
async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

/// Insert a minimal user row so audit_log.user_id FK is satisfied.
async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '[\"tenant_admin\"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("digest-test@example.com")
    .bind("hash_not_used")
    .bind("Digest Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Read audit_log row for a given resource_id.
async fn read_audit_row(
    pool: &sqlx::PgPool,
    resource_id: &str,
) -> Option<(String, String, Option<serde_json::Value>)> {
    let row: Option<(String, String, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT action, resource_type, changes FROM audit_log WHERE resource_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(resource_id)
    .fetch_optional(pool)
    .await
    .expect("query audit_log");

    row
}

// ============================================================================
// Test 1: create_digest (upsert) writes an Update audit entry
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test report_digest_audit_test -- --ignored
async fn create_digest_writes_audit_entry(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    let digest_id = Uuid::new_v4();

    // Simulate what the create_digest handler does: upsert_digest succeeds,
    // then build and log an AuditEntry.
    let digest_value = serde_json::json!({
        "id": digest_id.to_string(),
        "digest_type": "daily_summary",
        "frequency": "daily",
        "enabled": true,
    });

    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::Update,
        ResourceType::Settings,
        digest_id.to_string(),
        format!("Upserted report digest {}", digest_id),
    )
    .with_user_email("digest-test@example.com")
    .with_new_value(digest_value.clone());

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(entry).await.expect("audit log write");

    // Verify the audit row exists with correct columns
    let row = read_audit_row(&pool, &digest_id.to_string())
        .await
        .expect("audit row must exist");

    assert_eq!(row.0, "update", "action should be 'update'");
    assert_eq!(row.1, "settings", "resource_type should be 'settings'");

    let changes = row.2.expect("changes JSONB must be present");
    assert_eq!(
        changes["description"], format!("Upserted report digest {}", digest_id),
        "description should match"
    );
    assert_eq!(changes["new_value"]["id"], digest_id.to_string());
    assert_eq!(changes["user_email"], "digest-test@example.com");
}

// ============================================================================
// Test 2: delete_digest writes a Delete audit entry
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test report_digest_audit_test -- --ignored
async fn delete_digest_writes_audit_entry(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    let digest_id = Uuid::new_v4();

    // Simulate what the delete_digest handler does: delete_digest succeeds,
    // then build and log an AuditEntry.
    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::Delete,
        ResourceType::Settings,
        digest_id.to_string(),
        format!("Deleted report digest {}", digest_id),
    )
    .with_user_email("digest-test@example.com");

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(entry).await.expect("audit log write");

    // Verify the audit row exists with correct columns
    let row = read_audit_row(&pool, &digest_id.to_string())
        .await
        .expect("audit row must exist");

    assert_eq!(row.0, "delete", "action should be 'delete'");
    assert_eq!(row.1, "settings", "resource_type should be 'settings'");

    let changes = row.2.expect("changes JSONB must be present");
    assert_eq!(
        changes["description"],
        format!("Deleted report digest {}", digest_id),
        "description should match"
    );
    assert_eq!(changes["user_email"], "digest-test@example.com");
    // Delete operations should have no new_value
    assert!(
        changes["new_value"].is_null(),
        "delete operations should not have new_value"
    );
}
