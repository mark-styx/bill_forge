//! Integration tests for InvoiceRiskScorer (refs #420).
//!
//! Validates the unified background risk scorer that combines duplicate +
//! fraud + amount-spike signals per ingested invoice and routes high-tier
//! verdicts to the exception work queue.
//!
//! Requires DATABASE_URL — run with:
//!   cargo test --test invoice_risk_scoring_tests -- --ignored

#![cfg(feature = "analytics")]

use billforge_api::services::invoice_risk_scoring::{InvoiceRiskScorer, RiskTier};
use billforge_core::{
    domain::{InvoiceId, VendorId},
    types::TenantId,
};
use billforge_db::repositories::VendorRepositoryImpl;
use chrono::Utc;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("risk-test-{}@example.com", user_id))
    .bind("hash_not_used")
    .bind("Risk Test User")
    .execute(pool)
    .await
    .expect("insert test user");
    user_id
}

async fn insert_vendor(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    name: &str,
    email: Option<&str>,
) -> Uuid {
    let vendor_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, vendor_type, email, status)
           VALUES ($1, $2, $3, 'business', $4, 'active')
           ON CONFLICT DO NOTHING"#,
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(email.unwrap_or("vendor@example.com"))
    .execute(pool)
    .await
    .expect("insert test vendor");
    vendor_id
}

#[allow(clippy::too_many_arguments)]
async fn insert_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user_id: Uuid,
    vendor_id: Option<Uuid>,
    vendor_name: &str,
    invoice_number: &str,
    total_cents: i64,
    invoice_date: Option<chrono::NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices
            (id, tenant_id, vendor_id, vendor_name, invoice_number, total_amount_cents,
             currency, invoice_date, line_items, capture_status, processing_status,
             document_id, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, 'USD', $7, '[]'::jsonb,
                   'ready_for_review', 'submitted', $8, $9)"#,
    )
    .bind(id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .bind(vendor_name)
    .bind(invoice_number)
    .bind(total_cents)
    .bind(invoice_date)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert invoice");
    id
}

async fn insert_exception_queue(pool: &sqlx::PgPool, tenant_id: &TenantId) -> Uuid {
    let queue_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO work_queues
            (id, tenant_id, name, description, queue_type, is_default, is_active,
             settings, created_at, updated_at)
           VALUES ($1, $2, 'Exception Queue', 'Risk-flagged invoices',
                   'exception', false, true, '{}'::jsonb, NOW(), NOW())
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(queue_id)
    .bind(*tenant_id.as_uuid())
    .execute(pool)
    .await
    .expect("insert exception queue");
    queue_id
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn scores_exact_duplicate_high(pool: sqlx::PgPool) {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    let user_id = insert_user(&pool, &tenant_id).await;

    let vendor_id = insert_vendor(&pool, &tenant_id, "Acme Corporation", None).await;
    let today = Some(Utc::now().date_naive());

    // First invoice (the "existing" one).
    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Acme Corporation",
        "ACME-9001",
        150_000,
        today,
    )
    .await;

    // Second invoice — same vendor / amount / date, OCR-confusable number
    // (O <-> 0). The unique invoice_number constraint prevents an exact
    // string collision; OCR-aware Levenshtein still scores the pair as a
    // near-perfect duplicate so the verdict tier escalates to Block.
    let new_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Acme Corporation",
        "ACME-9OO1",
        150_000,
        today,
    )
    .await;

    let scorer = InvoiceRiskScorer::new();
    let verdict = scorer
        .score_invoice(&tenant_id, &InvoiceId(new_id), &pool)
        .await
        .expect("score invoice");

    assert_eq!(
        verdict.tier,
        RiskTier::Block,
        "exact duplicate must escalate to Block, got {:?} score={}",
        verdict.tier,
        verdict.score
    );
    assert!(
        !verdict.duplicate_signals.is_empty(),
        "should surface a duplicate signal"
    );
}

#[sqlx::test]
#[ignore]
async fn scores_clean_invoice_low(pool: sqlx::PgPool) {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    let user_id = insert_user(&pool, &tenant_id).await;

    // Seed an old domain so domain_age is Low, not High.
    sqlx::query(
        r#"INSERT INTO vendor_domain_first_seen (tenant_id, domain, first_seen_at)
           VALUES ($1, 'clean-vendor.example.com', NOW() - INTERVAL '400 days')
           ON CONFLICT (tenant_id, domain) DO NOTHING"#,
    )
    .bind(*tenant_id.as_uuid())
    .execute(&pool)
    .await
    .expect("seed old domain");

    let vendor_id = insert_vendor(
        &pool,
        &tenant_id,
        "Pristine Goods Co",
        Some("ap@clean-vendor.example.com"),
    )
    .await;
    let today = Some(Utc::now().date_naive());

    let invoice_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Pristine Goods Co",
        "PG-0001",
        12_500,
        today,
    )
    .await;

    let scorer = InvoiceRiskScorer::new();
    let verdict = scorer
        .score_invoice(&tenant_id, &InvoiceId(invoice_id), &pool)
        .await
        .expect("score invoice");

    assert_eq!(
        verdict.tier,
        RiskTier::Clear,
        "clean invoice must score Clear, got {:?} score={} signals={:?}",
        verdict.tier,
        verdict.score,
        verdict.fraud_signals
    );
    assert!(verdict.duplicate_signals.is_empty());
}

#[sqlx::test]
#[ignore]
async fn routes_to_exception_queue(pool: sqlx::PgPool) {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    let user_id = insert_user(&pool, &tenant_id).await;
    insert_exception_queue(&pool, &tenant_id).await;

    let vendor_id = insert_vendor(&pool, &tenant_id, "Acme Corporation", None).await;
    let today = Some(Utc::now().date_naive());

    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Acme Corporation",
        "ACME-DUP-1",
        500_000,
        today,
    )
    .await;
    let new_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Acme Corporation",
        "ACME-DUP-I",
        500_000,
        today,
    )
    .await;

    let scorer = InvoiceRiskScorer::new();
    let pool_arc = Arc::new(pool.clone());
    let verdict = scorer
        .score_and_route(&tenant_id, &InvoiceId(new_id), pool_arc)
        .await
        .expect("score_and_route");

    assert_eq!(verdict.tier, RiskTier::Block);

    // Verdict row persisted with the evidence JSON.
    let verdict_row = sqlx::query(
        "SELECT score, tier, evidence FROM invoice_risk_verdicts \
         WHERE invoice_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(new_id)
    .fetch_one(&pool)
    .await
    .expect("load verdict row");
    let tier_db: String = verdict_row.get("tier");
    assert_eq!(tier_db, "block");
    let evidence: serde_json::Value = verdict_row.get("evidence");
    assert!(evidence.get("duplicate_signals").is_some());

    // Queue item created in the exception queue.
    let queue_item_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM queue_items qi
            JOIN work_queues wq ON wq.id = qi.queue_id
           WHERE qi.invoice_id = $1
             AND wq.queue_type = 'exception'"#,
    )
    .bind(new_id)
    .fetch_one(&pool)
    .await
    .expect("count exception queue items");
    assert_eq!(
        queue_item_count, 1,
        "high-tier verdict should produce exactly one exception queue item"
    );
}

#[sqlx::test]
#[ignore]
async fn bank_change_signal_surfaces(pool: sqlx::PgPool) {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    let user_id = insert_user(&pool, &tenant_id).await;

    let vendor_id = insert_vendor(&pool, &tenant_id, "Bank Flip Co", None).await;
    let vendor_id_obj = VendorId(vendor_id);
    let repo = VendorRepositoryImpl::new(Arc::new(pool.clone()));

    // Two banking changes within 30 days -> bank_change should fire High.
    repo.record_banking_change(
        &tenant_id,
        &vendor_id_obj,
        None,
        "1111",
        "First Bank",
        "checking",
        "enc:aaaa",
        "enc:routing-a",
        user_id,
    )
    .await
    .expect("first banking change");
    repo.record_banking_change(
        &tenant_id,
        &vendor_id_obj,
        Some("1111"),
        "2222",
        "Second Bank",
        "checking",
        "enc:bbbb",
        "enc:routing-b",
        user_id,
    )
    .await
    .expect("second banking change");

    let invoice_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Bank Flip Co",
        "BF-001",
        25_000,
        Some(Utc::now().date_naive()),
    )
    .await;

    let scorer = InvoiceRiskScorer::new();
    let verdict = scorer
        .score_invoice(&tenant_id, &InvoiceId(invoice_id), &pool)
        .await
        .expect("score invoice");

    let bank_signal = verdict
        .fraud_signals
        .iter()
        .find(|s| s.kind == "bank_change")
        .expect("bank_change signal present");
    assert_eq!(
        format!("{:?}", bank_signal.risk).to_lowercase(),
        "high",
        "bank_change risk should be High after two changes in 30d, got {:?}",
        bank_signal.risk
    );
}

#[sqlx::test]
#[ignore]
async fn audit_log_written(pool: sqlx::PgPool) {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    let user_id = insert_user(&pool, &tenant_id).await;
    insert_exception_queue(&pool, &tenant_id).await;

    let vendor_id = insert_vendor(&pool, &tenant_id, "Audit Vendor", None).await;
    let today = Some(Utc::now().date_naive());

    insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Audit Vendor",
        "AUD-1",
        80_000,
        today,
    )
    .await;
    let new_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Audit Vendor",
        "AUD-I",
        80_000,
        today,
    )
    .await;

    let scorer = InvoiceRiskScorer::new();
    let pool_arc = Arc::new(pool.clone());
    let _ = scorer
        .score_and_route(&tenant_id, &InvoiceId(new_id), pool_arc)
        .await
        .expect("score_and_route");

    // The audit entry uses action 'update' against ResourceType::Invoice with
    // the verdict kind in metadata. Confirm at least one row was written for
    // this invoice resource_id with the expected metadata kind.
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM audit_log
           WHERE resource_type = 'invoice'
             AND resource_id = $1
             AND tenant_id = $2
             AND changes->'metadata'->>'kind' = 'invoice.risk_scored'"#,
    )
    .bind(new_id.to_string())
    .bind(*tenant_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("count audit rows");
    assert!(
        count >= 1,
        "expected at least one invoice.risk_scored audit entry, got {}",
        count
    );
}

#[sqlx::test]
#[ignore]
async fn cross_tenant_fake_invoice_pattern_blocks_invoice(pool: sqlx::PgPool) {
    // Verifies the federated cross-tenant duplicate signal called out in #420:
    // when N>=5 other tenants in the federated risk network have flagged this
    // vendor's hash bucket with `fake_invoice_pattern`, the scorer must
    // surface a CrossTenantSignal and escalate the verdict to Block - even
    // when the local 5-signal fuzzy match returns nothing.
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;

    // Bring up the federated network schema (migration 141) on the same pool
    // we use as the metadata DB for the scorer below.
    sqlx::raw_sql(include_str!(
        "../../../migrations/141_federated_vendor_risk_network.up.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply federated network migration");

    let user_id = insert_user(&pool, &tenant_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id, "Federated Flagged Co", None).await;

    // Compute the canonical vendor_hash using the same rule the scorer uses
    // internally. We hard-code the inputs here so the test asserts the
    // contract (canonical tuple + SHA-256 + salt) rather than re-reading
    // private helpers.
    let salt = "invoice-risk-scorer-test-salt";
    let normalized = "federated flagged co";
    let vendor_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(b"|");
        hasher.update(normalized.as_bytes());
        hasher.update(b"||"); // empty tax_id + empty bank fingerprint
        hex::encode(hasher.finalize())
    };

    // Seed 5 distinct contributing tenants each reporting a
    // `fake_invoice_pattern` signal so the k-anonymity floor of 5 is met.
    for i in 0..5 {
        sqlx::query(
            r#"INSERT INTO federated_vendor_signals
                 (vendor_hash, signal_type, contributing_tenant_hash, signal_weight)
               VALUES ($1, 'fake_invoice_pattern', $2, 1.0)"#,
        )
        .bind(&vendor_hash)
        .bind(format!("test-tenant-hmac-{:064}", i))
        .execute(&pool)
        .await
        .expect("insert federated signal");
    }

    let invoice_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Federated Flagged Co",
        "FFC-0001",
        50_000,
        Some(Utc::now().date_naive()),
    )
    .await;

    let scorer = InvoiceRiskScorer::new()
        .with_federated_network(Arc::new(pool.clone()), Some(salt.to_string()));
    let verdict = scorer
        .score_invoice(&tenant_id, &InvoiceId(invoice_id), &pool)
        .await
        .expect("score invoice");

    assert!(
        !verdict.cross_tenant_signals.is_empty(),
        "expected at least one federated cross-tenant signal, got {:?}",
        verdict.cross_tenant_signals,
    );
    let fake = verdict
        .cross_tenant_signals
        .iter()
        .find(|s| s.signal_type == "fake_invoice_pattern")
        .expect("fake_invoice_pattern signal present");
    assert!(
        fake.contributor_count >= 5,
        "contributor count below k-anonymity floor: {}",
        fake.contributor_count
    );
    assert_eq!(
        verdict.tier,
        RiskTier::Block,
        "federated fake_invoice_pattern hit must escalate to Block (tier={:?} score={})",
        verdict.tier,
        verdict.score
    );
}

#[sqlx::test]
#[ignore]
async fn cross_tenant_lookup_below_floor_does_not_flag(pool: sqlx::PgPool) {
    // Companion to the test above: a single contributor (below k=5) must NOT
    // produce a cross-tenant signal, confirming the k-anonymity floor is
    // enforced at the read path. Otherwise a tenant with one neighbor could
    // de-anonymize that neighbor's flag.
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;
    sqlx::raw_sql(include_str!(
        "../../../migrations/141_federated_vendor_risk_network.up.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply federated network migration");

    let user_id = insert_user(&pool, &tenant_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id, "Single Reporter Co", None).await;

    let salt = "invoice-risk-scorer-test-salt-floor";
    let normalized = "single reporter co";
    let vendor_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(b"|");
        hasher.update(normalized.as_bytes());
        hasher.update(b"||");
        hex::encode(hasher.finalize())
    };

    // Only one contributor — below the k=5 floor.
    sqlx::query(
        r#"INSERT INTO federated_vendor_signals
             (vendor_hash, signal_type, contributing_tenant_hash, signal_weight)
           VALUES ($1, 'fake_invoice_pattern', $2, 1.0)"#,
    )
    .bind(&vendor_hash)
    .bind("solo-contributor-hmac")
    .execute(&pool)
    .await
    .expect("insert federated signal");

    let invoice_id = insert_invoice(
        &pool,
        &tenant_id,
        user_id,
        Some(vendor_id),
        "Single Reporter Co",
        "SRC-0001",
        50_000,
        Some(Utc::now().date_naive()),
    )
    .await;

    let scorer = InvoiceRiskScorer::new()
        .with_federated_network(Arc::new(pool.clone()), Some(salt.to_string()));
    let verdict = scorer
        .score_invoice(&tenant_id, &InvoiceId(invoice_id), &pool)
        .await
        .expect("score invoice");

    assert!(
        verdict.cross_tenant_signals.is_empty(),
        "k-anonymity floor must hide cross-tenant signal at contributor_count<5"
    );
}

#[sqlx::test]
#[ignore]
async fn rls_policy_is_installed_on_invoice_risk_verdicts(pool: sqlx::PgPool) {
    // Test pools run as a privileged role that BYPASSRLS, so the policy
    // cannot be exercised against real rows here (see RLS notes in
    // rls_tenant_db_tables.rs). Instead, this test asserts the migration
    // wired the table up correctly: ENABLE+FORCE RLS plus a tenant-isolation
    // policy keyed on app.current_tenant_id. Production runs as the
    // billforge_app NOSUPERUSER NOBYPASSRLS role; the policy below is what
    // actually gates cross-tenant reads there.
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    setup_schema(&pool, &tenant_id).await;

    let rls_enabled: (bool, bool) = sqlx::query_as(
        r#"SELECT relrowsecurity, relforcerowsecurity
             FROM pg_class WHERE relname = 'invoice_risk_verdicts'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("read pg_class flags");
    assert!(
        rls_enabled.0,
        "invoice_risk_verdicts must have ROW LEVEL SECURITY enabled"
    );
    assert!(
        rls_enabled.1,
        "invoice_risk_verdicts must FORCE RLS (so table owners are not exempt)"
    );

    let policy: (String, String) = sqlx::query_as(
        r#"SELECT polname, pg_get_expr(polqual, polrelid)
             FROM pg_policy
             JOIN pg_class ON pg_class.oid = pg_policy.polrelid
            WHERE relname = 'invoice_risk_verdicts'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("read pg_policy");
    let qual = policy.1;
    assert!(
        qual.contains("app.current_tenant_id"),
        "tenant-isolation policy must key on app.current_tenant_id (got: {})",
        qual
    );
}
