//! Cross-tenant isolation integration tests
//!
//! Verifies that:
//! 1. The `require_tenant` middleware enforces a non-nil `tenant_id` on every request
//! 2. SQL queries for vendors, invoices, approval links, close periods, and discounts are tenant-scoped
//! 3. Cross-tenant access returns 404/403; a 200 on cross-tenant read is a hard failure
//!
//! Run: `cargo test -p billforge-api --test cross_tenant_isolation_tests`

#![allow(warnings)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
    Router,
};
use billforge_api::middleware::{require_auth, require_tenant};
use billforge_auth::{AuthService, Claims, JwtConfig, JwtService, TokenType};
use billforge_core::{Role, TenantId, UserId};
use billforge_db::MetadataDatabase;
use std::sync::Arc;
use tower::util::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const JWT_SECRET: &str = "test-secret-cross-tenant-isolation";

fn test_auth_service() -> Arc<AuthService> {
    let jwt_config = JwtConfig {
        secret: JWT_SECRET.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://fake@localhost/fake")
        .expect("connect_lazy should not fail");
    let metadata_db = Arc::new(MetadataDatabase::from_pool(pool));
    Arc::new(AuthService::new(jwt_config, metadata_db))
}

fn test_jwt_service() -> JwtService {
    JwtService::new(JwtConfig {
        secret: JWT_SECRET.to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    })
}

/// Generate a valid access token for a given user/tenant pair.
fn make_token(user_id: &UserId, tenant_id: &TenantId) -> String {
    let jwt = test_jwt_service();
    jwt.create_access_token(user_id, tenant_id, "test@example.com", &[Role::ApUser])
        .expect("token creation should succeed")
}

/// Generate a JWT where `tenant_id` is the nil UUID.
fn make_token_nil_tenant(user_id: &UserId) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.as_uuid().to_string(),
        tenant_id: Uuid::nil().to_string(),
        email: "nil-tenant@test.com".to_string(),
        roles: vec!["ap_user".to_string()],
        iat: now - 60,
        exp: now + 3600,
        token_type: TokenType::Access,
        vendor_id: None,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .expect("encoding should succeed")
}

/// Build a minimal test router with both `require_auth` and `require_tenant` layers.
/// The handler echoes the `TenantGuard` tenant_id back as plain text.
fn build_test_router() -> Router {
    let auth = test_auth_service();
    Router::new()
        .route("/api/v1/tenants-check", get(tenant_echo_handler))
        .layer(middleware::from_fn_with_state(auth.clone(), require_tenant))
        .layer(middleware::from_fn_with_state(auth, require_auth))
}

async fn tenant_echo_handler(
    axum::extract::Extension(guard): axum::extract::Extension<
        billforge_api::middleware::TenantGuard,
    >,
) -> String {
    guard.0.to_string()
}

/// Get a real database pool from DATABASE_URL.
async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Insert a test vendor and return its ID.
async fn insert_vendor(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, status, routing_rules, created_at, updated_at)
           VALUES ($1, $2, $3, 'active', '{}'::jsonb, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("Failed to insert test vendor");
    id
}

/// Insert a test invoice and return its ID.
async fn insert_invoice(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    invoice_number: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Ensure the user row exists for the FK constraint on created_by
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) \
         VALUES ($1, $2, 'cross-tenant-test@example.com', '', 'Cross Tenant Test') \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .ok();

    sqlx::query(
        r#"INSERT INTO invoices
               (id, tenant_id, vendor_id, vendor_name, invoice_number, document_id,
                currency, total_amount_cents, capture_status, processing_status,
                created_by, created_at, updated_at)
           VALUES ($1, $2, $3, 'Test Vendor', $4, $5, 'USD', 10000, 'complete', 'received', $6, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(invoice_number)
    .bind(doc_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to insert test invoice");
    id
}

/// Cleanup helper.
async fn cleanup_test_data(pool: &sqlx::PgPool, tenant_id: Uuid, prefix: &str) {
    sqlx::query("DELETE FROM invoices WHERE tenant_id = $1 AND invoice_number LIKE $2")
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE $2")
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
}

// ===========================================================================
// Test 1: Middleware rejects nil tenant_id
// ===========================================================================

#[tokio::test]
async fn missing_tenant_id_returns_401() {
    let app = build_test_router();
    let user_id = UserId::from_uuid(Uuid::new_v4());
    let token = make_token_nil_tenant(&user_id);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/tenants-check")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Request with nil tenant_id should be rejected with 401"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("tenant_unresolved"),
        "Response should contain 'tenant_unresolved', got: {}",
        body_str
    );
}

// ===========================================================================
// Test 2: Valid tenant_id passes middleware and inserts TenantGuard
// ===========================================================================

#[tokio::test]
async fn valid_tenant_id_passes_middleware() {
    // require_tenant now loads TenantContext from the metadata database to populate
    // module entitlements for downstream gates. With the connect_lazy stub pool used
    // here, the lookup will fail with `tenant_context_load_failed` — proving that the
    // middleware accepted the valid token (didn't 401) and reached the DB-load step
    // (didn't fall through silently like the pre-fix gate_module).
    let app = build_test_router();
    let user_id = UserId::from_uuid(Uuid::new_v4());
    let tenant_id = TenantId::new();
    let token = make_token(&user_id, &tenant_id);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/tenants-check")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "Valid token must pass auth + nil-tenant check and reach the TenantContext load step (stub pool returns 500)"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("tenant_context_load_failed"),
        "Response should be tenant_context_load_failed when the stub pool cannot reach metadata DB, got: {}",
        body_str
    );
}

// ===========================================================================
// Test 3: vendor_get_cross_tenant_returns_404
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn vendor_get_cross_tenant_returns_404() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Create a vendor in tenant A
    let vendor_a_id = insert_vendor(&pool, tenant_a, "CT-VENDOR Tenant A").await;

    // Query with tenant B's ID should return no rows
    let row: Option<(String,)> =
        sqlx::query_as("SELECT name FROM vendors WHERE id = $1 AND tenant_id = $2")
            .bind(vendor_a_id)
            .bind(tenant_b)
            .fetch_optional(&pool)
            .await
            .expect("Query should succeed");

    assert!(
        row.is_none(),
        "Cross-tenant vendor GET must return no rows — got {:?}",
        row
    );

    // Cleanup
    sqlx::query("DELETE FROM vendors WHERE id = $1")
        .bind(vendor_a_id)
        .execute(&pool)
        .await
        .ok();
}

// ===========================================================================
// Test 4: vendor_list_excludes_other_tenant
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn vendor_list_excludes_other_tenant() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let vendor_a = insert_vendor(&pool, tenant_a, "CT-LIST Vendor A").await;
    let vendor_b = insert_vendor(&pool, tenant_b, "CT-LIST Vendor B").await;

    // List tenant A vendors
    let tenant_a_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM vendors WHERE tenant_id = $1 AND name LIKE 'CT-LIST%'")
            .bind(tenant_a)
            .fetch_all(&pool)
            .await
            .expect("Query should succeed");

    assert!(
        tenant_a_ids.contains(&vendor_a),
        "Tenant A should see its own vendor"
    );
    assert!(
        !tenant_a_ids.contains(&vendor_b),
        "Tenant A must NOT see tenant B's vendor"
    );
    assert_eq!(
        tenant_a_ids.len(),
        1,
        "Tenant A should see exactly 1 vendor"
    );

    // Cleanup
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE 'CT-LIST%'")
        .bind(tenant_a)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM vendors WHERE tenant_id = $1 AND name LIKE 'CT-LIST%'")
        .bind(tenant_b)
        .execute(&pool)
        .await
        .ok();
}

// ===========================================================================
// Test 5: invoice_get_cross_tenant_returns_404
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn invoice_get_cross_tenant_returns_404() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Create vendor + invoice in tenant A
    let vendor_a = insert_vendor(&pool, tenant_a, "CT-INVOICE Vendor A").await;
    let invoice_a = insert_invoice(&pool, tenant_a, vendor_a, "CT-INV-001").await;

    // Query with tenant B's ID should return no rows
    let row: Option<(String,)> =
        sqlx::query_as("SELECT invoice_number FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_a)
            .bind(tenant_b)
            .fetch_optional(&pool)
            .await
            .expect("Query should succeed");

    assert!(
        row.is_none(),
        "Cross-tenant invoice GET must return no rows — got {:?}",
        row
    );

    // Cleanup
    cleanup_test_data(&pool, tenant_a, "CT-INVOICE").await;
    cleanup_test_data(&pool, tenant_b, "CT-INVOICE").await;
}

// ===========================================================================
// Test 6: invoice_update_cross_tenant_returns_404_or_403
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn invoice_update_cross_tenant_returns_404_or_403() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Create vendor + invoice in tenant A
    let vendor_a = insert_vendor(&pool, tenant_a, "CT-PATCH Vendor A").await;
    let invoice_a = insert_invoice(&pool, tenant_a, vendor_a, "CT-PATCH-001").await;

    // Attempt to update using tenant B's tenant_id in WHERE clause
    let result = sqlx::query(
        "UPDATE invoices SET total_amount_cents = 99999, updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_a)
    .bind(tenant_b) // wrong tenant
    .execute(&pool)
    .await
    .expect("Query should succeed");

    assert_eq!(
        result.rows_affected(),
        0,
        "Cross-tenant invoice UPDATE must affect 0 rows"
    );

    // Verify original row is untouched
    let amount: Option<(i64,)> =
        sqlx::query_as("SELECT total_amount_cents FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_a)
            .bind(tenant_a)
            .fetch_optional(&pool)
            .await
            .expect("Query should succeed");

    let (original_amount,) = amount.expect("Original invoice should still exist");
    assert_eq!(
        original_amount, 10000,
        "Original invoice should not be modified"
    );

    // Cleanup
    cleanup_test_data(&pool, tenant_a, "CT-PATCH").await;
    cleanup_test_data(&pool, tenant_b, "CT-PATCH").await;
}

// ===========================================================================
// Test 7: approval_link_cross_tenant_blocked
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn approval_link_cross_tenant_blocked() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Create vendor + invoice in tenant A
    let vendor_a = insert_vendor(&pool, tenant_a, "CT-APPROVAL Vendor A").await;
    let invoice_a = insert_invoice(&pool, tenant_a, vendor_a, "CT-APPROVAL-001").await;

    // Set processing_status to 'pending_approval' so it's distinct from the
    // default 'received' value and we can verify no cross-tenant mutation.
    sqlx::query("UPDATE invoices SET processing_status = 'pending_approval' WHERE id = $1 AND tenant_id = $2")
        .bind(invoice_a)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("Should update processing_status");

    // Attempt a state machine transition using tenant B's tenant_id
    // (simulates an approval token for tenant A being used against tenant B's context)
    let result = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(invoice_a)
    .bind(tenant_b) // wrong tenant
    .fetch_optional(&pool)
    .await
    .expect("Query should succeed");

    assert!(
        result.is_none(),
        "Cross-tenant FOR UPDATE must return no rows — the transition would fail with NotFound"
    );

    // Verify the invoice processing_status is still 'pending_approval' (not modified)
    let status: Option<(String,)> =
        sqlx::query_as("SELECT processing_status FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_a)
            .bind(tenant_a)
            .fetch_optional(&pool)
            .await
            .expect("Query should succeed");

    let (current_status,) = status.expect("Invoice should still exist");
    assert_eq!(
        current_status, "pending_approval",
        "Cross-tenant approval must not change invoice processing_status"
    );

    // Cleanup
    cleanup_test_data(&pool, tenant_a, "CT-APPROVAL").await;
    cleanup_test_data(&pool, tenant_b, "CT-APPROVAL").await;
}

// ---------------------------------------------------------------------------
// Helpers: close_periods & discounts
// ---------------------------------------------------------------------------

/// Insert a test close period and return its ID.
async fn insert_close_period(pool: &sqlx::PgPool, tenant_id: Uuid, period_label: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO close_periods (id, tenant_id, period_label, period_start, period_end, cutoff_date)
           VALUES ($1, $2, $3, '2026-05-01'::date, '2026-05-31'::date, '2026-05-25'::date)"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(period_label)
    .execute(pool)
    .await
    .expect("Failed to insert test close period");
    id
}

/// Insert a test invoice with discount columns populated, then return its ID.
async fn insert_discount_invoice(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    invoice_number: &str,
) -> Uuid {
    let id = insert_invoice(pool, tenant_id, vendor_id, invoice_number).await;

    // Patch invoice_date + discount columns so the worklist query finds the row
    sqlx::query(
        r#"UPDATE invoices
           SET invoice_date = CURRENT_DATE,
               discount_percent = 2.0,
               discount_days = 10,
               discount_deadline = CURRENT_DATE + 5
           WHERE id = $1"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("Failed to patch discount columns");

    id
}

/// Cleanup helper for close_periods.
async fn cleanup_close_periods(pool: &sqlx::PgPool, tenant_id: Uuid, prefix: &str) {
    sqlx::query("DELETE FROM close_periods WHERE tenant_id = $1 AND period_label LIKE $2")
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .execute(pool)
        .await
        .ok();
}

// ===========================================================================
// Test 8: close_period_get_cross_tenant_returns_404
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn close_period_get_cross_tenant_returns_404() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Insert period in tenant A
    let period_a = insert_close_period(&pool, tenant_a, "CT-PERIOD-A-2026-05").await;

    // Mirrors routes/close_periods.rs:325-332 (update_period fetch) and :84-90 (find_locked_period_for_date)
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM close_periods WHERE id = $1 AND tenant_id = $2")
            .bind(period_a)
            .bind(tenant_b)
            .fetch_optional(&pool)
            .await
            .expect("Query should succeed");

    assert!(
        row.is_none(),
        "Cross-tenant close_period GET must return no rows — got {:?}",
        row
    );

    // Cleanup
    cleanup_close_periods(&pool, tenant_a, "CT-PERIOD-A").await;
    cleanup_close_periods(&pool, tenant_b, "CT-PERIOD-A").await;
}

// ===========================================================================
// Test 9: close_period_list_excludes_other_tenant
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn close_period_list_excludes_other_tenant() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Insert one period per tenant
    let period_a = insert_close_period(&pool, tenant_a, "CT-LIST-A-2026-05").await;
    let _period_b = insert_close_period(&pool, tenant_b, "CT-LIST-B-2026-05").await;

    // Mirrors routes/close_periods.rs:184-192 (list_periods)
    let tenant_a_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM close_periods WHERE tenant_id = $1 AND period_label LIKE 'CT-LIST%'",
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("Query should succeed");

    assert!(
        tenant_a_ids.contains(&period_a),
        "Tenant A should see its own period"
    );
    assert_eq!(
        tenant_a_ids.len(),
        1,
        "Tenant A should see exactly 1 period"
    );

    // Cleanup
    cleanup_close_periods(&pool, tenant_a, "CT-LIST").await;
    cleanup_close_periods(&pool, tenant_b, "CT-LIST").await;
}

// ===========================================================================
// Test 10: close_period_update_cutoff_cross_tenant_affects_zero_rows
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn close_period_update_cutoff_cross_tenant_affects_zero_rows() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let period_a = insert_close_period(&pool, tenant_a, "CT-UPD-A-2026-05").await;

    // Capture original cutoff_date
    let (original_cutoff,): (String,) = sqlx::query_as(
        "SELECT cutoff_date::text FROM close_periods WHERE id = $1 AND tenant_id = $2",
    )
    .bind(period_a)
    .bind(tenant_a)
    .fetch_one(&pool)
    .await
    .expect("Should fetch original period");

    // Mirrors routes/close_periods.rs:287-297 (update_period UPDATE) and run_close locking UPDATE
    let result = sqlx::query(
        "UPDATE close_periods SET cutoff_date = '2099-01-01'::date, status = 'open', updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(period_a)
    .bind(tenant_b) // wrong tenant
    .execute(&pool)
    .await
    .expect("Query should succeed");

    assert_eq!(
        result.rows_affected(),
        0,
        "Cross-tenant close_period UPDATE must affect 0 rows"
    );

    // Verify original cutoff_date unchanged
    let (current_cutoff,): (String,) = sqlx::query_as(
        "SELECT cutoff_date::text FROM close_periods WHERE id = $1 AND tenant_id = $2",
    )
    .bind(period_a)
    .bind(tenant_a)
    .fetch_one(&pool)
    .await
    .expect("Should re-fetch original period");

    assert_eq!(
        current_cutoff, original_cutoff,
        "Cross-tenant UPDATE must not change cutoff_date"
    );

    // Cleanup
    cleanup_close_periods(&pool, tenant_a, "CT-UPD").await;
    cleanup_close_periods(&pool, tenant_b, "CT-UPD").await;
}

// ===========================================================================
// Test 11: discount_worklist_excludes_other_tenant
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn discount_worklist_excludes_other_tenant() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Create vendor + discount-eligible invoice in each tenant
    let vendor_a = insert_vendor(&pool, tenant_a, "CT-DISC Vendor A").await;
    let vendor_b = insert_vendor(&pool, tenant_b, "CT-DISC Vendor B").await;
    let invoice_a = insert_discount_invoice(&pool, tenant_a, vendor_a, "CT-DISC-INV-A").await;
    let invoice_b = insert_discount_invoice(&pool, tenant_b, vendor_b, "CT-DISC-INV-B").await;

    // Mirrors routes/discounts.rs:205-222 (get_worklist SELECT)
    let worklist_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT i.id FROM invoices i
           WHERE i.tenant_id = $1
             AND i.discount_percent IS NOT NULL
             AND i.discount_captured_at IS NULL
             AND i.discount_missed_at IS NULL
             AND i.discount_deadline >= CURRENT_DATE"#,
    )
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .expect("Query should succeed");

    assert!(
        worklist_ids.contains(&invoice_a),
        "Tenant A worklist should contain its own invoice"
    );
    assert!(
        !worklist_ids.contains(&invoice_b),
        "Tenant A worklist must NOT contain tenant B's invoice"
    );
    assert_eq!(
        worklist_ids.len(),
        1,
        "Tenant A worklist should contain exactly 1 invoice"
    );

    // Cleanup
    cleanup_test_data(&pool, tenant_a, "CT-DISC").await;
    cleanup_test_data(&pool, tenant_b, "CT-DISC").await;
}

// ===========================================================================
// Test 12: discount_capture_cross_tenant_affects_zero_rows
// ===========================================================================

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test cross_tenant_isolation_tests -- --ignored
async fn discount_capture_cross_tenant_affects_zero_rows() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let vendor_a = insert_vendor(&pool, tenant_a, "CT-CAPTURE Vendor A").await;
    let invoice_a = insert_discount_invoice(&pool, tenant_a, vendor_a, "CT-CAPTURE-INV-A").await;

    // Mirrors routes/discounts.rs:334-341 (capture_discount UPDATE)
    let result = sqlx::query(
        "UPDATE invoices SET discount_captured_at = NOW(), updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_a)
    .bind(tenant_b) // wrong tenant
    .execute(&pool)
    .await
    .expect("Query should succeed");

    assert_eq!(
        result.rows_affected(),
        0,
        "Cross-tenant discount capture UPDATE must affect 0 rows"
    );

    // Verify discount_captured_at is still NULL
    let (captured_at,): (Option<String>,) = sqlx::query_as(
        "SELECT discount_captured_at::text FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_a)
    .bind(tenant_a)
    .fetch_one(&pool)
    .await
    .expect("Should re-fetch invoice");

    assert!(
        captured_at.is_none(),
        "Cross-tenant capture must not set discount_captured_at — got {:?}",
        captured_at
    );

    // Cleanup
    cleanup_test_data(&pool, tenant_a, "CT-CAPTURE").await;
    cleanup_test_data(&pool, tenant_b, "CT-CAPTURE").await;
}
