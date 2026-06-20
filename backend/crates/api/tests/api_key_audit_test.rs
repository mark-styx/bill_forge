//! Audit-trail coverage for AuthService API key mutations (#410).
//!
//! Asserts that the AuditEntry shape AuthService::create_api_key and
//! revoke_api_key emit on success actually lands in `audit_log` via the
//! per-tenant AuditRepositoryImpl path, including with `user_id=NULL`
//! (PAT-style actor encoded only in `changes->>'user_email'`).
//!
//! Run: `cargo test -p billforge-api --test api_key_audit_test`

#![allow(warnings)]

use billforge_core::domain::{AuditAction, AuditEntry, ResourceType};
use billforge_core::traits::AuditService;
use billforge_core::TenantId;
use billforge_db::repositories::AuditRepositoryImpl;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test api_key_audit_test -- --ignored
async fn api_key_create_and_revoke_emit_audit_entries(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    setup_schema(&pool, &tenant_id).await;
    let audit_repo = AuditRepositoryImpl::new(pool.clone());

    // --- Mirror AuthService::create_api_key's audit emission ---
    let key_id = Uuid::new_v4();
    let key_prefix = "bf_abcd1234";
    let create_entry = AuditEntry::new(
        tenant_id.clone(),
        None,
        AuditAction::Create,
        ResourceType::ApiKey,
        key_id.to_string(),
        format!("Created API key '{}'", "integration test key"),
    )
    .with_user_email(format!("api-key:{}", key_prefix))
    .with_metadata(serde_json::json!({
        "key_prefix": key_prefix,
        "roles": ["tenant_admin"],
        "expires_at": serde_json::Value::Null,
    }));
    audit_repo
        .log(create_entry)
        .await
        .expect("create audit must succeed with user_id=NULL");

    // --- Mirror AuthService::revoke_api_key's audit emission ---
    let revoke_entry = AuditEntry::new(
        tenant_id.clone(),
        None,
        AuditAction::Delete,
        ResourceType::ApiKey,
        key_id.to_string(),
        "Revoked API key",
    )
    .with_user_email(format!("api-key:{}", key_id))
    .with_metadata(serde_json::json!({ "key_id": key_id }));
    audit_repo
        .log(revoke_entry)
        .await
        .expect("revoke audit must succeed");

    // --- Verify both rows landed with the correct shape ---
    let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"SELECT action, resource_type, resource_id, user_id IS NULL
           FROM audit_log
           WHERE resource_id = $1
           ORDER BY action"#,
    )
    .bind(key_id.to_string())
    .fetch_all(&*pool)
    .await
    .expect("query audit rows");

    assert_eq!(rows.len(), 2, "expected one create and one delete row");
    assert_eq!(rows[0].0, "create");
    assert_eq!(rows[0].1, "api_key");
    assert_eq!(rows[0].2, key_id.to_string());
    assert!(
        rows[0].3,
        "create row user_id must be NULL (PAT-originated)"
    );
    assert_eq!(rows[1].0, "delete");
    assert_eq!(rows[1].1, "api_key");
    assert!(rows[1].3, "delete row user_id must be NULL");

    // Confirm the api-key identity is preserved in changes->>'user_email'
    let create_email: Option<String> = sqlx::query_scalar(
        "SELECT changes->>'user_email' FROM audit_log WHERE resource_id = $1 AND action = 'create'",
    )
    .bind(key_id.to_string())
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(create_email.as_deref(), Some(&*format!("api-key:{}", key_prefix)));
}
