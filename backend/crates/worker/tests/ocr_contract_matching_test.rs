//! Integration tests for contract matching inside the OCR straight-through path (#331).
//!
//! Tests the `apply_contract_match_outcome` helper and the combined flow of
//! `match_invoice_to_contract` + `apply_contract_match_outcome` against a real
//! PostgreSQL database. All tests are #[ignore] by default since they require
//! a migrated database with the `contracts` and `contract_matches` tables.

#![allow(warnings)]

use std::sync::Arc;

use billforge_core::{
    domain::{InvoiceId, ProcessingStatus},
    types::TenantId,
};
use billforge_invoice_processing::{ContractMatchInput, ContractMatchOutcome, match_invoice_to_contract};
use billforge_worker::jobs::ocr_processing::apply_contract_match_outcome;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Seed a tenant row so FK constraints pass.
async fn seed_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenants (id, name, subdomain, active, created_at)
         VALUES ($1, 'Contract STP Test Tenant', 'contract-stp-test', true, NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to seed tenant");
}

/// Seed a vendor row so FK constraints pass.
async fn seed_vendor(pool: &PgPool, tenant_id: Uuid, vendor_id: Uuid) {
    let schema = format!("tenant_{}", tenant_id.to_string().replace('-', "_"));
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

/// Seed an invoice row directly. `total_cents` is in cents (e.g. 100000 = $1000).
/// Sets `po_number = NULL` and `vendor_id = Some(..)` by default for non-PO test path.
async fn seed_invoice(
    pool: &PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    total_cents: i64,
    invoice_date: Option<NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices
               (id, tenant_id, vendor_id, vendor_name, invoice_number,
                invoice_date, total_amount_cents, currency,
                capture_status, processing_status,
                line_items, supporting_documents, tags, custom_fields,
                document_id, created_at, updated_at)
           VALUES ($1, $2, $3, 'Test Vendor', 'INV-CM-001',
                   $4, $5, 'USD',
                   'ready_for_review', 'submitted',
                   '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, '{}'::jsonb,
                   $6, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(invoice_date)
    .bind(total_cents)
    .bind(Uuid::new_v4()) // document_id
    .execute(pool)
    .await
    .expect("Failed to seed invoice");
    id
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
// Case A: Non-PO invoice + active contract within tolerance -> Approved
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts/contract_matches tables"]
async fn test_in_band_non_po_invoice_approved(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    // Contract: $1000/mo, 0% escalator, 2% tolerance
    seed_contract(
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

    // Invoice for $1010 (= 101000 cents) — 1% variance, within 2% tolerance
    let invoice_id = seed_invoice(&pool, tenant_id, vendor_id, 101000, Some(invoice_date)).await;
    let tid = TenantId::from_uuid(tenant_id);
    let iid = InvoiceId(invoice_id);

    // Run contract matching + outcome application (mirrors the STP path).
    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date,
        amount: 1010.0,
        currency: "USD".to_string(),
    };
    let outcome = match_invoice_to_contract(&pool, &input, invoice_id)
        .await
        .expect("match should succeed");

    // Verify it returned InBand.
    assert!(
        matches!(outcome, ContractMatchOutcome::InBand { .. }),
        "Expected InBand, got {:?}",
        outcome
    );

    // Apply outcome (same helper used by run_straight_through_processing).
    let status = apply_contract_match_outcome(&Arc::new(pool.clone()), &tid, &iid, &outcome)
        .await
        .expect("should return Some");

    assert_eq!(status, ProcessingStatus::Approved, "InBand should produce Approved");

    // Verify DB state: processing_status = approved.
    let db_status: String =
        sqlx::query_scalar("SELECT processing_status FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(db_status, "approved");

    // Verify contract_matches row exists with match_result = in_band.
    let match_result: String =
        sqlx::query_scalar("SELECT match_result FROM contract_matches WHERE invoice_id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(match_result, "in_band");

    Ok(())
}

// ---------------------------------------------------------------------------
// Case B: Non-PO invoice + active contract outside tolerance -> OnHold
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts/contract_matches tables"]
async fn test_out_of_band_non_po_invoice_on_hold(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;

    // Contract: $1000/mo, 0% escalator, 2% tolerance
    seed_contract(
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

    // Invoice for $1100 (= 110000 cents) — 10% variance, outside 2% tolerance
    let invoice_id = seed_invoice(&pool, tenant_id, vendor_id, 110000, Some(invoice_date)).await;
    let tid = TenantId::from_uuid(tenant_id);
    let iid = InvoiceId(invoice_id);

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date,
        amount: 1100.0,
        currency: "USD".to_string(),
    };
    let outcome = match_invoice_to_contract(&pool, &input, invoice_id)
        .await
        .expect("match should succeed");

    assert!(
        matches!(outcome, ContractMatchOutcome::OutOfBand { .. }),
        "Expected OutOfBand, got {:?}",
        outcome
    );

    let status = apply_contract_match_outcome(&Arc::new(pool.clone()), &tid, &iid, &outcome)
        .await
        .expect("should return Some");

    assert_eq!(status, ProcessingStatus::OnHold, "OutOfBand should produce OnHold");

    // Verify DB state: processing_status = on_hold.
    let db_status: String =
        sqlx::query_scalar("SELECT processing_status FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(db_status, "on_hold");

    // Verify notes contains "Contract mismatch".
    let notes: Option<String> =
        sqlx::query_scalar("SELECT notes FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert!(
        notes.as_ref().map_or(false, |n| n.contains("Contract mismatch")),
        "Expected notes to contain 'Contract mismatch', got {:?}",
        notes
    );

    // Verify contract_matches row exists with match_result = out_of_band.
    let match_result: String =
        sqlx::query_scalar("SELECT match_result FROM contract_matches WHERE invoice_id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(match_result, "out_of_band");

    Ok(())
}

// ---------------------------------------------------------------------------
// Case C: Non-PO invoice + no contract -> fall-through (NoActiveContract)
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with contracts/contract_matches tables"]
async fn test_no_contract_falls_through_to_workflow(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();
    let invoice_date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    seed_tenant(&pool, tenant_id).await;
    seed_vendor(&pool, tenant_id, vendor_id).await;
    // Deliberately do NOT seed a contract.

    let invoice_id = seed_invoice(&pool, tenant_id, vendor_id, 50000, Some(invoice_date)).await;
    let tid = TenantId::from_uuid(tenant_id);
    let iid = InvoiceId(invoice_id);

    let input = ContractMatchInput {
        tenant_id,
        vendor_id,
        invoice_date,
        amount: 500.0,
        currency: "USD".to_string(),
    };
    let outcome = match_invoice_to_contract(&pool, &input, invoice_id)
        .await
        .expect("match should succeed");

    assert_eq!(outcome, ContractMatchOutcome::NoActiveContract);

    // apply_contract_match_outcome returns None for NoActiveContract,
    // signalling the caller to fall through to WorkflowEngine.
    let result = apply_contract_match_outcome(&Arc::new(pool.clone()), &tid, &iid, &outcome).await;
    assert!(
        result.is_none(),
        "NoActiveContract should return None so caller falls through to WorkflowEngine"
    );

    // Invoice should remain in 'submitted' status (set by seed, untouched by contract matching).
    let db_status: String =
        sqlx::query_scalar("SELECT processing_status FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(db_status, "submitted", "Invoice should remain submitted for WorkflowEngine");

    Ok(())
}
