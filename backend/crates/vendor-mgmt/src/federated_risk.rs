//! Federated Vendor Risk Network (refs #408).
//!
//! Privacy-preserving cross-tenant vendor risk signal sharing. Tenants opt
//! in via `tenant_risk_network_consent`, contribute hashed signals to the
//! metadata-DB `federated_vendor_signals` table, and read k-anonymized
//! aggregates (floor of 5 distinct contributing tenants) when querying a
//! local vendor.
//!
//! Hashing contract (must stay in lockstep with migration 141):
//!   - `vendor_hash`             = SHA-256(network_salt || canonical_tuple)
//!   - `contributing_tenant_hash` = HMAC-SHA256(network_salt, tenant_id_bytes)
//!
//! `why this vendor is flagged` explanations are templated solely from
//! signal_type + contributor_count - no tenant-specific data leaks through.

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::fmt;
use uuid::Uuid;

/// Minimum number of distinct contributing tenants required before an
/// aggregate row is returned (k-anonymity floor). Mirrors the value used
/// by the peer-insights benchmark surface (migration 130).
pub const DEFAULT_K_ANONYMITY_FLOOR: i64 = 5;

// ---------------------------------------------------------------------------
// Signal type discriminator
// ---------------------------------------------------------------------------

/// The four federated risk signal categories accepted by the network.
/// Must stay in lockstep with the `signal_type` CHECK constraint on
/// `federated_vendor_signals`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederatedSignalType {
    BankAccountChange,
    OfacNearMatch,
    FakeInvoicePattern,
    DisputeRateHigh,
}

impl FederatedSignalType {
    /// Wire/database token for this variant.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::BankAccountChange => "bank_account_change",
            Self::OfacNearMatch => "ofac_near_match",
            Self::FakeInvoicePattern => "fake_invoice_pattern",
            Self::DisputeRateHigh => "dispute_rate_high",
        }
    }

    /// Parse the database token back into a variant.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "bank_account_change" => Some(Self::BankAccountChange),
            "ofac_near_match" => Some(Self::OfacNearMatch),
            "fake_invoice_pattern" => Some(Self::FakeInvoicePattern),
            "dispute_rate_high" => Some(Self::DisputeRateHigh),
            _ => None,
        }
    }

    /// Human label used in the network-only explanation sentence.
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::BankAccountChange => "suspicious bank-account change",
            Self::OfacNearMatch => "OFAC near-match",
            Self::FakeInvoicePattern => "fake-invoice pattern",
            Self::DisputeRateHigh => "elevated dispute rate",
        }
    }
}

impl fmt::Display for FederatedSignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_label())
    }
}

// ---------------------------------------------------------------------------
// Aggregate DTO returned by aggregate_for_vendor
// ---------------------------------------------------------------------------

/// A single k-anonymized aggregate row. `explanation` is templated solely
/// from signal_type + contributor_count - no tenant-specific data leaks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSignalAggregate {
    pub signal_type: FederatedSignalType,
    pub contributor_count: i64,
    pub weighted_score: f64,
    pub explanation: String,
}

// ---------------------------------------------------------------------------
// Hashing primitives
// ---------------------------------------------------------------------------

/// Salted SHA-256 hash of a canonical vendor tuple. The salt MUST stay
/// constant across the whole network (per-process env var `NETWORK_HASH_SALT`)
/// so two tenants hashing the same vendor land on the same bucket.
///
/// The canonical tuple is `salt|normalized_name|tax_id|bank_fingerprint`,
/// with `|` as separator and empty strings for missing optional fields. The
/// caller is expected to have already normalized `normalized_name` to a
/// lowercased, punctuation-stripped form (see `normalize_vendor_name`).
pub fn vendor_hash(
    normalized_name: &str,
    tax_id: Option<&str>,
    bank_fingerprint: Option<&str>,
    network_salt: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(network_salt.as_bytes());
    hasher.update(b"|");
    hasher.update(normalized_name.as_bytes());
    hasher.update(b"|");
    hasher.update(tax_id.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(bank_fingerprint.unwrap_or("").as_bytes());
    hex::encode(hasher.finalize())
}

/// HMAC-SHA256(network_salt, tenant_id_bytes). Used as the
/// `contributing_tenant_hash` so distinct-contributor counts are possible
/// without storing or exposing which tenants contributed.
pub fn tenant_hmac(tenant_id: Uuid, network_salt: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(network_salt.as_bytes())
        .expect("HMAC-SHA256 accepts any key length");
    mac.update(tenant_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Lowercase + collapse whitespace + strip non-alphanumeric. Cheap canonical
/// form so that "Acme, Inc." and "ACME INC" collapse to the same hash bucket.
pub fn normalize_vendor_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_space = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------

/// Returns `true` when an active `tenant_risk_network_consent` row exists
/// (opted_in_at set, opted_out_at NULL).
pub async fn is_tenant_opted_in(meta_pool: &PgPool, tenant_id: Uuid) -> Result<bool> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT (opted_out_at IS NULL) FROM tenant_risk_network_consent WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(meta_pool)
    .await
    .context("Failed to read tenant_risk_network_consent")?;

    Ok(row.map(|(active,)| active).unwrap_or(false))
}

/// Upsert an active opt-in row. Idempotent: calling twice leaves a single
/// row with `opted_out_at` cleared.
pub async fn opt_in_tenant(meta_pool: &PgPool, tenant_id: Uuid) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tenant_risk_network_consent (tenant_id, opted_in_at, opted_out_at)
        VALUES ($1, NOW(), NULL)
        ON CONFLICT (tenant_id) DO UPDATE SET
            opted_in_at  = NOW(),
            opted_out_at = NULL
        "#,
    )
    .bind(tenant_id)
    .execute(meta_pool)
    .await
    .context("Failed to opt tenant into risk network")?;
    Ok(())
}

/// Mark the tenant as opted out. Existing contributions are intentionally
/// left in place: they are hashed and unlinkable, and removing the
/// HMAC-hashed contributor records would skew historical aggregates without
/// providing extra privacy.
pub async fn opt_out_tenant(meta_pool: &PgPool, tenant_id: Uuid) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tenant_risk_network_consent (tenant_id, opted_in_at, opted_out_at)
        VALUES ($1, NOW(), NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            opted_out_at = NOW()
        "#,
    )
    .bind(tenant_id)
    .execute(meta_pool)
    .await
    .context("Failed to opt tenant out of risk network")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Contribution
// ---------------------------------------------------------------------------

/// Append a hashed signal to `federated_vendor_signals` if (and only if)
/// the tenant has an active opt-in row. No-op for opted-out tenants so
/// callsites can invoke this unconditionally.
pub async fn contribute_signal(
    meta_pool: &PgPool,
    tenant_id: Uuid,
    vendor_hash: &str,
    signal_type: FederatedSignalType,
    weight: f32,
    network_salt: &str,
) -> Result<()> {
    if !is_tenant_opted_in(meta_pool, tenant_id).await? {
        // Strict no-op for non-opted-in tenants.
        return Ok(());
    }
    let tenant_hash = tenant_hmac(tenant_id, network_salt);
    sqlx::query(
        r#"
        INSERT INTO federated_vendor_signals
            (vendor_hash, signal_type, contributing_tenant_hash, signal_weight)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(vendor_hash)
    .bind(signal_type.as_db_str())
    .bind(&tenant_hash)
    .bind(weight)
    .execute(meta_pool)
    .await
    .context("Failed to insert federated vendor signal")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Aggregation (read surface)
// ---------------------------------------------------------------------------

/// Return aggregated signals for a single vendor, suppressing any row whose
/// `contributor_count` falls below the k-anonymity floor. Reads the view
/// `federated_vendor_risk_aggregates` created in migration 141.
pub async fn aggregate_for_vendor(
    meta_pool: &PgPool,
    vendor_hash: &str,
    k_anonymity_floor: i64,
) -> Result<Vec<NetworkSignalAggregate>> {
    let rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
        r#"
        SELECT signal_type, signal_count, contributor_count, weighted_score
        FROM federated_vendor_risk_aggregates
        WHERE vendor_hash = $1
          AND contributor_count >= $2
        ORDER BY weighted_score DESC
        "#,
    )
    .bind(vendor_hash)
    .bind(k_anonymity_floor)
    .fetch_all(meta_pool)
    .await
    .context("Failed to read federated vendor risk aggregates")?;

    let mut out: Vec<NetworkSignalAggregate> = Vec::with_capacity(rows.len());
    for (signal_type_db, _signal_count, contributor_count, weighted_score) in rows {
        let Some(signal_type) = FederatedSignalType::from_db_str(&signal_type_db) else {
            // Unknown variant in the DB: skip rather than leak a raw string.
            continue;
        };
        let explanation = build_explanation(signal_type, contributor_count);
        out.push(NetworkSignalAggregate {
            signal_type,
            contributor_count,
            weighted_score,
            explanation,
        });
    }
    Ok(out)
}

/// Templated, network-only `why this vendor is flagged` sentence. The
/// inputs are *only* the signal type + contributor count: no tenant or
/// vendor identifiers are referenced.
pub fn build_explanation(signal_type: FederatedSignalType, contributor_count: i64) -> String {
    format!(
        "{} other tenants in the network have reported a {} for this vendor within the last 90 days.",
        contributor_count,
        signal_type.display_label()
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SALT: &str = "test-network-salt-do-not-use-in-prod";

    #[test]
    fn vendor_hash_is_stable_for_same_inputs() {
        let a = vendor_hash("acme inc", Some("12-3456789"), Some("last4-9999"), TEST_SALT);
        let b = vendor_hash("acme inc", Some("12-3456789"), Some("last4-9999"), TEST_SALT);
        assert_eq!(a, b, "same inputs must hash deterministically");
    }

    #[test]
    fn vendor_hash_differs_when_tax_id_differs() {
        let a = vendor_hash("acme inc", Some("12-3456789"), None, TEST_SALT);
        let b = vendor_hash("acme inc", Some("99-9999999"), None, TEST_SALT);
        assert_ne!(
            a, b,
            "different tax_id must produce a different vendor hash"
        );
    }

    #[test]
    fn vendor_hash_differs_when_bank_fingerprint_differs() {
        let a = vendor_hash("acme inc", None, Some("last4-1234"), TEST_SALT);
        let b = vendor_hash("acme inc", None, Some("last4-5678"), TEST_SALT);
        assert_ne!(
            a, b,
            "different bank fingerprint must produce a different vendor hash"
        );
    }

    #[test]
    fn vendor_hash_differs_under_different_salt() {
        let a = vendor_hash("acme inc", None, None, "salt-one");
        let b = vendor_hash("acme inc", None, None, "salt-two");
        assert_ne!(a, b, "salt rotation must reshuffle the hash space");
    }

    #[test]
    fn tenant_hmac_is_stable_and_salt_sensitive() {
        let tenant = Uuid::new_v4();
        let a = tenant_hmac(tenant, TEST_SALT);
        let b = tenant_hmac(tenant, TEST_SALT);
        let c = tenant_hmac(tenant, "different-salt");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64, "hex-encoded SHA-256 is 64 chars");
    }

    #[test]
    fn normalize_vendor_name_strips_punctuation_and_lowercases() {
        assert_eq!(normalize_vendor_name("Acme, Inc."), "acme inc");
        assert_eq!(normalize_vendor_name("  ACME   INC  "), "acme inc");
        assert_eq!(normalize_vendor_name("O'Brien & Co."), "o brien co");
    }

    #[test]
    fn build_explanation_grounded_only_in_signal_type_and_count() {
        let msg = build_explanation(FederatedSignalType::OfacNearMatch, 7);
        assert!(msg.contains("7"), "must include contributor count");
        assert!(msg.contains("OFAC near-match"), "must include signal label");
        // Guard against accidental leakage of tenant-specific terms.
        assert!(!msg.to_lowercase().contains("tenant_"));
        assert!(!msg.to_lowercase().contains("vendor_id"));
    }

    #[test]
    fn federated_signal_type_db_roundtrip_covers_all_variants() {
        let all = [
            FederatedSignalType::BankAccountChange,
            FederatedSignalType::OfacNearMatch,
            FederatedSignalType::FakeInvoicePattern,
            FederatedSignalType::DisputeRateHigh,
        ];
        for v in all {
            let s = v.as_db_str();
            assert_eq!(FederatedSignalType::from_db_str(s), Some(v));
        }
        assert_eq!(FederatedSignalType::from_db_str("not_a_signal"), None);
    }
}
