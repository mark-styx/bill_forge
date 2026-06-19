//! Integration tests for the in-app notification inbox (refs #375).
//!
//! Verifies that:
//!  1. The primary producer — `ApprovalRepository::create` — writes an
//!     `in_app_notifications` row for each named approver immediately after
//!     inserting an approval request (the bell finally has real data).
//!  2. `GET /notifications` SQL (tenant + user scoped) returns only the
//!     calling user's rows and the correct unread_count.
//!  3. The mark-read / mark-all-read UPDATEs flip `read_at` for the right rows.
//!  4. Cross-tenant access via the RLS `app.current_tenant_id` policy returns
//!     zero rows — preventing regression into the #368 missing-RLS pattern.
//!
//! Run: `cargo test -p billforge-api --test notifications_inbox_test -- --ignored`

#![allow(warnings)]

use billforge_core::{
    domain::{ApprovalRequest, ApprovalStatus, ApprovalTarget},
    traits::ApprovalRepository,
    InvoiceId, TenantId, UserId, WorkflowRuleId,
};
use billforge_db::{PgManager, WorkflowRepositoryImpl};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (mirror backend/crates/db/tests/tenant_isolation_test.rs)
// ---------------------------------------------------------------------------

async fn seed_tenant(pool: &sqlx::PgPool, tenant_id: &TenantId, name: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(name.to_lowercase().replace(' ', "-"))
    .execute(pool)
    .await
    .expect("seed tenant");
}

async fn seed_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("user-{}@test.com", user_id))
    .bind("hash")
    .bind("Inbox Test User")
    .execute(pool)
    .await
    .expect("seed user");
}

async fn seed_vendor(pool: &sqlx::PgPool, tenant_id: &TenantId, vendor_id: Uuid) {
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind("Inbox Test Vendor")
    .execute(pool)
    .await
    .expect("seed vendor");
}

async fn seed_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
    vendor_id: Uuid,
    user_id: Uuid,
    invoice_number: &str,
) {
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_id, vendor_name, invoice_number, \
                                total_amount_cents, document_id, created_by) \
         VALUES ($1, $2, $3, $4, $5, 1000, $6, $7)",
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .bind("Inbox Test Vendor")
    .bind(invoice_number)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed invoice");
}

async fn seed_workflow_rule(pool: &sqlx::PgPool, tenant_id: &TenantId, rule_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO workflow_rules (id, tenant_id, name, priority, is_active, rule_type,
                  conditions, actions, created_at, updated_at)
           VALUES ($1, $2, 'Inbox Test Rule', 1, true, 'approval',
                   '[]'::jsonb, '[]'::jsonb, NOW(), NOW())"#,
    )
    .bind(rule_id)
    .bind(*tenant_id.as_uuid())
    .execute(pool)
    .await
    .expect("seed workflow rule");
}

/// Two-tenant fixture mirroring tenant_isolation_test.rs::setup_two_tenants.
async fn setup_two_tenants(
    tag: &str,
) -> (PgManager, TenantId, TenantId, sqlx::PgPool, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("PgManager");

    let tenant_a: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("inbox-a-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();
    let tenant_b: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("inbox-b-{tag}").as_bytes())
        .to_string()
        .parse()
        .unwrap();

    manager.delete_tenant(&tenant_a).await.ok();
    manager.delete_tenant(&tenant_b).await.ok();
    manager
        .create_tenant(&tenant_a, &format!("Inbox Tenant A {tag}"))
        .await
        .expect("create tenant A");
    manager
        .create_tenant(&tenant_b, &format!("Inbox Tenant B {tag}"))
        .await
        .expect("create tenant B");

    let pool_a = (*manager.tenant(&tenant_a).await.expect("pool A")).clone();
    let pool_b = (*manager.tenant(&tenant_b).await.expect("pool B")).clone();

    // run_tenant_migrations provisions in_app_notifications (migration 134).
    manager.run_tenant_migrations(&pool_a).await.expect("migrate A");
    manager.run_tenant_migrations(&pool_b).await.expect("migrate B");
    seed_tenant(&pool_a, &tenant_a, &format!("Inbox A {tag}")).await;
    seed_tenant(&pool_b, &tenant_b, &format!("Inbox B {tag}")).await;

    (manager, tenant_a, tenant_b, pool_a, pool_b)
}

async fn teardown_two_tenants(manager: &PgManager, tenant_a: &TenantId, tenant_b: &TenantId) {
    manager.delete_tenant(tenant_a).await.ok();
    manager.delete_tenant(tenant_b).await.ok();
}

// ---------------------------------------------------------------------------
// Producer assertion helpers
// ---------------------------------------------------------------------------

/// Counts unread in_app_notifications for a user under a tenant pool whose
/// `app.current_tenant_id` is already bound (PgManager sets it per-connection).
async fn count_unread(pool: &sqlx::PgPool, user_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM in_app_notifications \
         WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("count unread")
}

async fn read_state(pool: &sqlx::PgPool, notification_id: Uuid) -> Option<bool> {
    sqlx::query_scalar::<_, Option<bool>>(
        "SELECT (read_at IS NOT NULL) FROM in_app_notifications WHERE id = $1",
    )
    .bind(notification_id)
    .fetch_optional(pool)
    .await
    .expect("read state")
    .flatten()
}

// ===========================================================================
// Test 1: ApprovalRepository::create fans out an in-app notification
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn producer_writes_notification_on_approval_request_creation() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("producer").await;

    let approver_a = Uuid::new_v4();
    let creator_a = Uuid::new_v4();
    let vendor_a = Uuid::new_v4();
    let invoice_a = Uuid::new_v4();
    let rule_a = Uuid::new_v4();

    seed_user(&pool_a, &tenant_a, approver_a).await;
    seed_user(&pool_a, &tenant_a, creator_a).await;
    seed_vendor(&pool_a, &tenant_a, vendor_a).await;
    seed_invoice(&pool_a, &tenant_a, invoice_a, vendor_a, creator_a, "PROD-INV-001").await;
    seed_workflow_rule(&pool_a, &tenant_a, rule_a).await;

    let before = count_unread(&pool_a, approver_a).await;
    assert_eq!(before, 0, "inbox should start empty for the approver");

    let repo = WorkflowRepositoryImpl::new(Arc::new(pool_a.clone()));
    let request = ApprovalRequest {
        id: Uuid::new_v4(),
        invoice_id: InvoiceId(invoice_a),
        tenant_id: tenant_a.clone(),
        rule_id: WorkflowRuleId(rule_a),
        requested_from: ApprovalTarget::User(UserId(approver_a)),
        status: ApprovalStatus::Pending,
        comments: None,
        responded_by: None,
        responded_at: None,
        created_at: Utc::now(),
        expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
    };
    repo.create(&tenant_a, request).await.expect("create approval");

    let after = count_unread(&pool_a, approver_a).await;
    assert_eq!(
        after, 1,
        "ApprovalRepository::create must fan out exactly one inbox notification to the approver"
    );

    // The fanned-out row should carry the approval_request kind + a deep-link.
    let (kind, link): (String, Option<String>) = sqlx::query_as(
        "SELECT kind, link FROM in_app_notifications \
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(approver_a)
    .fetch_one(&pool_a)
    .await
    .expect("fetch fanned-out row");
    assert_eq!(kind, "approval_request");
    assert!(link.as_deref().unwrap_or("").starts_with("/processing/approvals/"));

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 2: GET-style query returns only the caller's tenant rows
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn feed_query_is_tenant_and_user_scoped() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) = setup_two_tenants("feed-scope").await;

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_user(&pool_b, &tenant_b, user_b).await;

    // Direct fixture inserts (each pool is RLS-scoped to its own tenant).
    sqlx::query(
        "INSERT INTO in_app_notifications (tenant_id, user_id, kind, title, message, link, created_at) \
         VALUES ($1, $2, 'system', 'A-only', NULL, NULL, NOW())",
    )
    .bind(*tenant_a.as_uuid())
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("insert A notification");

    sqlx::query(
        "INSERT INTO in_app_notifications (tenant_id, user_id, kind, title, message, link, created_at) \
         VALUES ($1, $2, 'system', 'B-only', NULL, NULL, NOW())",
    )
    .bind(*tenant_b.as_uuid())
    .bind(user_b)
    .execute(&pool_b)
    .await
    .expect("insert B notification");

    // Tenant A's view should see exactly its own row, not tenant B's.
    let titles_a: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM in_app_notifications WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_a)
    .fetch_all(&pool_a)
    .await
    .expect("list A");
    assert_eq!(titles_a, vec!["A-only".to_string()]);
    assert_eq!(
        count_unread(&pool_a, user_a).await,
        1,
        "unread_count must reflect tenant A's single unread row"
    );

    // Tenant B's view should see only its own row.
    let titles_b: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM in_app_notifications WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_b)
    .fetch_all(&pool_b)
    .await
    .expect("list B");
    assert_eq!(titles_b, vec!["B-only".to_string()]);

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Test 3: mark-read flips read_at; subsequent count reflects it
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn mark_read_flips_read_at_and_decrements_unread() {
    let (manager, tenant_a, _tenant_b, pool_a, _pool_b) = setup_two_tenants("mark-read").await;

    let user_a = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;

    let notif_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO in_app_notifications (id, tenant_id, user_id, kind, title, message, link, created_at) \
         VALUES ($1, $2, $3, 'system', 'Mark Me', NULL, NULL, NOW())",
    )
    .bind(notif_id)
    .bind(*tenant_a.as_uuid())
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("insert notification");

    assert_eq!(read_state(&pool_a, notif_id).await, Some(false));
    assert_eq!(count_unread(&pool_a, user_a).await, 1);

    // Mirrors the POST /notifications/{id}/read handler UPDATE.
    let result = sqlx::query(
        "UPDATE in_app_notifications SET read_at = NOW() WHERE id = $1 AND user_id = $2",
    )
    .bind(notif_id)
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("mark read");
    assert_eq!(
        result.rows_affected(),
        1,
        "mark-read UPDATE must affect exactly the caller's row"
    );

    assert_eq!(read_state(&pool_a, notif_id).await, Some(true));
    assert_eq!(
        count_unread(&pool_a, user_a).await,
        0,
        "unread_count must drop to zero after marking read"
    );

    teardown_two_tenants(&manager, &tenant_a, &_tenant_b).await;
}

// ===========================================================================
// Test 4: Cross-tenant lookup by id returns nothing (RLS isolation)
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn cross_tenant_lookup_by_id_returns_no_rows() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) =
        setup_two_tenants("cross-tenant").await;

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_user(&pool_b, &tenant_b, user_b).await;

    let notif_a = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO in_app_notifications (id, tenant_id, user_id, kind, title, message, link, created_at) \
         VALUES ($1, $2, $3, 'system', 'A-secret', NULL, NULL, NOW())",
    )
    .bind(notif_a)
    .bind(*tenant_a.as_uuid())
    .bind(user_a)
    .execute(&pool_a)
    .await
    .expect("insert A notification");

    // Tenant B's pool is RLS-scoped to tenant_b; querying for tenant A's id
    // must return zero rows even though user_b is also passed.
    let leaked: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM in_app_notifications WHERE id = $1 AND user_id = $2")
            .bind(notif_a)
            .bind(user_b)
            .fetch_optional(&pool_b)
            .await
            .expect("cross-tenant query");
    assert!(
        leaked.is_none(),
        "Cross-tenant notification lookup must be blocked by RLS — got {:?}",
        leaked
    );

    // And a cross-tenant DELETE by id must affect zero rows.
    let delete_result = sqlx::query("DELETE FROM in_app_notifications WHERE id = $1")
        .bind(notif_a)
        .execute(&pool_b)
        .await
        .expect("cross-tenant delete");
    assert_eq!(
        delete_result.rows_affected(),
        0,
        "Cross-tenant DELETE must affect 0 rows under RLS"
    );

    // The original row in tenant A must still be intact.
    let still_there: Option<(String,)> =
        sqlx::query_as("SELECT title FROM in_app_notifications WHERE id = $1")
            .bind(notif_a)
            .fetch_optional(&pool_a)
            .await
            .expect("re-fetch A");
    assert_eq!(
        still_there.map(|(t,)| t),
        Some("A-secret".to_string()),
        "Cross-tenant DELETE must not mutate the owning tenant's row"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}

// ===========================================================================
// Test 5: read-all flips every unread row for the caller
// ===========================================================================

#[tokio::test]
#[ignore = "requires Postgres + billforge_app role; run with --ignored"]
async fn mark_all_read_clears_unread_for_caller_only() {
    let (manager, tenant_a, tenant_b, pool_a, pool_b) = setup_two_tenants("read-all").await;

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    seed_user(&pool_a, &tenant_a, user_a).await;
    seed_user(&pool_b, &tenant_b, user_b).await;

    for _ in 0..3 {
        sqlx::query(
            "INSERT INTO in_app_notifications (tenant_id, user_id, kind, title, message, link, created_at) \
             VALUES ($1, $2, 'system', 'A', NULL, NULL, NOW())",
        )
        .bind(*tenant_a.as_uuid())
        .bind(user_a)
        .execute(&pool_a)
        .await
        .expect("insert A");
    }
    sqlx::query(
        "INSERT INTO in_app_notifications (tenant_id, user_id, kind, title, message, link, created_at) \
         VALUES ($1, $2, 'system', 'B', NULL, NULL, NOW())",
    )
    .bind(*tenant_b.as_uuid())
    .bind(user_b)
    .execute(&pool_b)
    .await
    .expect("insert B");

    assert_eq!(count_unread(&pool_a, user_a).await, 3);
    assert_eq!(count_unread(&pool_b, user_b).await, 1);

    // Mirrors the POST /notifications/read-all handler UPDATE.
    sqlx::query("UPDATE in_app_notifications SET read_at = NOW() WHERE user_id = $1 AND read_at IS NULL")
        .bind(user_a)
        .execute(&pool_a)
        .await
        .expect("read-all A");

    assert_eq!(
        count_unread(&pool_a, user_a).await,
        0,
        "read-all must clear every unread row for the caller"
    );
    assert_eq!(
        count_unread(&pool_b, user_b).await,
        1,
        "read-all for tenant A must not touch tenant B's unread rows"
    );

    teardown_two_tenants(&manager, &tenant_a, &tenant_b).await;
}
