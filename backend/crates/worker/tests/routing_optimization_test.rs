//! Tests for routing optimization worker tenant isolation.
//!
//! The fix changed `run_routing_optimization` to use tenant-specific pools
//! (via `pg_manager.tenant()`) instead of the shared metadata pool for
//! per-tenant routing queries. This matches the pattern in anomaly_detection.rs
//! and categorization_training.rs.
//!
//! Pure logic tests for expertise score calculation live in the `#[cfg(test)]`
//! module inside routing_optimization.rs. This file tests the tenant isolation
//! contract at the type level.

use billforge_core::types::TenantId;
use uuid::Uuid;

#[test]
fn test_tenant_id_roundtrip() {
    // Verify TenantId round-trips through string representation.
    // The routing_optimization loop uses tenant_id.as_str() for logging
    // and pg_manager.tenant(&tenant_id) for pool lookup. Both must agree.
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let tenant_id_str = tenant_id.as_str();
    assert!(!tenant_id_str.is_empty());

    let parsed: TenantId = tenant_id_str.parse().expect("TenantId should parse back");
    assert_eq!(tenant_id, parsed);
}

#[test]
fn test_multiple_tenant_ids_are_distinct() {
    // Verifies that distinct TenantIds produce distinct string keys,
    // which is critical for pg_manager.tenant() routing to the correct
    // database per tenant.
    let t1 = TenantId::from_uuid(Uuid::new_v4());
    let t2 = TenantId::from_uuid(Uuid::new_v4());

    assert_ne!(t1, t2);
    assert_ne!(t1.as_str(), t2.as_str());
}
