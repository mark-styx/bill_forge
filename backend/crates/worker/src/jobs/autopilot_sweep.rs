//! Autopilot Sweep Job (refs #379)
//!
//! Background pass that auto-resolves queued exceptions when their proposed
//! resolution confidence is at or above the tenant's `autopilot_threshold` AND
//! the exception_type is in the tenant's `autopilot_enabled_types` list.
//!
//! When `autopilot_enabled_types` is **absent** from settings the sweep defaults
//! to the full set of types it currently supports, so a tenant who has never
//! touched settings still benefits from auto-resolve. When the key is **present
//! but an empty array** the tenant has explicitly opted out and the sweep skips
//! them.
//!
//! This is NOT a new ML model: the per-exception confidence is the score the
//! existing detector already emits (OCR confidence, duplicate similarity,
//! PO-match score, categorization confidence, policy severity). The cockpit
//! is a unification + UX layer, not a new inference path.
//!
//! The sweep deliberately mirrors the deterministic proposal-builder that
//! powers GET /autopilot/queue, so what a human sees in the cockpit is exactly
//! what the sweep would have auto-resolved.

use anyhow::{Context, Result};
use tracing::{info, warn};

use billforge_core::TenantId;
use billforge_db::PgManager;
use std::sync::Arc;

/// Run the autopilot sweep for a single validated tenant.
pub async fn run_tenant_autopilot_sweep(
    pg_manager: Arc<PgManager>,
    tenant_id: &TenantId,
) -> Result<()> {
    let tenant_id_str = tenant_id.as_str();
    info!(tenant_id = %tenant_id_str, "Autopilot sweep starting");

    let pool = pg_manager.tenant(tenant_id).await?;

    // Read per-tenant settings from tenants.settings JSONB on the metadata DB.
    let settings_json: serde_json::Value =
        sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_optional(pg_manager.metadata())
            .await
            .context("Failed to read tenant settings")?
            .unwrap_or_else(|| serde_json::json!({}));

    let threshold: f32 = settings_json
        .get("autopilot_threshold")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.95);
    // When the key is absent, default to every type the sweep supports today
    // so a tenant who has never written settings still gets auto-resolve.
    // When the key is present but empty, the tenant has explicitly opted out.
    let enabled: Vec<String> = match settings_json.get("autopilot_enabled_types") {
        Some(v) => v
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        None => vec![
            "ocr_low_confidence".to_string(),
            "gl_ambiguity".to_string(),
            "missing_po".to_string(),
            "vendor_mismatch".to_string(),
        ],
    };

    // Empty enabled list = tenant has opted out of auto-resolve entirely.
    if enabled.is_empty() || threshold <= 0.0 {
        info!(
            tenant_id = %tenant_id_str,
            threshold, enabled_count = enabled.len(),
            "Autopilot sweep skipping tenant (opted out)"
        );
        return Ok(());
    }

    let mut auto_resolved: u64 = 0;
    for exception_type in &enabled {
        auto_resolved += auto_resolve_tenant_exceptions(
            &pool,
            tenant_id.as_uuid(),
            exception_type,
            threshold,
        )
        .await
        .map_err(|e| anyhow::anyhow!("auto-resolve failed for {}: {}", exception_type, e))?;
    }

    info!(
        tenant_id = %tenant_id_str,
        auto_resolved, threshold,
        "Autopilot sweep completed"
    );
    Ok(())
}

/// For one exception_type, find queued exceptions at or above the threshold
/// that have not yet been decided, and write an `auto_resolved` row for each.
///
/// The actual side-effect on the underlying invoice (advancing OCR exception
/// status, flagging a duplicate, etc.) is intentionally minimal here: the
/// sweep records the decision, which is what drives the Daily Report. The
/// downstream engines read autopilot_decisions to decide whether to skip a
/// now-resolved exception.
async fn auto_resolve_tenant_exceptions(
    pool: &sqlx::PgPool,
    tenant_id: &uuid::Uuid,
    exception_type: &str,
    threshold: f32,
) -> Result<u64> {
    // Reuse the same proposal mapping that powers the queue so what the
    // sweep auto-resolves is exactly what a human would have seen as
    // auto_resolve_eligible=true. The SQL mirrors the signal row in
    // billforge_api::routes::autopilot.
    let rows = sqlx::query_as::<_, SignalRow>(
        r#"SELECT id, ocr_confidence, categorization_confidence, po_number, gl_code,
                  ocr_exception_status, vendor_name
           FROM invoices
           WHERE tenant_id = $1
           LIMIT 500"#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .context("Failed to load invoice signals for sweep")?;

    let mut count: u64 = 0;
    for row in rows {
        let conf = match confidence_for(exception_type, &row) {
            Some(c) => c,
            None => continue,
        };
        if conf < threshold {
            continue;
        }
        let composite = format!("{}:{}", exception_type, row.id);

        // INSERT ... ON CONFLICT would be cleaner, but there is no unique
        // constraint on (tenant_id, exception_id). Use NOT EXISTS so the
        // sweep is idempotent across reruns.
        let inserted = sqlx::query(
            r#"INSERT INTO autopilot_decisions
               (tenant_id, exception_id, invoice_id, exception_type, decision, confidence)
               SELECT $1, $2, $3, $4, 'auto_resolved', $5
               WHERE NOT EXISTS (
                   SELECT 1 FROM autopilot_decisions
                   WHERE tenant_id = $1 AND exception_id = $2
               )"#,
        )
        .bind(tenant_id)
        .bind(&composite)
        .bind(row.id)
        .bind(exception_type)
        .bind(conf)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to insert autopilot decision: {}", e))?;

        count += inserted.rows_affected();
    }

    if count > 0 {
        info!(
            tenant_id = %tenant_id,
            exception_type, threshold, count,
            "Autopilot sweep auto-resolved exceptions"
        );
    }
    Ok(count)
}

#[derive(sqlx::FromRow)]
struct SignalRow {
    id: uuid::Uuid,
    ocr_confidence: Option<f32>,
    categorization_confidence: Option<f32>,
    po_number: Option<String>,
    gl_code: Option<String>,
    ocr_exception_status: String,
    vendor_name: String,
}

/// Map an exception_type to the detector-emitted confidence for this invoice.
///
/// This MUST stay byte-for-byte equivalent to the proposal-emission predicates
/// in billforge_api::routes::autopilot::build_proposals. If a predicate there
/// gates a proposal (e.g. `ocr_exception_status == "pending"` for OCR, or
/// `categorization_confidence < 0.80` for GL ambiguity), the same predicate
/// must gate a `Some(confidence)` return here. Otherwise the sweep will write
/// phantom `auto_resolved` rows for invoices that have no actual exception of
/// that type, inflating the Daily Report and pre-claiming the composite
/// exception_id so the queue silently suppresses a real exception of the same
/// type if one later arises.
fn confidence_for(exception_type: &str, row: &SignalRow) -> Option<f32> {
    match exception_type {
        // Mirror build_proposals case 1: only pending-review rows with
        // OCR confidence strictly below 0.90 emit a proposal.
        "ocr_low_confidence" => {
            if row.ocr_exception_status != "pending" {
                return None;
            }
            row.ocr_confidence.filter(|c| *c < 0.90)
        }
        // Mirror build_proposals case 3: a proposal is emitted only when
        // categorization_confidence is present and below 0.80, OR when there
        // is no categorization AND no gl_code yet (the 0.10 fallback).
        "gl_ambiguity" => match row.categorization_confidence {
            Some(cat_conf) if cat_conf < 0.80 => Some(cat_conf.clamp(0.0, 1.0)),
            None if row.gl_code.is_none() => Some(0.10),
            _ => None,
        },
        // Mirror build_proposals case 2: a proposal is emitted only when
        // po_number is None or trims to empty.
        "missing_po" => {
            let po_missing = row
                .po_number
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true);
            if po_missing {
                Some(0.30)
            } else {
                None
            }
        }
        // Mirror build_proposals case 4: a proposal is emitted only when the
        // vendor_name is blank, contains "unknown", or is the placeholder
        // "processing...".
        "vendor_mismatch" => {
            let lower = row.vendor_name.to_lowercase();
            if row.vendor_name.trim().is_empty()
                || lower.contains("unknown")
                || lower == "processing..."
            {
                Some(0.20)
            } else {
                None
            }
        }
        // `duplicate` and `policy_violation` require per-invoice detector
        // state that the sweep does not re-derive (the queue itself defers to
        // load_pending_duplicates_and_policy from invoice_audit_log); the
        // human-in-the-loop cockpit remains the resolution path for them.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn confidence_for_ocr_returns_low_confidence_only() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.55),
            categorization_confidence: Some(0.99),
            po_number: None,
            gl_code: None,
            ocr_exception_status: "pending".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("ocr_low_confidence", &row), Some(0.55));
    }

    #[test]
    fn confidence_for_ocr_returns_none_when_high_confidence() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: None,
            po_number: None,
            gl_code: None,
            ocr_exception_status: "pending".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("ocr_low_confidence", &row), None);
    }

    #[test]
    fn confidence_for_ocr_returns_none_when_not_pending() {
        // Mirrors build_proposals: an OCR exception already resolved must NOT
        // be re-emitted by the sweep even if ocr_confidence is still low,
        // otherwise the sweep would write a phantom auto_resolved row.
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.55),
            categorization_confidence: Some(0.99),
            po_number: None,
            gl_code: None,
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("ocr_low_confidence", &row), None);
    }

    #[test]
    fn confidence_for_gl_returns_categorization() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.40),
            po_number: None,
            gl_code: None,
            ocr_exception_status: "pending".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("gl_ambiguity", &row), Some(0.40));
    }

    #[test]
    fn confidence_for_gl_returns_none_when_above_threshold() {
        // A clean invoice with cat_conf >= 0.80 has no GL ambiguity. Without
        // this predicate the sweep would pre-claim gl_ambiguity:<id> for every
        // well-categorized invoice at thresholds below 0.95.
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.95),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("gl_ambiguity", &row), None);
    }

    #[test]
    fn confidence_for_gl_falls_back_to_low_confidence_when_uncategorized_and_uncoded() {
        // Mirrors build_proposals' else-if branch: no categorization AND no
        // gl_code emits the 0.10 fallback proposal.
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: None,
            po_number: Some("PO-1".to_string()),
            gl_code: None,
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("gl_ambiguity", &row), Some(0.10));
    }

    #[test]
    fn confidence_for_gl_returns_none_when_uncategorized_but_already_coded() {
        // Mirrors build_proposals: cat_conf is None but gl_code is present,
        // so no proposal is emitted (the invoice already has a coding).
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: None,
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("gl_ambiguity", &row), None);
    }

    #[test]
    fn confidence_for_missing_po_returns_some_when_po_absent() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: None,
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("missing_po", &row), Some(0.30));
    }

    #[test]
    fn confidence_for_missing_po_returns_some_when_po_blank() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("   ".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("missing_po", &row), Some(0.30));
    }

    #[test]
    fn confidence_for_missing_po_returns_none_when_po_present() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("missing_po", &row), None);
    }

    #[test]
    fn confidence_for_vendor_mismatch_returns_some_when_vendor_unknown() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Unknown Vendor".to_string(),
        };
        assert_eq!(confidence_for("vendor_mismatch", &row), Some(0.20));
    }

    #[test]
    fn confidence_for_vendor_mismatch_returns_some_when_vendor_blank() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "".to_string(),
        };
        assert_eq!(confidence_for("vendor_mismatch", &row), Some(0.20));
    }

    #[test]
    fn confidence_for_vendor_mismatch_returns_some_when_vendor_processing() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Processing...".to_string(),
        };
        assert_eq!(confidence_for("vendor_mismatch", &row), Some(0.20));
    }

    #[test]
    fn confidence_for_vendor_mismatch_returns_none_when_vendor_resolved() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.95),
            categorization_confidence: Some(0.99),
            po_number: Some("PO-1".to_string()),
            gl_code: Some("5000".to_string()),
            ocr_exception_status: "approved".to_string(),
            vendor_name: "Acme Corp".to_string(),
        };
        assert_eq!(confidence_for("vendor_mismatch", &row), None);
    }

    #[test]
    fn confidence_for_duplicate_and_policy_violation_still_return_none() {
        // `duplicate` and `policy_violation` require per-invoice detector
        // state the sweep does not re-derive; without these arms the sweep
        // would write phantom auto_resolved rows for invoices that have no
        // exception of that type.
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.55),
            categorization_confidence: Some(0.40),
            po_number: None,
            gl_code: None,
            ocr_exception_status: "pending".to_string(),
            vendor_name: "".to_string(),
        };
        assert_eq!(confidence_for("duplicate", &row), None);
        assert_eq!(confidence_for("policy_violation", &row), None);
    }
}
