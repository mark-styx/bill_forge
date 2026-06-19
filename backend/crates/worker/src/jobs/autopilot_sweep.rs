//! Autopilot Sweep Job (refs #379)
//!
//! Background pass that auto-resolves queued exceptions when their proposed
//! resolution confidence is at or above the tenant's `autopilot_threshold` AND
//! the exception_type is in the tenant's `autopilot_enabled_types` list.
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
    let enabled: Vec<String> = settings_json
        .get("autopilot_enabled_types")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

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
        r#"SELECT id, ocr_confidence, categorization_confidence, po_number, gl_code
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
}

/// Map an exception_type to the detector-emitted confidence for this invoice.
/// Mirrors the logic in billforge_api::routes::autopilot::build_proposals.
fn confidence_for(exception_type: &str, row: &SignalRow) -> Option<f32> {
    match exception_type {
        "ocr_low_confidence" => row
            .ocr_confidence
            .filter(|c| *c < 0.90),
        "gl_ambiguity" => row.categorization_confidence,
        // These exception_types require per-invoice detector state that the
        // sweep does not re-derive; the human-in-the-loop cockpit remains the
        // resolution path for them.
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
        };
        assert_eq!(confidence_for("gl_ambiguity", &row), Some(0.40));
    }

    #[test]
    fn confidence_for_unsupported_types_returns_none() {
        let row = SignalRow {
            id: Uuid::new_v4(),
            ocr_confidence: Some(0.55),
            categorization_confidence: Some(0.40),
            po_number: None,
            gl_code: None,
        };
        assert_eq!(confidence_for("missing_po", &row), None);
        assert_eq!(confidence_for("vendor_mismatch", &row), None);
        assert_eq!(confidence_for("duplicate", &row), None);
        assert_eq!(confidence_for("policy_violation", &row), None);
    }
}
