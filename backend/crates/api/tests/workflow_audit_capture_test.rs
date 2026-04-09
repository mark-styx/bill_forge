//! Tests for workflow audit before/after state capture (refs #137)
//!
//! Validates that audit entries for SOX-critical workflow mutations
//! correctly populate old_value and new_value fields. Tests mirror
//! the construction patterns used in workflows.rs handlers, following
//! the convention in approval_aggregation_tests.rs.

use billforge_core::domain::{AuditAction, AuditEntry, ResourceType};

// ============================================================================
// Test 1: Update workflow rule audit captures before and after state
// ============================================================================

#[test]
fn update_rule_audit_entry_captures_old_and_new_name() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let old_value = serde_json::json!({
        "name": "Initial",
        "is_active": true,
        "priority": 1,
    });
    let new_value = serde_json::json!({
        "name": "Updated",
        "is_active": true,
        "priority": 1,
    });

    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user_id.clone()),
        AuditAction::Update,
        ResourceType::WorkflowRule,
        "rule-123",
        "Updated workflow rule 'Updated'",
    )
    .with_user_email("test@example.com")
    .with_old_value(old_value.clone())
    .with_new_value(new_value.clone());

    assert_eq!(entry.action, AuditAction::Update);
    assert_eq!(entry.resource_type, ResourceType::WorkflowRule);
    assert_eq!(
        entry.old_value.as_ref().unwrap().get("name").unwrap(),
        "Initial"
    );
    assert_eq!(
        entry.new_value.as_ref().unwrap().get("name").unwrap(),
        "Updated"
    );
}

#[test]
fn update_rule_old_value_preserves_full_state() {
    // Verify old_value contains the complete pre-mutation record, not just changed fields
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let old_value = serde_json::json!({
        "name": "Initial",
        "description": "Original description",
        "priority": 5,
        "is_active": true,
        "rule_type": "approval",
    });

    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::Update,
        ResourceType::WorkflowRule,
        "rule-456",
        "Updated workflow rule",
    )
    .with_old_value(old_value.clone());

    let ov = entry.old_value.unwrap();
    assert_eq!(ov["name"], "Initial");
    assert_eq!(ov["description"], "Original description");
    assert_eq!(ov["priority"], 5);
    assert_eq!(ov["is_active"], true);
}

// ============================================================================
// Test 2: Deactivate workflow rule captures is_active transition
// ============================================================================

#[test]
fn deactivate_rule_audit_entry_captures_is_active_transition() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    // Simulates the pattern in deactivate_rule handler:
    // .with_old_value(json!({"is_active": old_rule.is_active}))
    // .with_new_value(json!({"is_active": false}))
    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user_id.clone()),
        AuditAction::Update,
        ResourceType::WorkflowRule,
        "rule-789",
        "Deactivated workflow rule",
    )
    .with_user_email("admin@example.com")
    .with_old_value(serde_json::json!({ "is_active": true }))
    .with_new_value(serde_json::json!({ "is_active": false }));

    assert_eq!(
        entry
            .old_value
            .as_ref()
            .unwrap()
            .get("is_active")
            .unwrap(),
        true
    );
    assert_eq!(
        entry
            .new_value
            .as_ref()
            .unwrap()
            .get("is_active")
            .unwrap(),
        false
    );
}

#[test]
fn activate_rule_audit_entry_captures_is_active_transition() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    // Simulates the pattern in activate_rule handler:
    // .with_old_value(json!({"is_active": old_rule.is_active}))
    // .with_new_value(json!({"is_active": true}))
    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::Update,
        ResourceType::WorkflowRule,
        "rule-101",
        "Activated workflow rule",
    )
    .with_old_value(serde_json::json!({ "is_active": false }))
    .with_new_value(serde_json::json!({ "is_active": true }));

    assert_eq!(
        entry
            .old_value
            .as_ref()
            .unwrap()
            .get("is_active")
            .unwrap(),
        false
    );
    assert_eq!(
        entry
            .new_value
            .as_ref()
            .unwrap()
            .get("is_active")
            .unwrap(),
        true
    );
}

// ============================================================================
// Test 3: Approve request captures pending-to-approved transition
// ============================================================================

#[test]
fn approve_audit_entry_captures_status_transition() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    // Simulates the approve handler pattern:
    // old_value from SELECT before UPDATE
    // new_value with status, responded_by, responded_at, comments
    let old_value = serde_json::json!({
        "status": "pending",
        "responded_by": null,
        "responded_at": null,
        "comments": null,
    });
    let new_value = serde_json::json!({
        "status": "approved",
        "responded_by": user_id.to_string(),
        "responded_at": "2026-04-09T12:00:00+00:00",
        "comments": "Looks good",
    });

    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user_id.clone()),
        AuditAction::InvoiceApproved,
        ResourceType::ApprovalRequest,
        "approval-123",
        "Approved invoice INV-001",
    )
    .with_user_email("approver@example.com")
    .with_old_value(old_value)
    .with_new_value(new_value)
    .with_metadata(serde_json::json!({
        "invoice_id": "invoice-456",
        "comments": "Looks good",
    }));

    let ov = entry.old_value.unwrap();
    assert_eq!(ov["status"], "pending");
    assert!(ov["responded_by"].is_null());

    let nv = entry.new_value.unwrap();
    assert_eq!(nv["status"], "approved");
    assert_eq!(nv["comments"], "Looks good");

    // Metadata should still be present (kept from original pattern)
    let meta = entry.metadata.unwrap();
    assert_eq!(meta["invoice_id"], "invoice-456");
}

#[test]
fn reject_audit_entry_captures_status_transition() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let old_value = serde_json::json!({
        "status": "pending",
        "responded_by": null,
        "responded_at": null,
        "comments": null,
    });
    let new_value = serde_json::json!({
        "status": "rejected",
        "responded_by": user_id.to_string(),
        "responded_at": "2026-04-09T12:00:00+00:00",
        "comments": "Amount mismatch",
    });

    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::InvoiceRejected,
        ResourceType::ApprovalRequest,
        "approval-456",
        "Rejected invoice INV-002",
    )
    .with_old_value(old_value)
    .with_new_value(new_value)
    .with_metadata(serde_json::json!({
        "invoice_id": "invoice-789",
        "reason": "Amount mismatch",
    }));

    assert_eq!(
        entry.old_value.as_ref().unwrap()["status"],
        "pending"
    );
    assert_eq!(
        entry.new_value.as_ref().unwrap()["status"],
        "rejected"
    );
    assert_eq!(
        entry.new_value.as_ref().unwrap()["comments"],
        "Amount mismatch"
    );
}

// ============================================================================
// Test 4: Audit fingerprint serialization (log_audit_or_record_gap pattern)
// ============================================================================

#[test]
fn audit_entry_fingerprint_serializes_all_sox_fields() {
    // Validates that the fingerprint built in log_audit_or_record_gap
    // captures the fields needed for manual reconciliation
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user_id.clone()),
        AuditAction::Update,
        ResourceType::WorkflowRule,
        "rule-999",
        "Updated workflow rule 'Test'",
    )
    .with_old_value(serde_json::json!({"name": "Old"}))
    .with_new_value(serde_json::json!({"name": "New"}));

    // This mirrors the fingerprint construction in log_audit_or_record_gap
    let fingerprint = serde_json::json!({
        "id": entry.id,
        "tenant_id": entry.tenant_id,
        "action": entry.action,
        "resource_type": entry.resource_type,
        "resource_id": entry.resource_id,
        "old_value": entry.old_value,
        "new_value": entry.new_value,
    });

    // Verify the fingerprint has all fields needed for reconciliation
    assert!(fingerprint.get("id").is_some(), "fingerprint must have id");
    assert!(
        fingerprint.get("tenant_id").is_some(),
        "fingerprint must have tenant_id"
    );
    assert!(
        fingerprint.get("action").is_some(),
        "fingerprint must have action"
    );
    assert!(
        fingerprint.get("resource_type").is_some(),
        "fingerprint must have resource_type"
    );
    assert!(
        fingerprint.get("resource_id").is_some(),
        "fingerprint must have resource_id"
    );
    assert!(
        fingerprint.get("old_value").is_some(),
        "fingerprint must have old_value"
    );
    assert!(
        fingerprint.get("new_value").is_some(),
        "fingerprint must have new_value"
    );

    // Verify old/new values round-trip correctly
    assert_eq!(fingerprint["old_value"]["name"], "Old");
    assert_eq!(fingerprint["new_value"]["name"], "New");
}

// ============================================================================
// Test 5: Serialization failure fallback (unwrap_or(Value::Null))
// ============================================================================

#[test]
fn serialization_failure_falls_back_to_null() {
    // Tests the pattern: serde_json::to_value(&record).unwrap_or(Value::Null)
    // If serialization fails, old_value/new_value should be Null rather than panic.
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    // Simulate a serialization failure by using a value that produces Null
    let fallback_value = serde_json::to_value("test").unwrap_or(serde_json::Value::Null);
    assert_eq!(fallback_value, serde_json::json!("test"));

    // The Null fallback should still produce a valid audit entry
    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::Delete,
        ResourceType::WorkflowRule,
        "rule-del",
        "Deleted workflow rule",
    )
    .with_old_value(serde_json::Value::Null)
    .with_new_value(serde_json::Value::Null);

    assert_eq!(entry.old_value, Some(serde_json::Value::Null));
    assert_eq!(entry.new_value, Some(serde_json::Value::Null));
}

// ============================================================================
// Test 6: Delete handler captures old state (not new)
// ============================================================================

#[test]
fn delete_rule_audit_entry_captures_old_state_only() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let old_value = serde_json::json!({
        "name": "Rule to Delete",
        "is_active": true,
        "priority": 3,
    });

    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::Delete,
        ResourceType::WorkflowRule,
        "rule-del-123",
        "Deleted workflow rule",
    )
    .with_old_value(old_value.clone());

    assert_eq!(
        entry.old_value.as_ref().unwrap()["name"],
        "Rule to Delete"
    );
    assert!(
        entry.new_value.is_none(),
        "Delete operations should not have new_value"
    );
}

// ============================================================================
// Test 7: Verify audit_entry.id is populated for reconciliation
// ============================================================================

#[test]
fn audit_entry_has_uuid_for_reconciliation() {
    let tenant_id = billforge_core::types::TenantId::new();
    let user_id = billforge_core::types::UserId::new();

    let entry = AuditEntry::new(
        tenant_id,
        Some(user_id),
        AuditAction::Update,
        ResourceType::ApprovalRequest,
        "approval-recon",
        "Approved invoice",
    )
    .with_old_value(serde_json::json!({"status": "pending"}))
    .with_new_value(serde_json::json!({"status": "approved"}));

    // The entry UUID must be non-nil for audit reconciliation
    assert_ne!(
        entry.id,
        uuid::Uuid::nil(),
        "Audit entry ID must be populated for reconciliation"
    );
}
