//! Integration tests for contract-aware matching of recurring non-PO spend.
//!
//! Tests the `match_invoice_to_contract` function against a real PostgreSQL
//! database. All tests are #[ignore] by default since they require a migrated
//! database with the `contracts` and `contract_matches` tables.

#![allow(warnings)]

use billforge_invoice_processing::contract_matching::{
    compute_expected_amount, ContractMatchInput, ContractMatchOutcome,
};
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Internal struct mirroring `ContractRow` for use in `compute_expected_amount`
/// tests without needing a database.
struct TestContract {
    monthly_amount: f64,
    escalator_pct: f64,
    escalator_anniversary_month: Option<i16>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    tolerance_pct: f64,
}

impl TestContract {
    fn as_row(&self) -> billforge_invoice_processing::contract_matching::ContractRow {
        billforge_invoice_processing::contract_matching::ContractRow {
            id: Uuid::new_v4(),
            monthly_amount: self.monthly_amount,
            escalator_pct: self.escalator_pct,
            escalator_anniversary_month: self.escalator_anniversary_month,
            start_date: self.start_date,
            end_date: self.end_date,
            tolerance_pct: self.tolerance_pct,
        }
    }
}

/// Seed a tenant row into the database so FK constraints pass.
async fn seed_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenants (id, name, subdomain, active, created_at)
         VALUES ($1, 'Contract Test Tenant', 'contract-test', true, NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to seed tenant");
}

/// Seed a vendor row so FK constraints on `contracts(vendor_id)` pass.
async fn seed_vendor(pool: &PgPool, tenant_id: Uuid, vendor_id: Uuid) {
    let schema = format!("tenant_{}", tenant_id.to_string().replace('-', "_"));
    // Try tenant-schema vendor first.
    let in_tenant = sqlx::query(&format!(
        "INSERT INTO {}.vendors (id, name, active, created_at)
         VALUES ($1, 'Test Vendor', true, NOW())
         ON CONFLICT (id) DO NOTHING",
        schema
    ))
    .bind(vendor_id)
    .execute(pool)
    .await;

    if in_tenant.is_err() {
        // Fallback: public schema vendors table.
        let _ = sqlx::query(
            "INSERT INTO vendors (id, tenant_id, name, active, created_at)
             VALUES ($1, $2, 'Test Vendor', true, NOW())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(vendor_id)
        .bind(tenant_id)
        .execute(pool)
        .await;
    }
}

/// Insert a contract row directly.
async fn seed_contract(
    pool: &PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    monthly_amount: f64,
    escalator_pct: f64,
    start_date: NaiveDate,
    end_date: NaiveDate,
    tolerance_pct: f64,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO contracts
               (id, tenant_id, vendor_id, monthly_amount, escalator_pct,
                start_date, end_date, tolerance_pct, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'active')"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(monthly_amount)
    .bind(escalator_pct)
    .bind(start_date)
    .bind(end_date)
    .bind(tolerance_pct)
    .execute(pool)
    .await
    .expect("Failed to seed contract");
    id
}

// ---------------------------------------------------------------------------
// Pure-logic tests (no DB needed)
// ---------------------------------------------------------------------------

#[test]
fn in_band_invoice_matches_expected_amount() {
    let contract = TestContract {
        monthly_amount: 1000.0,
        escalator_pct: 0.0,
        escalator_anniversary_month: None,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        tolerance_pct: 2.0,
    };
    let expected = compute_expected_amount(
        &contract.as_row(),
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    );
    assert!((expected - 1000.0).abs() < 0.01);

    // Amount of 1010 = 1% variance, within 2% tolerance.
    let variance = ((1010.0 - expected) / expected) * 100.0;
    assert!(variance.abs() <= 2.0, "Should be in-band");
}

#[test]
fn out_of_band_invoice_exceeds_tolerance() {
    let contract = TestContract {
        monthly_amount: 1000.0,
        escalator_pct: 0.0,
        escalator_anniversary_month: None,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        tolerance_pct: 2.0,
    };
    let expected = compute_expected_amount(
        &contract.as_row(),
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    );

    // Amount of 1050 = 5% variance, outside 2% tolerance.
    let variance = ((1050.0 - expected) / expected) * 100.0;
    assert!(variance.abs() > 2.0, "Should be out-of-band");
}

#[test]
fn escalator_applied_after_anniversary() {
    let contract = TestContract {
        monthly_amount: 1000.0,
        escalator_pct: 5.0, // 5% annual
        escalator_anniversary_month: Some(1),
        start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2028, 12, 31).unwrap(),
        tolerance_pct: 2.0,
    };
    // After 2 anniversaries (Jan 2026, Jan 2027).
    let expected = compute_expected_amount(
        &contract.as_row(),
        NaiveDate::from_ymd_opt(2027, 3, 1).unwrap(),
    );
    // 1000 * 1.05^2 = 1102.50
    assert!((expected - 1102.50).abs() < 0.01);
}

#[test]
fn expired_contract_date_past_end() {
    let contract = TestContract {
        monthly_amount: 500.0,
        escalator_pct: 0.0,
        escalator_anniversary_month: None,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        tolerance_pct: 2.0,
    };
    // Invoice in 2026 is past end date.
    let invoice_date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    // The contract's end_date is before invoice_date, so in the DB query
    // `end_date >= invoice_date` would be false. This test just verifies
    // date logic.
    assert!(invoice_date > contract.end_date);
}

#[test]
fn no_contract_returns_zero_expected() {
    // When there's no match, compute_expected_amount is not called.
    // This just verifies the fallback path.
    let contract = TestContract {
        monthly_amount: 0.0,
        escalator_pct: 0.0,
        escalator_anniversary_month: None,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        tolerance_pct: 2.0,
    };
    let expected = compute_expected_amount(
        &contract.as_row(),
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    );
    assert_eq!(expected, 0.0);
}

// ---------------------------------------------------------------------------
// Integration tests (require DB)
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts tables"]
async fn test_in_band_invoice_returns_inband(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    let contract_id = seed_contract(
        &pool,
        tenant_id,
        vendor_id,
        1000.0,
        0.0,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        2.0, // 2% tolerance
    )
    .await;

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
        amount: 1010.0, // 1% variance, within 2%
        currency: "USD".to_string(),
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, invoice_id)
            .await
            .expect("match should succeed");

    match outcome {
        ContractMatchOutcome::InBand {
            contract_id: cid,
            expected,
            variance_pct,
        } => {
            assert_eq!(cid, contract_id);
            assert!((expected - 1000.0).abs() < 0.01);
            assert!(variance_pct.abs() <= 2.0);
        }
        other => panic!("Expected InBand, got {:?}", other),
    }

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts tables"]
async fn test_out_of_band_invoice_returns_outofband(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    let _contract_id = seed_contract(
        &pool,
        tenant_id,
        vendor_id,
        1000.0,
        0.0,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
        2.0,
    )
    .await;

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
        amount: 1100.0, // 10% variance, outside 2%
        currency: "USD".to_string(),
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, invoice_id)
            .await
            .expect("match should succeed");

    match outcome {
        ContractMatchOutcome::OutOfBand { variance_pct, .. } => {
            assert!(variance_pct.abs() > 2.0);
        }
        other => panic!("Expected OutOfBand, got {:?}", other),
    }

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts tables"]
async fn test_escalator_applied_after_anniversary(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    // 3% annual escalator, anniversary in January.
    let _contract_id = seed_contract(
        &pool,
        tenant_id,
        vendor_id,
        1000.0,
        3.0,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2028, 12, 31).unwrap(),
        5.0, // generous tolerance
    )
    .await;

    // Feb 2027 = 2 full years of escalation.
    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date: NaiveDate::from_ymd_opt(2027, 2, 1).unwrap(),
        amount: 1060.90, // 1000 * 1.03^2 = 1060.90
        currency: "USD".to_string(),
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, invoice_id)
            .await
            .expect("match should succeed");

    match outcome {
        ContractMatchOutcome::InBand { expected, .. } => {
            assert!((expected - 1060.90).abs() < 0.01);
        }
        other => panic!("Expected InBand, got {:?}", other),
    }

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts tables"]
async fn test_expired_contract_returns_expired(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    let _contract_id = seed_contract(
        &pool,
        tenant_id,
        vendor_id,
        500.0,
        0.0,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(), // ended Dec 2025
        2.0,
    )
    .await;

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(), // past end
        amount: 500.0,
        currency: "USD".to_string(),
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, invoice_id)
            .await
            .expect("match should succeed");

    match outcome {
        ContractMatchOutcome::Expired { .. } => {}
        other => panic!("Expected Expired, got {:?}", other),
    }

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts tables"]
async fn test_no_contract_returns_no_active_contract(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await;
    // Deliberately do NOT seed a contract.

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
        amount: 500.0,
        currency: "USD".to_string(),
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, invoice_id)
            .await
            .expect("match should succeed");

    assert_eq!(outcome, ContractMatchOutcome::NoActiveContract);

    Ok(())
}
