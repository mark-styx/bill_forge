//! Tests for industry starter packs (#354).
//!
//! Unit tests verifying pack data structures, industry parsing, and
//! the generic-empty-pack backward-compatibility contract.

use billforge_api::starter_packs::{self, Industry};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Industry parsing
// ---------------------------------------------------------------------------

#[test]
fn industry_from_str_valid_values() {
    assert_eq!(
        Industry::from_str("construction").unwrap(),
        Industry::Construction
    );
    assert_eq!(
        Industry::from_str("professional_services").unwrap(),
        Industry::ProfessionalServices
    );
    assert_eq!(Industry::from_str("retail").unwrap(), Industry::Retail);
    assert_eq!(Industry::from_str("generic").unwrap(), Industry::Generic);
}

#[test]
fn industry_from_str_rejects_unknown() {
    let err = Industry::from_str("healthcare").unwrap_err();
    assert!(err.contains("Unknown industry 'healthcare'"), "{}", err);
    assert!(err.contains("construction"), "{}", err);
    assert!(err.contains("retail"), "{}", err);
}

#[test]
fn industry_from_str_rejects_empty() {
    assert!(Industry::from_str("").is_err());
}

#[test]
fn industry_from_str_is_case_sensitive() {
    assert!(Industry::from_str("Construction").is_err());
    assert!(Industry::from_str("RETAIL").is_err());
}

// ---------------------------------------------------------------------------
// Serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn industry_serde_round_trip() {
    for industry in [
        Industry::Construction,
        Industry::ProfessionalServices,
        Industry::Retail,
        Industry::Generic,
    ] {
        let json = serde_json::to_string(&industry).unwrap();
        let back: Industry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, industry);
    }
}

#[test]
fn industry_json_uses_snake_case() {
    let json = serde_json::to_string(&Industry::ProfessionalServices).unwrap();
    assert_eq!(json, "\"professional_services\"");
}

// ---------------------------------------------------------------------------
// Pack data integrity
// ---------------------------------------------------------------------------

#[test]
fn generic_pack_is_empty() {
    let pack = starter_packs::pack_for(Industry::Generic);
    assert!(pack.gl_accounts.is_empty());
    assert!(pack.vendor_categories.is_empty());
    assert!(pack.approval_rules.is_empty());
    assert!(pack.policy_thresholds.is_empty());
}

#[test]
fn construction_pack_contains_subcontractor_labor_gl() {
    let pack = starter_packs::pack_for(Industry::Construction);
    let found = pack
        .gl_accounts
        .iter()
        .find(|a| a.code == "5100" && a.name == "Subcontractor Labor");
    assert!(found.is_some(), "Construction pack must include 5100 - Subcontractor Labor");
}

#[test]
fn construction_pack_sizes() {
    let pack = starter_packs::pack_for(Industry::Construction);
    assert_eq!(pack.gl_accounts.len(), 8);
    assert_eq!(pack.vendor_categories.len(), 6);
    assert_eq!(pack.approval_rules.len(), 2);
    assert_eq!(pack.policy_thresholds.len(), 3);
}

#[test]
fn professional_services_pack_sizes() {
    let pack = starter_packs::pack_for(Industry::ProfessionalServices);
    assert_eq!(pack.gl_accounts.len(), 8);
    assert_eq!(pack.vendor_categories.len(), 6);
    assert_eq!(pack.approval_rules.len(), 2);
    assert_eq!(pack.policy_thresholds.len(), 3);
}

#[test]
fn retail_pack_sizes() {
    let pack = starter_packs::pack_for(Industry::Retail);
    assert_eq!(pack.gl_accounts.len(), 8);
    assert_eq!(pack.vendor_categories.len(), 6);
    assert_eq!(pack.approval_rules.len(), 2);
    assert_eq!(pack.policy_thresholds.len(), 3);
}

// ---------------------------------------------------------------------------
// Cross-reference: vendor categories reference valid GL codes
// ---------------------------------------------------------------------------

#[test]
fn vendor_categories_reference_existing_gl_codes() {
    for industry in [
        Industry::Construction,
        Industry::ProfessionalServices,
        Industry::Retail,
    ] {
        let pack = starter_packs::pack_for(industry);
        let codes: std::collections::HashSet<&str> =
            pack.gl_accounts.iter().map(|a| a.code).collect();
        for cat in &pack.vendor_categories {
            assert!(
                codes.contains(cat.default_gl_code),
                "{:?}: vendor category '{}' references unknown GL code '{}'",
                industry,
                cat.name,
                cat.default_gl_code
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Approval rules have valid structure
// ---------------------------------------------------------------------------

#[test]
fn approval_rules_have_required_json_fields() {
    for industry in [
        Industry::Construction,
        Industry::ProfessionalServices,
        Industry::Retail,
    ] {
        let pack = starter_packs::pack_for(industry);
        for rule in &pack.approval_rules {
            // conditions must be an array
            assert!(
                rule.conditions.is_array(),
                "{:?}: rule '{}' conditions is not an array",
                industry,
                rule.name
            );
            // actions must be an array
            assert!(
                rule.actions.is_array(),
                "{:?}: rule '{}' actions is not an array",
                industry,
                rule.name
            );
            // priority > 0
            assert!(
                rule.priority > 0,
                "{:?}: rule '{}' has non-positive priority",
                industry,
                rule.name
            );
        }
    }
}
