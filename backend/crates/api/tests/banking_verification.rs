//! Integration tests for vendor banking-change verification workflow (refs #243)
//!
//! Validates that:
//!   1. PUT banking details creates a pending verification row, sets payment_hold=true,
//!      and writes an AuditEntry with action VendorBankingChanged.
//!   2. Calling verify with valid callback payload sets status=verified, clears
//!      payment_hold, and writes an audit entry.
//!   3. is_payment_blocked returns true while pending, false after verified.

use billforge_core::domain::{AuditAction, BankingVerificationStatus, ResourceType};
use billforge_core::TenantId;
use billforge_core::VendorRepository;
use billforge_db::repositories::AuditRepositoryImpl;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run all tenant migrations so the new banking columns and verification table exist.
async fn setup_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

/// Insert a minimal user row so audit_log.user_id FK is satisfied.
async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("banking-test@example.com")
    .bind("hash_not_used")
    .bind("Banking Test User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Insert a minimal vendor row.
async fn insert_vendor(pool: &sqlx::PgPool, tenant_id: &TenantId) -> Uuid {
    let vendor_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, vendor_type)
           VALUES ($1, $2, 'Test Vendor Banking', 'business')"#,
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .execute(pool)
    .await
    .expect("insert test vendor");
    vendor_id
}

/// Read the latest audit_log entry for a resource_id, returning action and resource_type.
async fn read_latest_audit(pool: &sqlx::PgPool, resource_id: &str) -> Option<(String, String)> {
    sqlx::query_as(
        "SELECT action, resource_type FROM audit_log WHERE resource_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(resource_id)
    .fetch_optional(pool)
    .await
    .expect("query audit_log")
}

// ============================================================================
// Test 1: PUT banking details creates pending verification, sets payment_hold,
//         and writes VendorBankingChanged audit entry
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn banking_change_creates_pending_verification_and_freezes_payments(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Record a banking change (simulates PUT /:id/banking)
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None, // no previous banking
            "6789",
            "First National",
            "checking",
            "enc:123456789",
            "enc:021000021",
            user_id,
        )
        .await
        .expect("record banking change");

    // Verification should be pending
    assert_eq!(verification.status, BankingVerificationStatus::Pending);
    assert_eq!(verification.new_account_last_four, "6789");
    assert_eq!(verification.previous_account_last_four, None);
    assert_eq!(verification.vendor_id, vendor_id_obj);

    // Vendor should have payment_hold = true
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        vendor.payment_hold,
        "payment_hold should be true after banking change"
    );
    assert!(vendor.payment_hold_reason.is_some());

    // Vendor should have banking columns populated
    let bank_account = vendor
        .bank_account
        .expect("bank_account should be populated");
    assert_eq!(bank_account.account_last_four, "6789");
    assert_eq!(bank_account.bank_name, "First National");

    // Pending verification should exist in DB
    let has_pending = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(has_pending, "should have pending banking verification");

    // Write audit entry (simulates what the route handler does)
    use billforge_core::domain::AuditEntry;
    use billforge_core::traits::AuditService;
    use billforge_core::UserId;

    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(UserId(user_id)),
        AuditAction::VendorBankingChanged,
        ResourceType::Vendor,
        vendor_id.to_string(),
        "Banking details changed for vendor Test Vendor Banking",
    )
    .with_user_email("banking-test@example.com")
    .with_metadata(serde_json::json!({
        "verification_id": verification.id.to_string(),
        "prev_last_four": null,
        "new_last_four": "6789",
    }));

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(audit_entry).await.expect("audit log write");

    // Verify audit entry was written with correct action
    let audit_row = read_latest_audit(&pool, &vendor_id.to_string())
        .await
        .expect("audit row must exist");
    assert_eq!(audit_row.0, "vendor_banking_changed");
    assert_eq!(audit_row.1, "vendor");
}

// ============================================================================
// Test 2: Verify endpoint sets status=verified, clears payment_hold,
//         and writes VendorBankingVerified audit entry
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn verify_banking_clears_hold_and_creates_audit_entry(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();
    let verifier_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    insert_user(&pool, &tenant_id, verifier_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // First, record a banking change
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None,
            "4321",
            "Bank of America",
            "checking",
            "enc:987654321",
            "enc:021000021",
            user_id,
        )
        .await
        .expect("record banking change");

    // Confirm payment_hold is true
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(vendor.payment_hold);

    // Now verify the banking change (simulates POST /:id/banking-verifications/:vid/verify)
    let verified = repo
        .verify_banking_change(
            &tenant_id,
            verification.id,
            verifier_id,
            "phone",
            "+1-555-0100",
            Some("Called vendor contact, confirmed new account"),
        )
        .await
        .expect("verify banking change");

    // Verification should be verified
    assert_eq!(verified.status, BankingVerificationStatus::Verified);
    assert_eq!(verified.verified_by, Some(verifier_id));
    assert!(verified.verified_at.is_some());
    assert_eq!(verified.callback_contact, Some("+1-555-0100".to_string()));

    // Vendor should have payment_hold cleared
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        !vendor.payment_hold,
        "payment_hold should be false after verification"
    );
    assert!(vendor.payment_hold_reason.is_none());

    // No pending verification should remain
    let has_pending = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(
        !has_pending,
        "should not have pending banking verification after verify"
    );

    // Write audit entry (simulates route handler)
    use billforge_core::domain::AuditEntry;
    use billforge_core::traits::AuditService;
    use billforge_core::UserId;

    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(UserId(verifier_id)),
        AuditAction::VendorBankingVerified,
        ResourceType::VendorBankingVerification,
        verification.id.to_string(),
        "Banking change verified via phone",
    )
    .with_user_email("banking-test@example.com")
    .with_metadata(serde_json::json!({
        "verification_id": verification.id.to_string(),
        "callback_method": "phone",
    }));

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(audit_entry).await.expect("audit log write");

    // Verify audit entry
    let audit_row = read_latest_audit(&pool, &verification.id.to_string())
        .await
        .expect("audit row must exist");
    assert_eq!(audit_row.0, "vendor_banking_verified");
    assert_eq!(audit_row.1, "vendor_banking_verification");
}

// ============================================================================
// Test 3: is_payment_blocked helper returns true while pending, false after verified
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn is_payment_blocked_returns_true_while_pending_false_after_verified(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();
    let verifier_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    insert_user(&pool, &tenant_id, verifier_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Before any banking change, not blocked
    let blocked = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(!blocked, "should not be blocked before banking change");

    // Record a banking change
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None,
            "9999",
            "Wells Fargo",
            "savings",
            "enc:111111111",
            "enc:021000021",
            user_id,
        )
        .await
        .expect("record banking change");

    // Now blocked
    let blocked = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(blocked, "should be blocked while pending verification");

    // Verify the change
    repo.verify_banking_change(
        &tenant_id,
        verification.id,
        verifier_id,
        "known_email",
        "vendor@example.com",
        Some("Confirmed via known email"),
    )
    .await
    .expect("verify banking change");

    // No longer blocked
    let blocked = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(!blocked, "should not be blocked after verification");
}

// ============================================================================
// Test 4 (refs #266): First approval does NOT clear payment_hold,
//         sets status to PendingSecondApproval, and populates screening_results.
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn first_approval_does_not_clear_payment_hold(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Record a banking change
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None,
            "1234",
            "Chase",
            "checking",
            "enc:11112222",
            "enc:021000021",
            user_id,
        )
        .await
        .expect("record banking change");

    assert_eq!(verification.status, BankingVerificationStatus::Pending);

    // Simulate first approval with screening results
    let screening = serde_json::json!({
        "ofac": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "avs": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "plaid": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
    });

    let first_approved = repo
        .record_first_approval(
            &tenant_id,
            verification.id,
            user_id,
            &screening,
            "phone",
            "+1-555-0200",
            Some("First approver notes"),
        )
        .await
        .expect("record first approval");

    // Status should be PendingSecondApproval, NOT Verified
    assert_eq!(
        first_approved.status,
        BankingVerificationStatus::PendingSecondApproval,
        "status should be PendingSecondApproval after first approval"
    );

    // First approver should be recorded
    assert_eq!(first_approved.first_approver_id, Some(user_id));
    assert!(first_approved.first_approved_at.is_some());

    // Second approver should NOT be set yet
    assert!(first_approved.second_approver_id.is_none());
    assert!(first_approved.second_approved_at.is_none());

    // Screening results should be populated
    let sr = &first_approved.screening_results;
    assert!(
        sr.get("ofac").is_some(),
        "screening_results should have ofac"
    );
    assert!(sr.get("avs").is_some(), "screening_results should have avs");
    assert!(
        sr.get("plaid").is_some(),
        "screening_results should have plaid"
    );

    // Vendor payment_hold should STILL be true
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        vendor.payment_hold,
        "payment_hold must remain true after first approval"
    );

    // Should still be considered "pending" (blocking payments)
    let blocked = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(blocked, "should still be blocked after first approval");
}

// ============================================================================
// Test 5 (refs #266): Same user cannot provide both approvals
//         (separation-of-duties enforcement at the DB level)
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn same_user_cannot_dual_approve(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_id).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Record a banking change
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None,
            "5678",
            "Citibank",
            "checking",
            "enc:55556666",
            "enc:021000021",
            user_id,
        )
        .await
        .expect("record banking change");

    // First approval as user_id
    let screening = serde_json::json!({
        "ofac": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "avs": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "plaid": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
    });

    repo.record_first_approval(
        &tenant_id,
        verification.id,
        user_id,
        &screening,
        "phone",
        "+1-555-0300",
        None,
    )
    .await
    .expect("first approval should succeed");

    // Attempt second approval as the SAME user: the DB constraint should reject this
    let result = repo
        .record_second_approval(&tenant_id, verification.id, user_id)
        .await;

    assert!(
        result.is_err(),
        "second approval by the same user must fail"
    );

    // Payment hold should remain
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        vendor.payment_hold,
        "payment_hold must remain true when dual-approval fails"
    );
}

// ============================================================================
// Test 6 (refs #266): Second distinct approver clears hold,
//         records both approvers, and writes VendorBankingVerified audit.
// ============================================================================

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test banking_verification -- --ignored
async fn second_distinct_approver_clears_hold(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    setup_schema(&pool, &tenant_id).await;
    insert_user(&pool, &tenant_id, user_a).await;
    insert_user(&pool, &tenant_id, user_b).await;
    let vendor_id = insert_vendor(&pool, &tenant_id).await;

    let vendor_id_obj = billforge_core::domain::VendorId(vendor_id);
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Record a banking change
    let verification = repo
        .record_banking_change(
            &tenant_id,
            &vendor_id_obj,
            None,
            "9999",
            "Wells Fargo",
            "savings",
            "enc:77778888",
            "enc:021000021",
            user_a,
        )
        .await
        .expect("record banking change");

    // First approval as user A
    let screening = serde_json::json!({
        "ofac": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "avs": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
        "plaid": { "status": "pass", "checked_at": "2026-01-01T00:00:00Z" },
    });

    repo.record_first_approval(
        &tenant_id,
        verification.id,
        user_a,
        &screening,
        "in_person",
        "Office visit",
        Some("Met vendor representative"),
    )
    .await
    .expect("first approval");

    // Confirm hold is still active after first approval
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        vendor.payment_hold,
        "hold should be active after first approval"
    );

    // Second approval as user B (distinct from user A)
    let fully_verified = repo
        .record_second_approval(&tenant_id, verification.id, user_b)
        .await
        .expect("second approval should succeed");

    // Status should be Verified
    assert_eq!(
        fully_verified.status,
        BankingVerificationStatus::Verified,
        "status should be Verified after second approval"
    );

    // Both approvers recorded
    assert_eq!(fully_verified.first_approver_id, Some(user_a));
    assert!(fully_verified.first_approved_at.is_some());
    assert_eq!(fully_verified.second_approver_id, Some(user_b));
    assert!(fully_verified.second_approved_at.is_some());

    // verified_by should be the second approver
    assert_eq!(fully_verified.verified_by, Some(user_b));
    assert!(fully_verified.verified_at.is_some());

    // Payment hold should be cleared
    let vendor = repo
        .get_by_id(&tenant_id, &vendor_id_obj)
        .await
        .expect("get vendor")
        .expect("vendor exists");
    assert!(
        !vendor.payment_hold,
        "payment_hold should be cleared after second approval"
    );
    assert!(vendor.payment_hold_reason.is_none());

    // No longer blocked
    let blocked = repo
        .has_pending_banking_verification(&tenant_id, &vendor_id_obj)
        .await
        .expect("check pending");
    assert!(!blocked, "should not be blocked after full dual approval");

    // Write audit entry (simulates route handler writing VendorBankingVerified)
    use billforge_core::domain::AuditEntry;
    use billforge_core::traits::AuditService;
    use billforge_core::UserId;

    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(UserId(user_b)),
        AuditAction::VendorBankingVerified,
        ResourceType::VendorBankingVerification,
        verification.id.to_string(),
        "Banking change fully verified (dual approval)",
    )
    .with_user_email("banking-test@example.com")
    .with_metadata(serde_json::json!({
        "verification_id": verification.id.to_string(),
        "first_approver_id": user_a.to_string(),
        "second_approver_id": user_b.to_string(),
    }));

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(audit_entry).await.expect("audit log write");

    // Verify audit entry
    let audit_row = read_latest_audit(&pool, &verification.id.to_string())
        .await
        .expect("audit row must exist");
    assert_eq!(audit_row.0, "vendor_banking_verified");
    assert_eq!(audit_row.1, "vendor_banking_verification");
}
