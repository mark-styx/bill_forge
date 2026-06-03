//! Integration tests for fraud-guard signals (#314).
//!
//! Validates the four fraud checks (domain age, lookalike vendor name,
//! recent bank change, country mismatch) by calling the fraud-guard
//! functions directly against a real Postgres tenant schema.
//!
//! These tests require DATABASE_URL — run with:
//!   cargo test --test fraud_guard -- --ignored

use billforge_core::domain::VendorId;
use billforge_core::types::TenantId;
use billforge_db::repositories::VendorRepositoryImpl;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("fraud-test@example.com")
    .bind("hash_not_used")
    .bind("Fraud Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

async fn insert_vendor(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    name: &str,
    email: Option<&str>,
    country: Option<&str>,
) -> Uuid {
    let vendor_id = Uuid::new_v4();
    let email_val = email.unwrap_or("vendor@example.com");
    let country_val = country.unwrap_or("US");
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, vendor_type, email)
           VALUES ($1, $2, $3, 'business', $4)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(name)
    .bind(email_val)
    .execute(pool)
    .await
    .expect("insert test vendor");

    // Set address country if provided
    if country.is_some() {
        sqlx::query(
            r#"UPDATE vendors SET address = jsonb_build_object(
                'line1', '123 Main St', 'city', 'Anytown', 'postal_code', '12345', 'country', $3
               ) WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(vendor_id)
        .bind(*tenant_id.as_uuid())
        .bind(country_val)
        .execute(pool)
        .await
        .expect("set vendor country");
    }

    vendor_id
}

async fn insert_domain_first_seen(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    domain: &str,
    first_seen: &chrono::DateTime<chrono::Utc>,
) {
    sqlx::query(
        r#"INSERT INTO vendor_domain_first_seen (tenant_id, domain, first_seen_at)
           VALUES ($1, $2, $3)
           ON CONFLICT (tenant_id, domain) DO NOTHING"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(domain)
    .bind(first_seen)
    .execute(pool)
    .await
    .expect("insert domain first seen");
}

// ============================================================================
// Test 1: Lookalike vendor detection
// ============================================================================

#[sqlx::test]
#[ignore]
async fn lookalike_vendor_flagged_on_create(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    // Insert an existing vendor "Acme Corp"
    let existing_id = insert_vendor(&pool, &tenant_id, "Acme Corp", Some("acme@example.com"), None).await;

    // Make sure it's active
    sqlx::query("UPDATE vendors SET status = 'active' WHERE id = $1")
        .bind(existing_id)
        .execute(&*pool)
        .await
        .expect("activate vendor");

    // Run lookalike check for "Acme C0rp" (OCR confusable: O -> 0)
    let signals = billforge_api::fraud_guard::run_fraud_guard(
        &tenant_id,
        None,
        "Acme C0rp",
        "newcorp.example.com",
        None,
        None,
        &pool,
    )
    .await;

    assert_eq!(
        signals.lookalike.risk,
        billforge_api::fraud_guard::RiskLevel::High,
        "lookalike should be high for Acme C0rp vs Acme Corp"
    );
    assert!(
        signals.lookalike.top_match.is_some(),
        "should have a top match"
    );
    let m = signals.lookalike.top_match.unwrap();
    assert!(m.similarity >= 0.85, "similarity should be >= 0.85, got {}", m.similarity);
    assert_eq!(m.vendor_name, "Acme Corp");

    // Overall risk should be high because lookalike is high
    assert_eq!(signals.overall_risk, billforge_api::fraud_guard::RiskLevel::High);
}

// ============================================================================
// Test 2: Country mismatch
// ============================================================================

#[sqlx::test]
#[ignore]
async fn country_mismatch_flagged_as_high_risk(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    setup_schema(&pool, &tenant_id).await;

    // Direct unit test of check_country_mismatch
    let signal = billforge_api::fraud_guard::check_country_mismatch(Some("US"), Some("NG"));
    assert_eq!(signal.risk, billforge_api::fraud_guard::RiskLevel::High);
    assert_eq!(signal.vendor_country.as_deref(), Some("US"));
    assert_eq!(signal.bank_country.as_deref(), Some("NG"));

    // Matching countries should be low
    let signal_match = billforge_api::fraud_guard::check_country_mismatch(Some("US"), Some("us"));
    assert_eq!(signal_match.risk, billforge_api::fraud_guard::RiskLevel::Low);

    // Missing data should be unknown
    let signal_missing = billforge_api::fraud_guard::check_country_mismatch(None, Some("US"));
    assert_eq!(signal_missing.risk, billforge_api::fraud_guard::RiskLevel::Unknown);
}

// ============================================================================
// Test 3: Recent bank change detection
// ============================================================================

#[sqlx::test]
#[ignore]
async fn recent_bank_change_flagged_as_high_risk(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    let vendor_id =
        insert_vendor(&pool, &tenant_id, "Test Vendor Bank Change", None, None).await;

    let vendor_id_obj = VendorId(vendor_id);
    let repo = VendorRepositoryImpl::new(pool.clone());

    // First banking change
    repo.record_banking_change(
        &tenant_id,
        &vendor_id_obj,
        None,
        "1234",
        "First Bank",
        "checking",
        "enc:1111",
        "enc:routing1",
        user_id,
    )
    .await
    .expect("first banking change");

    // Second banking change within 30 days
    repo.record_banking_change(
        &tenant_id,
        &vendor_id_obj,
        Some("1234"),
        "5678",
        "Second Bank",
        "checking",
        "enc:2222",
        "enc:routing2",
        user_id,
    )
    .await
    .expect("second banking change");

    // Run fraud guard - should detect recent bank change
    let signals = billforge_api::fraud_guard::run_fraud_guard(
        &tenant_id,
        Some(&vendor_id_obj),
        "Test Vendor Bank Change",
        "example.com",
        None,
        None,
        &pool,
    )
    .await;

    assert_eq!(
        signals.bank_change.risk,
        billforge_api::fraud_guard::RiskLevel::High,
        "bank_change should be high when there's a recent prior change"
    );
    assert!(
        signals.bank_change.recent_changes > 1,
        "recent_changes should be > 1 (two changes within 30 days), got {}",
        signals.bank_change.recent_changes
    );
}

// ============================================================================
// Test 3b: First-ever bank change should NOT be flagged (regression guard)
//          Refs previous false-positive where a single change was flagged
//          because the threshold was > 0 instead of > 1.
// ============================================================================

#[sqlx::test]
#[ignore]
async fn first_bank_change_is_not_flagged(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    let vendor_id =
        insert_vendor(&pool, &tenant_id, "Test Vendor First Change", None, None).await;

    let vendor_id_obj = VendorId(vendor_id);
    let repo = VendorRepositoryImpl::new(pool.clone());

    // Single banking change — should NOT flag as high risk
    repo.record_banking_change(
        &tenant_id,
        &vendor_id_obj,
        None,
        "1234",
        "First Bank",
        "checking",
        "enc:1111",
        "enc:routing1",
        user_id,
    )
    .await
    .expect("first banking change");

    let signals = billforge_api::fraud_guard::run_fraud_guard(
        &tenant_id,
        Some(&vendor_id_obj),
        "Test Vendor First Change",
        "example.com",
        None,
        None,
        &pool,
    )
    .await;

    assert_eq!(
        signals.bank_change.risk,
        billforge_api::fraud_guard::RiskLevel::Low,
        "first-ever bank change should NOT be flagged as high risk (got {} recent_changes)",
        signals.bank_change.recent_changes
    );
    assert_eq!(
        signals.bank_change.recent_changes, 1,
        "should see exactly 1 recent change (the one we just made)"
    );
}

// ============================================================================
// Test 4: Domain age - brand-new domain
// ============================================================================

#[sqlx::test]
#[ignore]
async fn brand_new_domain_flagged_as_high_risk(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    setup_schema(&pool, &tenant_id).await;

    // No domain record exists — should be flagged as high (brand new)
    let signal =
        billforge_api::fraud_guard::check_domain_age(&tenant_id, "brand-new-domain.com", &pool).await;

    assert_eq!(signal.risk, billforge_api::fraud_guard::RiskLevel::High);
    assert_eq!(signal.domain, "brand-new-domain.com");
    assert!(signal.first_seen_at.is_none());
}

#[sqlx::test]
#[ignore]
async fn old_domain_flagged_as_low_risk(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    setup_schema(&pool, &tenant_id).await;

    // Insert a domain that was first seen 365 days ago
    let old_date = chrono::Utc::now() - chrono::Duration::days(365);
    insert_domain_first_seen(&pool, &tenant_id, "old-domain.com", &old_date).await;

    let signal =
        billforge_api::fraud_guard::check_domain_age(&tenant_id, "old-domain.com", &pool).await;

    assert_eq!(signal.risk, billforge_api::fraud_guard::RiskLevel::Low);
    assert!(signal.first_seen_at.is_some());
    assert!(signal.days_since_first_seen.unwrap() >= 365);
}

#[sqlx::test]
#[ignore]
async fn medium_age_domain_flagged_as_medium_risk(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());

    setup_schema(&pool, &tenant_id).await;

    // Insert a domain first seen 90 days ago
    let med_date = chrono::Utc::now() - chrono::Duration::days(90);
    insert_domain_first_seen(&pool, &tenant_id, "medium-domain.com", &med_date).await;

    let signal =
        billforge_api::fraud_guard::check_domain_age(&tenant_id, "medium-domain.com", &pool).await;

    assert_eq!(signal.risk, billforge_api::fraud_guard::RiskLevel::Medium);
}

// ============================================================================
// Test 5: extract_domain helper
// ============================================================================

#[test]
fn extract_domain_from_email() {
    assert_eq!(
        billforge_api::fraud_guard::extract_domain(Some("user@example.com"), None),
        "example.com"
    );
}

#[test]
fn extract_domain_from_website() {
    assert_eq!(
        billforge_api::fraud_guard::extract_domain(None, Some("https://www.example.com/page")),
        "example.com"
    );
}

#[test]
fn extract_domain_prefers_email() {
    assert_eq!(
        billforge_api::fraud_guard::extract_domain(
            Some("user@email-domain.com"),
            Some("https://web-domain.com")
        ),
        "email-domain.com"
    );
}

#[test]
fn extract_domain_empty_when_none() {
    assert_eq!(billforge_api::fraud_guard::extract_domain(None, None), "");
}

// ============================================================================
// Test 6: build_screening_results merges legacy + fraud keys
// ============================================================================

#[test]
fn build_screening_results_includes_all_keys() {
    let signals = billforge_api::fraud_guard::FraudSignals {
        domain_age: billforge_api::fraud_guard::DomainAgeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            domain: "test.com".to_string(),
            first_seen_at: None,
            days_since_first_seen: Some(100),
        },
        lookalike: billforge_api::fraud_guard::LookalikeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            top_match: None,
        },
        bank_change: billforge_api::fraud_guard::BankChangeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            recent_changes: 0,
        },
        country_mismatch: billforge_api::fraud_guard::CountrySignal {
            risk: billforge_api::fraud_guard::RiskLevel::Unknown,
            vendor_country: None,
            bank_country: None,
        },
        overall_risk: billforge_api::fraud_guard::RiskLevel::Low,
    };

    let json = billforge_api::fraud_guard::build_screening_results(&signals);

    // Legacy keys
    assert!(json.get("ofac").is_some(), "should have ofac key");
    assert!(json.get("avs").is_some(), "should have avs key");
    assert!(json.get("plaid").is_some(), "should have plaid key");

    // Fraud-guard keys
    assert!(json.get("domain_age").is_some(), "should have domain_age key");
    assert!(json.get("lookalike").is_some(), "should have lookalike key");
    assert!(json.get("bank_change").is_some(), "should have bank_change key");
    assert!(
        json.get("country_mismatch").is_some(),
        "should have country_mismatch key"
    );
    assert!(json.get("overall_risk").is_some(), "should have overall_risk key");
}
