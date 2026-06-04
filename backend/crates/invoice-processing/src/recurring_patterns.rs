//! Recurring-invoice detection and pattern-based auto-approval.
//!
//! Detects vendor + cadence patterns from historical approved invoices and
//! evaluates whether a new invoice matches an existing pattern within
//! configurable tolerances (amount, line items, arrival window).

use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Maximum coefficient-of-variation (stdev / mean) allowed for cadence
/// intervals before the pattern is considered too irregular to detect.
const CADENCE_CV_THRESHOLD: f64 = 0.3;

/// Minimum number of historical approved invoices required to establish a pattern.
const MIN_SAMPLE_COUNT: usize = 3;

/// Number of recent approved invoices to inspect per vendor.
const SAMPLE_WINDOW: i64 = 6;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A detected or persisted recurring pattern row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RecurringPattern {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vendor_id: Uuid,
    pub cadence_days: i32,
    pub trailing_median_cents: i64,
    pub sample_count: i32,
    pub last_invoice_date: Option<NaiveDate>,
    pub last_line_items_hash: Option<String>,
    pub last_line_items_signature: Option<serde_json::Value>,
    pub line_item_tolerance_pct: f64,
    pub auto_approve_enabled: bool,
    pub amount_tolerance_pct: f64,
    pub window_tolerance_days: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Outcome of evaluating an invoice against a pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", content = "reason", rename_all = "snake_case")]
pub enum PatternMatchResult {
    Eligible,
    Ineligible(String),
}

/// Lightweight historical invoice row used for cadence detection.
#[derive(Debug, Clone, sqlx::FromRow)]
struct HistoryRow {
    invoice_date: Option<NaiveDate>,
    total_amount_cents: i64,
    line_items: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Inspect the last `SAMPLE_WINDOW` approved invoices for the given
/// tenant+vendor and compute/update a recurring pattern if:
/// - At least `MIN_SAMPLE_COUNT` invoices exist.
/// - The cadence (median interval between invoice dates) has a coefficient of
///   variation below `CADENCE_CV_THRESHOLD`.
///
/// Returns `None` if there are not enough samples or the cadence is too irregular.
pub async fn detect_or_update_pattern(
    pool: &PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
) -> Result<Option<RecurringPattern>> {
    let rows = sqlx::query_as::<_, HistoryRow>(
        r#"SELECT invoice_date, total_amount_cents, line_items
           FROM invoices
           WHERE tenant_id = $1
             AND vendor_id = $2
             AND processing_status = 'approved'
             AND invoice_date IS NOT NULL
           ORDER BY invoice_date DESC
           LIMIT $3"#,
    )
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(SAMPLE_WINDOW)
    .fetch_all(pool)
    .await?;

    if rows.len() < MIN_SAMPLE_COUNT {
        return Ok(None);
    }

    // Collect invoice dates (ascending) for cadence computation.
    let mut dates: Vec<NaiveDate> = rows
        .iter()
        .filter_map(|r| r.invoice_date)
        .collect();
    dates.sort();

    if dates.len() < MIN_SAMPLE_COUNT {
        return Ok(None);
    }

    // Compute intervals in days between consecutive dates.
    let intervals: Vec<i64> = dates
        .windows(2)
        .map(|w| (w[1] - w[0]).num_days())
        .collect();

    let cadence_days = median_i64(&intervals) as i32;

    // Check cadence regularity: CV = stdev / mean.
    let mean = intervals.iter().sum::<i64>() as f64 / intervals.len() as f64;
    if mean <= 0.0 {
        return Ok(None);
    }
    let variance = intervals
        .iter()
        .map(|v| (*v as f64 - mean).powi(2))
        .sum::<f64>()
        / intervals.len() as f64;
    let stdev = variance.sqrt();
    let cv = stdev / mean;

    if cv > CADENCE_CV_THRESHOLD {
        return Ok(None);
    }

    // Trailing median amount (in cents).
    let mut amounts: Vec<i64> = rows.iter().map(|r| r.total_amount_cents).collect();
    amounts.sort();
    let trailing_median_cents = median_i64(&amounts);

    // Line-items hash of the most recent invoice (rows[0] = newest).
    let last_line_items_hash = hash_line_items(&rows[0].line_items);
    let last_line_items_signature =
        serde_json::to_value(compute_line_items_signature(&rows[0].line_items)).ok();
    let last_invoice_date = rows[0].invoice_date;
    let sample_count = rows.len() as i32;

    // Upsert pattern: create or update.
    let pattern = sqlx::query_as::<_, RecurringPattern>(
        r#"INSERT INTO recurring_patterns
               (tenant_id, vendor_id, cadence_days, trailing_median_cents,
                sample_count, last_invoice_date, last_line_items_hash,
                last_line_items_signature)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           ON CONFLICT (tenant_id, vendor_id) DO UPDATE
               SET cadence_days              = EXCLUDED.cadence_days,
                   trailing_median_cents     = EXCLUDED.trailing_median_cents,
                   sample_count              = EXCLUDED.sample_count,
                   last_invoice_date         = EXCLUDED.last_invoice_date,
                   last_line_items_hash      = EXCLUDED.last_line_items_hash,
                   last_line_items_signature = EXCLUDED.last_line_items_signature,
                   updated_at                = NOW()
           RETURNING id, tenant_id, vendor_id, cadence_days,
                     trailing_median_cents, sample_count,
                     last_invoice_date, last_line_items_hash,
                     last_line_items_signature,
                     CAST(line_item_tolerance_pct AS DOUBLE PRECISION) AS line_item_tolerance_pct,
                     auto_approve_enabled,
                     CAST(amount_tolerance_pct AS DOUBLE PRECISION) AS amount_tolerance_pct,
                     window_tolerance_days,
                     created_at, updated_at"#,
    )
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(cadence_days)
    .bind(trailing_median_cents)
    .bind(sample_count)
    .bind(last_invoice_date)
    .bind(&last_line_items_hash)
    .bind(&last_line_items_signature)
    .fetch_one(pool)
    .await?;

    Ok(Some(pattern))
}

// ---------------------------------------------------------------------------
// Match evaluation
// ---------------------------------------------------------------------------

/// Evaluate whether an incoming invoice matches the given recurring pattern.
///
/// Checks:
/// 1. Amount within +/-`amount_tolerance_pct` of `trailing_median_cents`.
/// 2. Line-items hash equals `last_line_items_hash`.
/// 3. Invoice date within +/-`window_tolerance_days` of the expected date
///    (`last_invoice_date + cadence_days`).
pub fn evaluate_pattern_match(
    invoice_amount_cents: i64,
    invoice_date: Option<NaiveDate>,
    invoice_line_items: &serde_json::Value,
    pattern: &RecurringPattern,
) -> PatternMatchResult {
    // (a) Amount tolerance check.
    let median = pattern.trailing_median_cents as f64;
    let tolerance = pattern.amount_tolerance_pct;
    let lower = median * (1.0 - tolerance / 100.0);
    let upper = median * (1.0 + tolerance / 100.0);
    let amount = invoice_amount_cents as f64;
    if amount < lower || amount > upper {
        return PatternMatchResult::Ineligible(format!(
            "Amount {} outside tolerance [{:.0}, {:.0}] (median {} ±{}%)",
            invoice_amount_cents,
            lower,
            upper,
            pattern.trailing_median_cents,
            tolerance
        ));
    }

    // (b) Line-items check: use structured signature comparison when available,
    //     fall back to hash comparison for legacy rows.
    if let Some(ref expected_signature) = pattern.last_line_items_signature {
        let current_signature =
            serde_json::to_value(compute_line_items_signature(invoice_line_items)).ok();
        if let Some(ref curr) = current_signature {
            if !signatures_match_within_tolerance(expected_signature, curr, pattern.line_item_tolerance_pct) {
                return PatternMatchResult::Ineligible(
                    "Line items changed since last pattern sample".to_string(),
                );
            }
        }
    } else if let Some(ref expected_hash) = pattern.last_line_items_hash {
        // Legacy row: fall back to strict hash comparison.
        let actual_hash = hash_line_items(invoice_line_items);
        if actual_hash != *expected_hash {
            return PatternMatchResult::Ineligible(
                "Line items changed since last pattern sample".to_string(),
            );
        }
    }

    // (c) Arrival window check.
    if let Some(inv_date) = invoice_date {
        if let Some(last_date) = pattern.last_invoice_date {
            let expected = last_date + chrono::Duration::days(pattern.cadence_days as i64);
            let delta = (inv_date - expected).num_days().abs();
            if delta > pattern.window_tolerance_days as i64 {
                return PatternMatchResult::Ineligible(format!(
                    "Invoice date {} is {} day(s) from expected {} (window ±{} days)",
                    inv_date, delta, expected, pattern.window_tolerance_days
                ));
            }
        }
    }

    PatternMatchResult::Eligible
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Look up the recurring pattern for a tenant+vendor pair.
pub async fn find_pattern(
    pool: &PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
) -> Result<Option<RecurringPattern>> {
    let pattern = sqlx::query_as::<_, RecurringPattern>(
        r#"SELECT id, tenant_id, vendor_id, cadence_days,
                  trailing_median_cents, sample_count,
                  last_invoice_date, last_line_items_hash,
                  last_line_items_signature,
                  CAST(line_item_tolerance_pct AS DOUBLE PRECISION) AS line_item_tolerance_pct,
                  auto_approve_enabled,
                  CAST(amount_tolerance_pct AS DOUBLE PRECISION) AS amount_tolerance_pct,
                  window_tolerance_days,
                  created_at, updated_at
           FROM recurring_patterns
           WHERE tenant_id = $1 AND vendor_id = $2"#,
    )
    .bind(tenant_id)
    .bind(vendor_id)
    .fetch_optional(pool)
    .await?;

    Ok(pattern)
}

/// Compute the SHA-256 hex digest of line-item JSON.
fn hash_line_items(items: &serde_json::Value) -> String {
    // Normalize: sort array by description for deterministic hashing.
    let normalized = normalize_line_items(items);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Sort line items by description for stable hashing.
fn normalize_line_items(items: &serde_json::Value) -> String {
    match items.as_array() {
        Some(arr) => {
            let mut sorted = arr.clone();
            sorted.sort_by(|a, b| {
                let da = a.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let db = b.get("description").and_then(|v| v.as_str()).unwrap_or("");
                da.cmp(db)
            });
            serde_json::to_string(&sorted).unwrap_or_else(|_| items.to_string())
        }
        None => items.to_string(),
    }
}

/// Normalize a description string: lowercase, strip non-alphanumeric, collapse whitespace.
fn normalize_description(desc: &str) -> String {
    let lower = desc.to_lowercase();
    let stripped: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut result = String::with_capacity(stripped.len());
    let mut prev_space = true; // trim leading spaces
    for c in stripped.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    let trimmed = result.trim_end();
    trimmed.to_string()
}

/// Compute a structured signature from line-item JSON: aggregate amounts by
/// normalized description, then sort by key. Returns `Vec<(String, i64)>`
/// where each entry is `(normalized_description, total_amount_cents)`.
fn compute_line_items_signature(items: &serde_json::Value) -> Vec<(String, i64)> {
    let arr = match items.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for item in arr {
        let desc = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let key = normalize_description(desc);
        // Accept both "amount" (cents integer) and "amount_cents".
        let cents = item
            .get("amount_cents")
            .and_then(|v| v.as_i64())
            .or_else(|| item.get("amount").and_then(|v| v.as_i64()))
            .unwrap_or(0);
        *map.entry(key).or_insert(0) += cents;
    }

    map.into_iter().collect()
}

/// Check whether two line-item signatures match within tolerance.
///
/// Two signatures match when:
/// 1. They contain the same set of normalized description keys.
/// 2. Each paired amount is within `tolerance_pct` percent **or** within 1 cent absolute.
fn signatures_match_within_tolerance(
    prev: &serde_json::Value,
    curr: &serde_json::Value,
    tolerance_pct: f64,
) -> bool {
    let prev_sig: Vec<(String, i64)> = match serde_json::from_value(prev.clone()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let curr_sig: Vec<(String, i64)> = match serde_json::from_value(curr.clone()) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if prev_sig.len() != curr_sig.len() {
        return false;
    }

    for (i, (p_desc, p_amt)) in prev_sig.iter().enumerate() {
        let (c_desc, c_amt) = &curr_sig[i];
        if p_desc != c_desc {
            return false;
        }
        // Within tolerance percent OR within 1 cent absolute.
        let delta = (*p_amt - *c_amt).abs() as f64;
        if delta > 1.0 {
            let max_delta = (*p_amt as f64) * tolerance_pct;
            if delta > max_delta {
                return false;
            }
        }
    }

    true
}

/// Median of a sorted slice of i64 values.
fn median_i64(sorted: &[i64]) -> i64 {
    assert!(!sorted.is_empty(), "cannot compute median of empty slice");
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(
        cadence_days: i32,
        trailing_median_cents: i64,
        amount_tolerance_pct: f64,
        window_tolerance_days: i32,
        last_invoice_date: Option<NaiveDate>,
        last_line_items_hash: Option<String>,
    ) -> RecurringPattern {
        RecurringPattern {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            vendor_id: Uuid::new_v4(),
            cadence_days,
            trailing_median_cents,
            sample_count: 5,
            last_invoice_date,
            last_line_items_hash,
            last_line_items_signature: None,
            line_item_tolerance_pct: 0.05,
            auto_approve_enabled: true,
            amount_tolerance_pct,
            window_tolerance_days,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    // -- Cadence computation tests --

    #[test]
    fn median_i64_odd_count() {
        let v = vec![10, 20, 30];
        assert_eq!(median_i64(&v), 20);
    }

    #[test]
    fn median_i64_even_count() {
        let v = vec![10, 20, 30, 40];
        assert_eq!(median_i64(&v), 25);
    }

    // -- Amount tolerance tests --

    #[test]
    fn amount_within_tolerance_is_eligible() {
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, None);
        // 100.00 ± 5% = [95.00, 105.00]
        let result = evaluate_pattern_match(100_00, None, &serde_json::json!([]), &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn amount_at_upper_boundary_is_eligible() {
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, None);
        // Upper bound: 105.00
        let result = evaluate_pattern_match(105_00, None, &serde_json::json!([]), &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn amount_above_tolerance_is_ineligible() {
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, None);
        // 106.00 is outside [95.00, 105.00]
        let result = evaluate_pattern_match(106_00, None, &serde_json::json!([]), &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(
                    reason.contains("Amount"),
                    "Expected amount-based rejection, got: {}",
                    reason
                );
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    #[test]
    fn amount_below_tolerance_is_ineligible() {
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, None);
        // 94.00 is outside [95.00, 105.00]
        let result = evaluate_pattern_match(94_00, None, &serde_json::json!([]), &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(reason.contains("Amount"));
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    // -- Line-items hash tests --

    #[test]
    fn line_items_unchanged_is_eligible() {
        let items = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        let hash = hash_line_items(&items);
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, Some(hash));
        let result = evaluate_pattern_match(100_00, None, &items, &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn line_items_changed_is_ineligible() {
        let original = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        let modified = serde_json::json!([
            {"description": "Rent", "amount": 110_00}
        ]);
        let hash = hash_line_items(&original);
        let pattern = make_pattern(30, 100_00, 5.0, 3, None, Some(hash));
        let result = evaluate_pattern_match(100_00, None, &modified, &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(reason.contains("Line items"));
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    // -- Window tolerance tests --

    #[test]
    fn date_within_window_is_eligible() {
        let last_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let expected = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(); // last + 31 days cadence
        let pattern = make_pattern(31, 100_00, 5.0, 3, Some(last_date), None);
        // Invoice on expected date
        let result = evaluate_pattern_match(
            100_00,
            Some(expected),
            &serde_json::json!([]),
            &pattern,
        );
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn date_within_window_plus_2_days_is_eligible() {
        let last_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let pattern = make_pattern(30, 100_00, 5.0, 3, Some(last_date), None);
        // Expected: 2026-05-31, actual: 2026-06-02 (2 days late, within ±3)
        let inv_date = NaiveDate::from_ymd_opt(2026, 6, 2).unwrap();
        let result =
            evaluate_pattern_match(100_00, Some(inv_date), &serde_json::json!([]), &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn date_outside_window_is_ineligible() {
        let last_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let pattern = make_pattern(30, 100_00, 5.0, 3, Some(last_date), None);
        // Expected: 2026-05-31, actual: 2026-06-05 (5 days late, outside ±3)
        let inv_date = NaiveDate::from_ymd_opt(2026, 6, 5).unwrap();
        let result =
            evaluate_pattern_match(100_00, Some(inv_date), &serde_json::json!([]), &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(reason.contains("Invoice date"), "Got: {}", reason);
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    // -- Normalization tests --

    #[test]
    fn normalize_sorts_by_description() {
        let items = serde_json::json!([
            {"description": "Zebra", "amount": 10},
            {"description": "Alpha", "amount": 20}
        ]);
        let normalized = normalize_line_items(&items);
        let parsed: serde_json::Value = serde_json::from_str(&normalized).unwrap();
        let first = parsed[0].get("description").unwrap().as_str().unwrap();
        assert_eq!(first, "Alpha");
    }

    #[test]
    fn hash_line_items_deterministic() {
        let items = serde_json::json!([
            {"description": "Service", "amount": 100}
        ]);
        let h1 = hash_line_items(&items);
        let h2 = hash_line_items(&items);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    // -- Helper for signature-based pattern tests --

    fn make_pattern_with_signature(
        cadence_days: i32,
        trailing_median_cents: i64,
        amount_tolerance_pct: f64,
        window_tolerance_days: i32,
        last_invoice_date: Option<NaiveDate>,
        signature_items: &serde_json::Value,
        line_item_tolerance_pct: f64,
    ) -> RecurringPattern {
        let sig = serde_json::to_value(compute_line_items_signature(signature_items)).ok();
        RecurringPattern {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            vendor_id: Uuid::new_v4(),
            cadence_days,
            trailing_median_cents,
            sample_count: 5,
            last_invoice_date,
            last_line_items_hash: None,
            last_line_items_signature: sig,
            line_item_tolerance_pct,
            auto_approve_enabled: true,
            amount_tolerance_pct,
            window_tolerance_days,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    // -- Signature-based line-item tests --

    #[test]
    fn line_items_with_minor_wording_change_still_eligible() {
        let original = serde_json::json!([
            {"description": "Monthly Rent - Suite 200", "amount": 100_00}
        ]);
        // OCR introduced case change, punctuation differences, extra spacing
        let ocr_variant = serde_json::json!([
            {"description": "monthly rent   suite 200", "amount": 100_00}
        ]);
        let pattern =
            make_pattern_with_signature(30, 100_00, 5.0, 3, None, &original, 0.05);
        let result = evaluate_pattern_match(100_00, None, &ocr_variant, &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn line_items_split_into_multiple_rows_still_eligible() {
        let original = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        // One $100 row split into two $50 rows with same normalized description
        let split = serde_json::json!([
            {"description": "Rent", "amount": 50_00},
            {"description": "Rent", "amount": 50_00}
        ]);
        let pattern =
            make_pattern_with_signature(30, 100_00, 5.0, 3, None, &original, 0.05);
        let result = evaluate_pattern_match(100_00, None, &split, &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn line_items_with_sub_tolerance_amount_drift_still_eligible() {
        let original = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        // $100.00 -> $100.30 (0.3% drift, well within 5% tolerance)
        let drifted = serde_json::json!([
            {"description": "Rent", "amount": 100_30}
        ]);
        let pattern =
            make_pattern_with_signature(30, 100_00, 5.0, 3, None, &original, 0.05);
        let result = evaluate_pattern_match(100_00, None, &drifted, &pattern);
        assert_eq!(result, PatternMatchResult::Eligible);
    }

    #[test]
    fn line_items_with_real_change_still_ineligible() {
        let original = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        // Completely different description — real change
        let changed = serde_json::json!([
            {"description": "Parking", "amount": 100_00}
        ]);
        let pattern =
            make_pattern_with_signature(30, 100_00, 5.0, 3, None, &original, 0.05);
        let result = evaluate_pattern_match(100_00, None, &changed, &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(reason.contains("Line items"));
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    #[test]
    fn line_items_with_amount_exceeding_tolerance_still_ineligible() {
        let original = serde_json::json!([
            {"description": "Rent", "amount": 100_00}
        ]);
        // $100.00 -> $110.00 (10% drift, exceeds 5% tolerance)
        let changed = serde_json::json!([
            {"description": "Rent", "amount": 110_00}
        ]);
        let pattern =
            make_pattern_with_signature(30, 100_00, 5.0, 3, None, &original, 0.05);
        let result = evaluate_pattern_match(100_00, None, &changed, &pattern);
        match result {
            PatternMatchResult::Ineligible(reason) => {
                assert!(reason.contains("Line items"));
            }
            PatternMatchResult::Eligible => panic!("Should be ineligible"),
        }
    }

    // -- Signature unit tests --

    #[test]
    fn compute_signature_aggregates_by_normalized_description() {
        let items = serde_json::json!([
            {"description": "Rent", "amount": 50_00},
            {"description": "rent!", "amount": 50_00}
        ]);
        let sig = compute_line_items_signature(&items);
        assert_eq!(sig.len(), 1);
        assert_eq!(sig[0].0, "rent");
        assert_eq!(sig[0].1, 100_00);
    }

    #[test]
    fn normalize_description_strips_punctuation_and_case() {
        assert_eq!(normalize_description("Monthly Rent - Suite #200"), "monthly rent suite 200");
        assert_eq!(normalize_description("  WEB   HOSTING  "), "web hosting");
    }
}
