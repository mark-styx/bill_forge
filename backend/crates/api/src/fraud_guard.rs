//! Fraud-guard checks for vendor creation and banking-detail changes.
//!
//! Four signals are computed:
//!   - **domain_age**: How long ago the vendor's email/website domain was first seen
//!     in this tenant. < 30 days = `high`, 30-180 = `medium`, else `low`.
//!   - **lookalike**: OCR-aware Levenshtein similarity against every active vendor
//!     name in the tenant. Top match >= 0.85 similarity = `high`.
//!   - **bank_change**: Whether this vendor had another banking-detail change in the
//!     last 30 days. If yes = `high`.
//!   - **country_mismatch**: ISO country code comparison between vendor address and
//!     bank account country. Mismatch = `high`, missing data = `unknown`.
//!
//! All checks are deterministic and tenant-local (no external WHOIS calls).
//! Results are stored in the existing `screening_results` JSONB column.

use billforge_core::text_similarity::ocr_levenshtein_similarity;
use billforge_core::types::TenantId;
use billforge_core::domain::VendorId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Risk level
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Unknown,
}

impl RiskLevel {
    /// Numeric ordering for `max()`: Unknown is treated as lower than Low
    /// so it never inflates the overall risk.
    fn rank(self) -> u8 {
        match self {
            RiskLevel::Unknown => 0,
            RiskLevel::Low => 1,
            RiskLevel::Medium => 2,
            RiskLevel::High => 3,
        }
    }

    pub fn max(a: Self, b: Self) -> Self {
        if a.rank() >= b.rank() { a } else { b }
    }
}

// ---------------------------------------------------------------------------
// Individual signal structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainAgeSignal {
    pub risk: RiskLevel,
    pub domain: String,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub days_since_first_seen: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookalikeSignal {
    pub risk: RiskLevel,
    pub top_match: Option<LookalikeMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookalikeMatch {
    pub vendor_id: String,
    pub vendor_name: String,
    pub similarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankChangeSignal {
    pub risk: RiskLevel,
    pub recent_changes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountrySignal {
    pub risk: RiskLevel,
    pub vendor_country: Option<String>,
    pub bank_country: Option<String>,
}

// ---------------------------------------------------------------------------
// Aggregate result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudSignals {
    pub domain_age: DomainAgeSignal,
    pub lookalike: LookalikeSignal,
    pub bank_change: BankChangeSignal,
    pub country_mismatch: CountrySignal,
    pub overall_risk: RiskLevel,
}

// ---------------------------------------------------------------------------
// Individual checks (pure / async functions)
// ---------------------------------------------------------------------------

/// Query `vendor_domain_first_seen` for the domain.
/// < 30 days old → high, 30-180 → medium, else → low.
/// Never-seen → high (brand-new domain).
pub async fn check_domain_age(
    tenant_id: &TenantId,
    domain: &str,
    pool: &PgPool,
) -> DomainAgeSignal {
    if domain.is_empty() {
        return DomainAgeSignal {
            risk: RiskLevel::Unknown,
            domain: String::new(),
            first_seen_at: None,
            days_since_first_seen: None,
        };
    }

    let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT first_seen_at FROM vendor_domain_first_seen WHERE tenant_id = $1 AND domain = $2",
    )
    .bind(*tenant_id.as_uuid())
    .bind(domain)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((first_seen,)) => {
            let days = (Utc::now() - first_seen).num_days().max(0);
            let risk = if days < 30 {
                RiskLevel::High
            } else if days < 180 {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            };
            DomainAgeSignal {
                risk,
                domain: domain.to_string(),
                first_seen_at: Some(first_seen),
                days_since_first_seen: Some(days),
            }
        }
        None => {
            // Brand-new domain never seen in this tenant
            DomainAgeSignal {
                risk: RiskLevel::High,
                domain: domain.to_string(),
                first_seen_at: None,
                days_since_first_seen: None,
            }
        }
    }
}

/// Query all active vendor names for the tenant, compute OCR-aware Levenshtein
/// similarity, and return the top match if similarity >= 0.85.
pub async fn check_lookalike_vendor(
    tenant_id: &TenantId,
    candidate_name: &str,
    exclude_vendor_id: Option<&VendorId>,
    pool: &PgPool,
) -> LookalikeSignal {
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM vendors WHERE tenant_id = $1 AND status = 'active'",
    )
    .bind(*tenant_id.as_uuid())
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let candidate_upper = candidate_name.to_uppercase();
    let mut best: Option<LookalikeMatch> = None;

    for (vid, vname) in &rows {
        // Skip the vendor being checked (so we don't match against ourselves)
        if let Some(exclude) = exclude_vendor_id {
            if *vid == exclude.0 {
                continue;
            }
        }
        let sim = ocr_levenshtein_similarity(&candidate_upper, &vname.to_uppercase());
        if sim >= 0.85 {
            match &best {
                Some(b) if b.similarity >= sim => {}
                _ => {
                    best = Some(LookalikeMatch {
                        vendor_id: vid.to_string(),
                        vendor_name: vname.clone(),
                        similarity: sim,
                    });
                }
            }
        }
    }

    let risk = if best.is_some() { RiskLevel::High } else { RiskLevel::Low };
    LookalikeSignal { risk, top_match: best }
}

/// Check `vendor_banking_verifications` for more than one row in the last 30 days
/// for this vendor. Uses a `> 1` threshold (not `> 0`) because the current change
/// row is already inserted before this check runs; only a *prior* recent change
/// constitutes a fraud signal.
pub async fn check_recent_bank_change(
    tenant_id: &TenantId,
    vendor_id: &VendorId,
    pool: &PgPool,
) -> BankChangeSignal {
    let count: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM vendor_banking_verifications
           WHERE tenant_id = $1 AND vendor_id = $2
             AND requested_at > NOW() - INTERVAL '30 days'"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id.0)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    let recent = count.0;
    // Threshold on > 1, not > 0: the current banking-change row is already
    // inserted by the time this check runs, so a count of 1 is normal.
    // Only flag high when there was a *prior* change within 30 days.
    let risk = if recent > 1 { RiskLevel::High } else { RiskLevel::Low };
    BankChangeSignal { risk, recent_changes: recent }
}

/// Direct ISO country code comparison. Mismatch = high, missing data = unknown.
pub fn check_country_mismatch(
    vendor_country: Option<&str>,
    bank_country: Option<&str>,
) -> CountrySignal {
    let vc = vendor_country.map(|s| s.to_string());
    let bc = bank_country.map(|s| s.to_string());

    let risk = match (&vc, &bc) {
        (Some(v), Some(b)) if !v.is_empty() && !b.is_empty() => {
            if v.eq_ignore_ascii_case(b) {
                RiskLevel::Low
            } else {
                RiskLevel::High
            }
        }
        _ => RiskLevel::Unknown,
    };

    CountrySignal {
        risk,
        vendor_country: vc,
        bank_country: bc,
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Run all four fraud-guard checks and return the aggregate `FraudSignals`.
///
/// `vendor_id` is `None` during vendor creation (no row exists yet).
/// `domain` is extracted from the vendor's email or website.
/// `vendor_country` / `bank_country` are ISO codes (may be empty/None).
pub async fn run_fraud_guard(
    tenant_id: &TenantId,
    vendor_id: Option<&VendorId>,
    vendor_name: &str,
    domain: &str,
    vendor_country: Option<&str>,
    bank_country: Option<&str>,
    pool: &PgPool,
) -> FraudSignals {
    let domain_age = check_domain_age(tenant_id, domain, pool).await;
    let lookalike = check_lookalike_vendor(tenant_id, vendor_name, vendor_id, pool).await;

    let bank_change = if let Some(vid) = vendor_id {
        check_recent_bank_change(tenant_id, vid, pool).await
    } else {
        // No vendor row yet — cannot have prior bank changes
        BankChangeSignal { risk: RiskLevel::Low, recent_changes: 0 }
    };

    let country_mismatch = check_country_mismatch(vendor_country, bank_country);

    let overall_risk = [
        domain_age.risk,
        lookalike.risk,
        bank_change.risk,
        country_mismatch.risk,
    ]
    .into_iter()
    .fold(RiskLevel::Low, RiskLevel::max);

    FraudSignals {
        domain_age,
        lookalike,
        bank_change,
        country_mismatch,
        overall_risk,
    }
}

// ---------------------------------------------------------------------------
// Helpers for persisting domain and converting to JSON
// ---------------------------------------------------------------------------

/// Upsert `(tenant_id, domain, now())` into `vendor_domain_first_seen`.
/// `ON CONFLICT DO NOTHING` keeps the original first-seen timestamp.
pub async fn upsert_domain_first_seen(
    tenant_id: &TenantId,
    domain: &str,
    pool: &PgPool,
) {
    if domain.is_empty() {
        return;
    }
    let _ = sqlx::query(
        r#"INSERT INTO vendor_domain_first_seen (tenant_id, domain, first_seen_at)
           VALUES ($1, $2, NOW())
           ON CONFLICT (tenant_id, domain) DO NOTHING"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(domain)
    .execute(pool)
    .await;
}

/// Extract domain from an email address (part after @) or return the string
/// as-is if it looks like a domain already. Returns empty string for None.
pub fn extract_domain(email: Option<&str>, website: Option<&str>) -> String {
    // Prefer email domain
    if let Some(email) = email {
        if let Some(at_pos) = email.rfind('@') {
            let domain = &email[at_pos + 1..];
            if !domain.is_empty() {
                return domain.to_lowercase();
            }
        }
    }
    // Fall back to website
    if let Some(site) = website {
        let stripped = site
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("www.");
        let domain = stripped.split('/').next().unwrap_or("");
        return domain.to_lowercase();
    }
    String::new()
}

/// Convert `FraudSignals` into a `serde_json::Value` that merges with the
/// existing screening stubs (ofac/avs/plaid).
pub fn fraud_signals_to_json(signals: &FraudSignals) -> serde_json::Value {
    serde_json::to_value(signals).unwrap_or_default()
}

/// Merge fraud-guard signals with the legacy screening stubs into a single
/// JSON value suitable for `screening_results`.
pub fn build_screening_results(signals: &FraudSignals) -> serde_json::Value {
    let now = Utc::now().to_rfc3339();
    let mut map = serde_json::Map::new();

    // Legacy stubs (kept for API contract compatibility)
    map.insert(
        "ofac".to_string(),
        serde_json::json!({ "status": "pass", "checked_at": now }),
    );
    map.insert(
        "avs".to_string(),
        serde_json::json!({ "status": "pass", "checked_at": now }),
    );
    map.insert(
        "plaid".to_string(),
        serde_json::json!({ "status": "pass", "checked_at": now }),
    );

    // Fraud-guard signals
    map.insert("domain_age".to_string(), serde_json::to_value(&signals.domain_age).unwrap_or_default());
    map.insert("lookalike".to_string(), serde_json::to_value(&signals.lookalike).unwrap_or_default());
    map.insert("bank_change".to_string(), serde_json::to_value(&signals.bank_change).unwrap_or_default());
    map.insert("country_mismatch".to_string(), serde_json::to_value(&signals.country_mismatch).unwrap_or_default());
    map.insert("overall_risk".to_string(), serde_json::to_value(&signals.overall_risk).unwrap_or_default());

    serde_json::Value::Object(map)
}
