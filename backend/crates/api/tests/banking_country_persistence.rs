//! Regression tests for #394: country_mismatch fraud signal on banking
//! dual-approval verification.
//!
//! Before the fix, `verify_banking` passed `None` for `bank_country` because the
//! column was never persisted on the vendor row, which forced
//! `check_country_mismatch` to return `RiskLevel::Unknown` and structurally
//! disabled the most predictive BEC indicator on the exact dual-approval flow
//! it was designed for. These tests validate the round trip: `update_banking`
//! persists `bank_country`, `verify_banking` reads it back, and
//! `check_country_mismatch` can actually elevate risk on the verification flow.
//!
//! Requires DATABASE_URL — run with:
//!   cargo test --test banking_country_persistence -- --ignored

use billforge_api::fraud_guard;
use billforge_core::domain::VendorId;
use billforge_core::TenantId;
use billforge_core::VendorRepository;
use std::sync::Arc;
use uuid::Uuid;

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
    .bind("banking-country-test@example.com")
    .bind("hash_not_used")
    .bind("Banking Country Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Insert a vendor with an address.country (matches the field path
/// `verify_banking` reads via `vendor.address.country`).
async fn insert_vendor_with_country(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_country: &str,
) -> Uuid {
    let vendor_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, vendor_type, address)
           VALUES ($1, $2, 'Test Vendor Country Persistence', 'business',
                   jsonb_build_object('line1', '1 Main St', 'city', 'Anytown',
                                      'postal_code', '00000', 'country', $3))"#,
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_country)
    .execute(pool)
    .await
    .expect("insert vendor with country");
    vendor_id
}

/// Simulate what `update_banking` does end to end: record the banking change
/// through the repo, then persist `bank_country` via the same UPDATE the
/// route handler issues.
async fn simulate_update_banking(
    pool: &Arc<sqlx::PgPool>,
    tenant_id: &TenantId,
    vendor_id_obj: &VendorId,
    user_id: Uuid,
    bank_country: &str,
) {
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    repo.record_banking_change(
        tenant_id,
        vendor_id_obj,
        None,
        "4242",
        "International Bank",
        "checking",
        "enc:1111",
        "enc:routing1",
        user_id,
    )
    .await
    .expect("record banking change");

    sqlx::query(
        "UPDATE vendors SET bank_country = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id_obj.0)
    .bind(*tenant_id.as_uuid())
    .bind(bank_country)
    .execute(&**pool)
    .await
    .expect("persist bank_country");
}

// ============================================================================
// Test A: mismatched countries elevate country_mismatch to High and feed
//         into the aggregate on the dual-approval verification flow.
// ============================================================================

#[sqlx::test]
#[ignore]
async fn verify_banking_country_mismatch_elevates_risk_to_high(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    // Vendor registered in US, bank submitted in DE.
    let vendor_id = insert_vendor_with_country(&pool, &tenant_id, "US").await;
    let vendor_id_obj = VendorId(vendor_id);

    simulate_update_banking(&pool, &tenant_id, &vendor_id_obj, user_id, "DE").await;

    // After persistence, reading the vendor back must surface the country
    // through `vendor.bank_account.country` — this is the exact path
    // `verify_banking` uses to build the bank_country argument.
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");

    let bank_account = vendor
        .bank_account
        .as_ref()
        .expect("bank_account populated");
    assert_eq!(
        bank_account.country.as_deref(),
        Some("DE"),
        "bank_country must round-trip through persistence"
    );

    // Mirror the verify_banking call: vendor.address.country vs the persisted
    // bank country. With mismatched countries the signal must be High and the
    // aggregate must reflect that elevation.
    let vendor_country = vendor.address.as_ref().map(|a| a.country.as_str());
    let bank_country = bank_account.country.as_deref();

    let signals = fraud_guard::run_fraud_guard(
        &tenant_id,
        Some(&vendor_id_obj),
        &vendor.name,
        "",
        vendor_country,
        bank_country,
        &pool,
    )
    .await;

    assert_eq!(
        signals.country_mismatch.risk,
        fraud_guard::RiskLevel::High,
        "country_mismatch must be High when persisted bank_country differs from vendor country"
    );
    assert_eq!(
        signals.country_mismatch.vendor_country.as_deref(),
        Some("US")
    );
    assert_eq!(signals.country_mismatch.bank_country.as_deref(), Some("DE"));
    assert_eq!(
        signals.overall_risk,
        fraud_guard::RiskLevel::High,
        "aggregate overall_risk must elevate to High on country mismatch"
    );
}

// ============================================================================
// Test B: matching countries on the verification flow surface as Low and do
//         not force-block via the country_mismatch signal.
// ============================================================================

#[sqlx::test]
#[ignore]
async fn verify_banking_matching_countries_resolve_to_low(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;

    let vendor_id = insert_vendor_with_country(&pool, &tenant_id, "US").await;
    let vendor_id_obj = VendorId(vendor_id);

    simulate_update_banking(&pool, &tenant_id, &vendor_id_obj, user_id, "US").await;

    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");

    let bank_account = vendor.bank_account.as_ref().expect("bank_account populated");
    let bank_country = bank_account.country.as_deref();
    let vendor_country = vendor.address.as_ref().map(|a| a.country.as_str());

    let signals = fraud_guard::run_fraud_guard(
        &tenant_id,
        Some(&vendor_id_obj),
        &vendor.name,
        "",
        vendor_country,
        bank_country,
        &pool,
    )
    .await;

    assert_eq!(
        signals.country_mismatch.risk,
        fraud_guard::RiskLevel::Low,
        "country_mismatch must be Low when vendor country matches persisted bank country"
    );
    assert_ne!(
        signals.country_mismatch.risk,
        fraud_guard::RiskLevel::Unknown,
        "country_mismatch must not collapse to Unknown when bank_country is persisted"
    );
}
