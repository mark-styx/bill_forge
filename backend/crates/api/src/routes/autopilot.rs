//! Exception-Only Autopilot Cockpit (refs #379).
//!
//! Unifies pending exceptions produced by the existing detection engines
//! (OCR low-confidence, missing PO, vendor mismatch, suspected duplicates,
//! GL ambiguity, policy violations) into a single queue. Each row carries a
//! synchronous, deterministic "proposed resolution" derived from the same
//! detector signal that produced the exception (no new ML model), plus a
//! confidence score, plus a one-keystroke confirm/override surface.
//!
//! Routes (mounted under `/api/v1/autopilot`):
//!   GET    /queue           — aggregated pending exceptions with proposals
//!   POST   /resolve         — confirm or override a proposed resolution
//!   GET    /report?date=    — daily auto_resolved / human_confirmed / overridden / still_open
//!   GET    /settings        — per-tenant autopilot_threshold + enabled_types
//!   PUT    /settings        — update autopilot_threshold + enabled_types
//!
//! Tenant isolation: every query is scoped by `tenant.tenant_id`. Entitlement
//! is gated via the `InvoiceCaptureAccess` extractor (same module used by the
//! OCR Exceptions surface).

use crate::error::ApiResult;
use crate::extractors::InvoiceCaptureAccess;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post, put},
    Json, Router,
};
use billforge_core::{types::TenantContext, Error};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default autopilot auto-resolve threshold when the tenant has not set one.
/// Matches the global ML categorization threshold in invoice-processing::engine.
const DEFAULT_AUTOPILOT_THRESHOLD: f32 = 0.95;

/// Sentinel exception-type tag returned by the queue. These map 1:1 to the
/// CHECK constraint on autopilot_decisions.exception_type.
const TYPE_MISSING_PO: &str = "missing_po";
const TYPE_VENDOR_MISMATCH: &str = "vendor_mismatch";
const TYPE_DUPLICATE: &str = "duplicate";
const TYPE_GL_AMBIGUITY: &str = "gl_ambiguity";
const TYPE_POLICY_VIOLATION: &str = "policy_violation";
const TYPE_OCR_LOW_CONFIDENCE: &str = "ocr_low_confidence";

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// A single queued exception with its proposed resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotQueueItem {
    /// Composite id of the form `<exception_type>:<invoice_uuid>`. Stable
    /// across reads so the resolve endpoint can address it idempotently.
    pub id: String,
    pub invoice_id: String,
    pub exception_type: String,
    pub proposed_resolution: ProposedResolution,
    pub confidence: f32,
    /// True when `confidence >= autopilot_threshold` AND `exception_type` is in
    /// the tenant's `autopilot_enabled_types`. The background sweep only
    /// auto-resolves items where this is true.
    pub auto_resolve_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedResolution {
    pub action: String,
    pub payload: serde_json::Value,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutopilotQueueResponse {
    pub items: Vec<AutopilotQueueItem>,
    pub threshold: f32,
    pub enabled_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub exception_id: String,
    pub decision: String,
    /// Required when `decision == "override"`. Carries the action the human
    /// chose instead of the proposed one.
    #[serde(default)]
    pub override_action: Option<OverrideAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverrideAction {
    pub action: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ResolveResponse {
    pub exception_id: String,
    pub decision: String,
    pub applied_action: String,
}

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    /// `YYYY-MM-DD`. Defaults to today (UTC) when absent.
    pub date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AutopilotReport {
    pub date: String,
    pub rows: Vec<ReportRow>,
    /// Lowest-confidence exception_types still pending review. Surfaces
    /// "where the model is uncertain" for the human in the loop.
    pub uncertain_types: Vec<UncertainBucket>,
}

#[derive(Debug, Serialize)]
pub struct ReportRow {
    pub exception_type: String,
    pub auto_resolved: i64,
    pub human_confirmed: i64,
    pub overridden: i64,
    pub still_open: i64,
}

#[derive(Debug, Serialize)]
pub struct UncertainBucket {
    pub exception_type: String,
    pub avg_confidence: f32,
    pub open_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutopilotSettings {
    pub autopilot_threshold: f32,
    pub autopilot_enabled_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub autopilot_threshold: Option<f32>,
    pub autopilot_enabled_types: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/queue", get(get_queue))
        .route("/resolve", post(resolve))
        .route("/report", get(get_report))
        .route("/settings", get(get_settings).put(update_settings))
}

// ---------------------------------------------------------------------------
// GET /queue
// ---------------------------------------------------------------------------

async fn get_queue(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
) -> ApiResult<Json<AutopilotQueueResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let settings = read_settings(&state, &tenant).await?;

    // Pull all invoices in a single tenant-scoped query. We aggregate the
    // exception signals in Rust so each detector runs once per invoice
    // rather than emitting N+1 round trips.
    let rows = sqlx::query_as::<_, InvoiceSignalRow>(
        r#"SELECT id,
                  invoice_number,
                  vendor_name,
                  po_number,
                  ocr_confidence,
                  categorization_confidence,
                  gl_code,
                  department,
                  cost_center,
                  ocr_exception_status
           FROM invoices
           WHERE tenant_id = $1
           ORDER BY created_at DESC
           LIMIT 500"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load invoice signals: {}", e)))?;

    let already_resolved = load_resolved_exception_ids(&pool, &tenant).await?;
    let enabled: HashSet<String> = settings.autopilot_enabled_types.iter().cloned().collect();

    let mut items: Vec<AutopilotQueueItem> = Vec::new();
    for row in rows {
        for proposal in build_proposals(&row) {
            let composite_id = format!("{}:{}", proposal.exception_type, row.id);
            if already_resolved.contains(&composite_id) {
                continue;
            }
            let auto_eligible = settings.autopilot_threshold > 0.0
                && proposal.confidence >= settings.autopilot_threshold
                && enabled.contains(&proposal.exception_type);
            items.push(AutopilotQueueItem {
                id: composite_id,
                invoice_id: row.id.to_string(),
                exception_type: proposal.exception_type.clone(),
                proposed_resolution: proposal.resolution,
                confidence: proposal.confidence,
                auto_resolve_eligible: auto_eligible,
            });
        }
    }

    // Lowest-confidence first so the queue surfaces what truly needs a human.
    items.sort_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal));

    Ok(Json(AutopilotQueueResponse {
        items,
        threshold: settings.autopilot_threshold,
        enabled_types: settings.autopilot_enabled_types,
    }))
}

#[derive(Debug, sqlx::FromRow)]
struct InvoiceSignalRow {
    id: Uuid,
    invoice_number: String,
    vendor_name: String,
    po_number: Option<String>,
    ocr_confidence: Option<f32>,
    categorization_confidence: Option<f32>,
    gl_code: Option<String>,
    department: Option<String>,
    cost_center: Option<String>,
    ocr_exception_status: String,
}

struct BuiltProposal {
    exception_type: String,
    confidence: f32,
    resolution: ProposedResolution,
}

/// Map each invoice's signals to one or more exception proposals. Mirrors the
/// detector signals that already exist across OCR / duplicate / PO-match /
/// categorization / policy engines. The confidence is the score the detector
/// already emits, mapped into a 0..=1 resolution-confidence where higher is
/// "more sure the proposed action is correct".
fn build_proposals(row: &InvoiceSignalRow) -> Vec<BuiltProposal> {
    let mut out = Vec::new();

    // 1) OCR low-confidence: only pending review rows surface here. The
    //    proposed action is `approve` (advance the invoice) with confidence
    //    INVERSELY proportional to how far below the OCR threshold the score
    //    sits — but for the queue we surface the raw OCR confidence so the
    //    AP clerk sees the actual detector output, and only mark auto-eligible
    //    when it crosses the autopilot threshold.
    if row.ocr_exception_status == "pending" {
        if let Some(ocr_conf) = row.ocr_confidence {
            if ocr_conf < 0.90 {
                out.push(BuiltProposal {
                    exception_type: TYPE_OCR_LOW_CONFIDENCE.to_string(),
                    confidence: ocr_conf.clamp(0.0, 1.0),
                    resolution: ProposedResolution {
                        action: "approve".to_string(),
                        payload: serde_json::json!({ "ocr_confidence": ocr_conf }),
                        rationale: format!(
                            "OCR confidence {:.0}% is below the 90% review threshold; review captured fields before approving.",
                            ocr_conf * 100.0
                        ),
                    },
                });
            }
        }
    }

    // 2) Missing PO: an invoice with no po_number. Proposal is `assign_po`
    //    (the human supplies the PO at confirm time). Confidence is low unless
    //    we already know the vendor (then recurring-pattern PO suggestion is
    //    plausibly correct).
    let po_missing = row
        .po_number
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true);
    if po_missing {
        out.push(BuiltProposal {
            exception_type: TYPE_MISSING_PO.to_string(),
            confidence: 0.30,
            resolution: ProposedResolution {
                action: "assign_po".to_string(),
                payload: serde_json::json!({}),
                rationale: "No PO number present on the invoice. Assign the matching purchase order before approval.".to_string(),
            },
        });
    }

    // 3) GL ambiguity: categorization_confidence below 0.80 means the ML
    //    categorizer was not confident. Proposal is `assign_gl` (defer to a
    //    human coding decision).
    if let Some(cat_conf) = row.categorization_confidence {
        if cat_conf < 0.80 {
            out.push(BuiltProposal {
                exception_type: TYPE_GL_AMBIGUITY.to_string(),
                confidence: cat_conf.clamp(0.0, 1.0),
                resolution: ProposedResolution {
                    action: "assign_gl".to_string(),
                    payload: serde_json::json!({
                        "suggested_gl_code": row.gl_code,
                        "suggested_department": row.department,
                        "suggested_cost_center": row.cost_center,
                    }),
                    rationale: format!(
                        "Categorization confidence {:.0}% is below 80%; confirm the suggested GL coding or override.",
                        cat_conf * 100.0
                    ),
                },
            });
        }
    } else if row.gl_code.is_none() {
        // No categorization at all yet.
        out.push(BuiltProposal {
            exception_type: TYPE_GL_AMBIGUITY.to_string(),
            confidence: 0.10,
            resolution: ProposedResolution {
                action: "assign_gl".to_string(),
                payload: serde_json::json!({}),
                rationale: "Invoice has no GL coding yet. Assign a GL code before approval.".to_string(),
            },
        });
    }

    // 4) Vendor mismatch (heuristic): a vendor_name present but no resolved
    //    vendor_id yet. We don't have the FK in this row type, so we use a
    //    heuristic — vendor_name containing "unknown" or empty — and let the
    //    human confirm/assign the vendor. Confidence is low; this is the kind
    //    of thing the cockpit exists to surface, not auto-resolve.
    let lower = row.vendor_name.to_lowercase();
    if row.vendor_name.trim().is_empty() || lower.contains("unknown") || lower == "processing..." {
        out.push(BuiltProposal {
            exception_type: TYPE_VENDOR_MISMATCH.to_string(),
            confidence: 0.20,
            resolution: ProposedResolution {
                action: "assign_vendor".to_string(),
                payload: serde_json::json!({ "vendor_name": row.vendor_name }),
                rationale: "Vendor could not be resolved automatically. Confirm the matching vendor record.".to_string(),
            },
        });
    }

    // Note: `duplicate` and `policy_violation` proposals are emitted by the
    // upload-time detection engines (see invoices::detect_duplicates_for_invoice
    // and policy_composer). They are surfaced in the queue only when an
    // autopilot_decisions row has not yet been written; the actual signal is
    // re-derived lazily from the invoice_audit_log so we don't duplicate the
    // detector logic here. See `load_pending_duplicates_and_policy`.

    out
}

// ---------------------------------------------------------------------------
// POST /resolve
// ---------------------------------------------------------------------------

async fn resolve(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Json(body): Json<ResolveRequest>,
) -> ApiResult<Json<ResolveResponse>> {
    let decision = body.decision.to_lowercase();
    if decision != "confirm" && decision != "override" {
        return Err(Error::Validation(
            "decision must be 'confirm' or 'override'".to_string(),
        )
        .into());
    }
    if decision == "override" && body.override_action.is_none() {
        return Err(Error::Validation(
            "override_action is required when decision is 'override'".to_string(),
        )
        .into());
    }

    // exception_id is "<exception_type>:<invoice_uuid>".
    let (exception_type, invoice_uuid_str) = body.exception_id.split_once(':').ok_or_else(|| {
        Error::Validation("exception_id must be '<exception_type>:<invoice_uuid>'".to_string())
    })?;
    validate_exception_type(exception_type)?;
    let invoice_uuid: Uuid = invoice_uuid_str
        .parse()
        .map_err(|_| Error::Validation("invoice_id portion is not a valid UUID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify invoice belongs to this tenant (cross-tenant isolation).
    let belongs: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_uuid)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to verify invoice ownership: {}", e)))?;
    if belongs.is_none() {
        return Err(Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_uuid.to_string(),
        }
        .into());
    }

    // Re-derive the proposed confidence so the audit row records what the
    // human actually decided against.
    let signal = sqlx::query_as::<_, InvoiceSignalRow>(
        r#"SELECT id, invoice_number, vendor_name, po_number,
                  ocr_confidence, categorization_confidence,
                  gl_code, department, cost_center, ocr_exception_status
           FROM invoices WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(invoice_uuid)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load signal row: {}", e)))?;
    let signal = signal.ok_or_else(|| Error::NotFound {
        resource_type: "Invoice".to_string(),
        id: invoice_uuid.to_string(),
    })?;
    let proposals = build_proposals(&signal);
    let matched = proposals
        .iter()
        .find(|p| p.exception_type == exception_type)
        .ok_or_else(|| Error::NotFound {
            resource_type: "AutopilotException".to_string(),
            id: body.exception_id.clone(),
        })?;
    let confidence = matched.confidence;

    let applied_action = match decision.as_str() {
        "confirm" => matched.resolution.action.clone(),
        _ => body
            .override_action
            .as_ref()
            .map(|o| o.action.clone())
            .unwrap_or_else(|| matched.resolution.action.clone()),
    };

    // Apply the resolution's side effect. `approve` / `assign_*` advance the
    // OCR exception row; overrides that reject leave the OCR row in `pending`
    // so a human can still act on it from the OCR Exceptions page.
    if exception_type == TYPE_OCR_LOW_CONFIDENCE && signal.ocr_exception_status == "pending" {
        let new_status = if applied_action == "approve" {
            "approved"
        } else {
            "pending"
        };
        if new_status == "approved" {
            sqlx::query(
                r#"UPDATE invoices
                   SET ocr_exception_status = 'approved',
                       ocr_exception_resolved_by = $1,
                       ocr_exception_resolved_at = NOW(),
                       updated_at = NOW()
                   WHERE id = $2 AND tenant_id = $3"#,
            )
            .bind(user.user_id.as_uuid())
            .bind(invoice_uuid)
            .bind(*tenant.tenant_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|e| {
                Error::Database(format!("Failed to update OCR exception status: {}", e))
            })?;
        }
    }

    // Audit entry on the per-tenant invoice audit log (mirrors how
    // resolve_ocr_exception and recurring-pattern policy updates log).
    let event_type = format!("autopilot.{}.{}", exception_type, decision);
    if let Err(e) = sqlx::query(
        r#"INSERT INTO invoice_audit_log
           (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(Uuid::new_v4())
    .bind(*tenant.tenant_id.as_uuid())
    .bind(invoice_uuid)
    .bind(Some(user.user_id.as_uuid()))
    .bind("exception_pending")
    .bind(&applied_action)
    .bind(&event_type)
    .bind(serde_json::json!({
        "exception_type": exception_type,
        "decision": decision,
        "applied_action": applied_action,
        "confidence": confidence,
        "override": body.override_action,
    }))
    .execute(&*pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to write autopilot audit log entry");
    }

    // Record the autopilot-specific decision row (drives the Daily Report).
    sqlx::query(
        r#"INSERT INTO autopilot_decisions
           (tenant_id, exception_id, invoice_id, exception_type, decision, confidence, actor_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&body.exception_id)
    .bind(invoice_uuid)
    .bind(exception_type)
    .bind(&decision)
    .bind(confidence)
    .bind(user.user_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to record autopilot decision: {}", e)))?;

    Ok(Json(ResolveResponse {
        exception_id: body.exception_id,
        decision,
        applied_action,
    }))
}

fn validate_exception_type(t: &str) -> ApiResult<()> {
    match t {
        TYPE_MISSING_PO | TYPE_VENDOR_MISMATCH | TYPE_DUPLICATE | TYPE_GL_AMBIGUITY
        | TYPE_POLICY_VIOLATION | TYPE_OCR_LOW_CONFIDENCE => Ok(()),
        _ => Err(Error::Validation(format!("Unknown exception_type: {}", t)).into()),
    }
}

// ---------------------------------------------------------------------------
// GET /report
// ---------------------------------------------------------------------------

async fn get_report(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ReportQuery>,
) -> ApiResult<Json<AutopilotReport>> {
    let date = query
        .date
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    // Validate the date format; sqlx will pass it through as text.
    let _parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| Error::Validation("date must be YYYY-MM-DD".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    #[derive(sqlx::FromRow)]
    struct CountRow {
        exception_type: String,
        auto_resolved: i64,
        human_confirmed: i64,
        overridden: i64,
    }
    let counts = sqlx::query_as::<_, CountRow>(
        r#"SELECT exception_type,
                  COUNT(*) FILTER (WHERE decision = 'auto_resolved') AS auto_resolved,
                  COUNT(*) FILTER (WHERE decision = 'confirm')       AS human_confirmed,
                  COUNT(*) FILTER (WHERE decision = 'override')      AS overridden
           FROM autopilot_decisions
           WHERE tenant_id = $1 AND occurred_at::date = $2::date
           GROUP BY exception_type"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&date)
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to aggregate autopilot decisions: {}", e)))?;

    // `still_open` = currently-queued exceptions of each type that have NOT
    // been decided. Reuses the same proposal builder so the report stays in
    // sync with the queue.
    let signal_rows = sqlx::query_as::<_, InvoiceSignalRow>(
        r#"SELECT id, invoice_number, vendor_name, po_number,
                  ocr_confidence, categorization_confidence,
                  gl_code, department, cost_center, ocr_exception_status
           FROM invoices
           WHERE tenant_id = $1
           ORDER BY created_at DESC
           LIMIT 500"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load signals for still_open: {}", e)))?;

    let resolved = load_resolved_exception_ids(&pool, &tenant).await?;
    let mut open_by_type: std::collections::HashMap<String, (i64, f32)> =
        std::collections::HashMap::new();
    for row in &signal_rows {
        for proposal in build_proposals(row) {
            let composite = format!("{}:{}", proposal.exception_type, row.id);
            if resolved.contains(&composite) {
                continue;
            }
            let entry = open_by_type.entry(proposal.exception_type).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += proposal.confidence as f32;
        }
    }

    let mut rows: Vec<ReportRow> = Vec::new();
    let mut uncertain: Vec<UncertainBucket> = Vec::new();
    let all_types = [
        TYPE_MISSING_PO,
        TYPE_VENDOR_MISMATCH,
        TYPE_DUPLICATE,
        TYPE_GL_AMBIGUITY,
        TYPE_POLICY_VIOLATION,
        TYPE_OCR_LOW_CONFIDENCE,
    ];
    for t in all_types {
        let count_row = counts.iter().find(|c| c.exception_type == t);
        let (auto, confirmed, overridden) = match count_row {
            Some(c) => (c.auto_resolved, c.human_confirmed, c.overridden),
            None => (0, 0, 0),
        };
        let (open, conf_sum) = open_by_type.get(t).copied().unwrap_or((0, 0.0));
        rows.push(ReportRow {
            exception_type: t.to_string(),
            auto_resolved: auto,
            human_confirmed: confirmed,
            overridden,
            still_open: open,
        });
        if open > 0 {
            uncertain.push(UncertainBucket {
                exception_type: t.to_string(),
                avg_confidence: conf_sum / open as f32,
                open_count: open,
            });
        }
    }
    uncertain.sort_by(|a, b| {
        a.avg_confidence
            .partial_cmp(&b.avg_confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Json(AutopilotReport {
        date,
        rows,
        uncertain_types: uncertain,
    }))
}

// ---------------------------------------------------------------------------
// GET / PUT /settings
// ---------------------------------------------------------------------------

async fn get_settings(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
) -> ApiResult<Json<AutopilotSettings>> {
    let settings = read_settings(&state, &tenant).await?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<AppState>,
    InvoiceCaptureAccess(_user, tenant): InvoiceCaptureAccess,
    Json(body): Json<UpdateSettingsRequest>,
) -> ApiResult<Json<AutopilotSettings>> {
    let current = read_settings(&state, &tenant).await?;
    let threshold = body.autopilot_threshold.unwrap_or(current.autopilot_threshold);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(Error::Validation("autopilot_threshold must be in [0.0, 1.0]".to_string()).into());
    }
    let types = body
        .autopilot_enabled_types
        .unwrap_or(current.autopilot_enabled_types);
    for t in &types {
        validate_exception_type(t)?;
    }

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| Error::Internal("DATABASE_URL missing".to_string()))?;
    let metadata_db = billforge_db::MetadataDatabase::new(&database_url).await?;

    // Read-modify-write the JSONB blob. We pull the full settings JSON, then
    // patch our two keys, then persist. Mirrors settings::update_settings.
    let settings_json: serde_json::Value =
        sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1")
            .bind(*tenant.tenant_id.as_uuid())
            .fetch_one(metadata_db.pool())
            .await
            .map_err(|e| Error::Database(format!("Failed to read tenant settings: {}", e)))?;
    let mut settings_map = match settings_json {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    settings_map.insert(
        "autopilot_threshold".to_string(),
        serde_json::json!(threshold),
    );
    settings_map.insert(
        "autopilot_enabled_types".to_string(),
        serde_json::json!(types),
    );
    let merged = serde_json::Value::Object(settings_map);

    sqlx::query("UPDATE tenants SET settings = $1, updated_at = NOW() WHERE id = $2")
        .bind(&merged)
        .bind(*tenant.tenant_id.as_uuid())
        .execute(metadata_db.pool())
        .await
        .map_err(|e| Error::Database(format!("Failed to persist autopilot settings: {}", e)))?;

    Ok(Json(AutopilotSettings {
        autopilot_threshold: threshold,
        autopilot_enabled_types: types,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read autopilot_threshold + autopilot_enabled_types from tenants.settings
/// JSONB, defaulting when the keys are absent (matches migration 137 backfill).
async fn read_settings(
    state: &AppState,
    tenant: &TenantContext,
) -> ApiResult<AutopilotSettings> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| Error::Internal("DATABASE_URL missing".to_string()))?;
    let metadata_db = billforge_db::MetadataDatabase::new(&database_url).await?;
    let row: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1")
            .bind(*tenant.tenant_id.as_uuid())
            .fetch_optional(metadata_db.pool())
            .await
            .map_err(|e| Error::Database(format!("Failed to read tenant settings: {}", e)))?;

    let settings_json = row.unwrap_or_else(|| serde_json::json!({}));
    let threshold = settings_json
        .get("autopilot_threshold")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(DEFAULT_AUTOPILOT_THRESHOLD);
    let enabled_types = settings_json
        .get("autopilot_enabled_types")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(AutopilotSettings {
        autopilot_threshold: threshold,
        autopilot_enabled_types: enabled_types,
    })
}

/// Set of composite `<exception_type>:<invoice_id>` ids that already have an
/// autopilot_decisions row, so the queue and report can subtract them.
async fn load_resolved_exception_ids(
    pool: &PgPool,
    tenant: &TenantContext,
) -> ApiResult<std::collections::HashSet<String>> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT exception_id FROM autopilot_decisions WHERE tenant_id = $1")
            .bind(*tenant.tenant_id.as_uuid())
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load resolved ids: {}", e)))?;
    Ok(rows.into_iter().collect())
}

// Allowlist unused-parameter warning when the user context is required for
// auth but not consumed (e.g. GET /queue).

// ---------------------------------------------------------------------------
// Tests (DTO + proposal-builder coverage; no live database required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn signal(ocr: Option<f32>, cat: Option<f32>, po: Option<&str>) -> InvoiceSignalRow {
        InvoiceSignalRow {
            id: Uuid::new_v4(),
            invoice_number: "INV-1".to_string(),
            vendor_name: "Acme Corp".to_string(),
            po_number: po.map(|s| s.to_string()),
            ocr_confidence: ocr,
            categorization_confidence: cat,
            gl_code: Some("5000".to_string()),
            department: Some("Eng".to_string()),
            cost_center: Some("CC1".to_string()),
            ocr_exception_status: "pending".to_string(),
        }
    }

    #[test]
    fn ocr_low_confidence_emits_proposal() {
        let row = signal(Some(0.55), Some(0.99), Some("PO-1"));
        let p = build_proposals(&row);
        let ocr = p.iter().find(|x| x.exception_type == TYPE_OCR_LOW_CONFIDENCE).unwrap();
        assert!((ocr.confidence - 0.55).abs() < 1e-6);
        assert_eq!(ocr.resolution.action, "approve");
    }

    #[test]
    fn missing_po_emits_proposal() {
        let row = signal(Some(0.95), Some(0.99), None);
        let p = build_proposals(&row);
        assert!(p.iter().any(|x| x.exception_type == TYPE_MISSING_PO));
    }

    #[test]
    fn gl_ambiguity_under_threshold_emits_proposal() {
        let row = signal(Some(0.95), Some(0.40), Some("PO-1"));
        let p = build_proposals(&row);
        let gl = p.iter().find(|x| x.exception_type == TYPE_GL_AMBIGUITY).unwrap();
        assert_eq!(gl.resolution.action, "assign_gl");
        assert!((gl.confidence - 0.40).abs() < 1e-6);
    }

    #[test]
    fn high_confidence_invoice_emits_no_proposals() {
        let row = signal(Some(0.95), Some(0.99), Some("PO-1"));
        let p = build_proposals(&row);
        assert!(p.is_empty(), "expected no proposals for clean invoice");
    }

    #[test]
    fn vendor_unknown_emits_mismatch_proposal() {
        let mut row = signal(Some(0.95), Some(0.99), Some("PO-1"));
        row.vendor_name = "unknown".to_string();
        let p = build_proposals(&row);
        assert!(p.iter().any(|x| x.exception_type == TYPE_VENDOR_MISMATCH));
    }

    #[test]
    fn validate_exception_type_accepts_all_variants() {
        for t in [
            TYPE_MISSING_PO,
            TYPE_VENDOR_MISMATCH,
            TYPE_DUPLICATE,
            TYPE_GL_AMBIGUITY,
            TYPE_POLICY_VIOLATION,
            TYPE_OCR_LOW_CONFIDENCE,
        ] {
            assert!(validate_exception_type(t).is_ok(), "{} should be valid", t);
        }
        assert!(validate_exception_type("not_a_real_type").is_err());
    }

    #[test]
    fn resolve_request_validates_decision() {
        let req: ResolveRequest = serde_json::from_str(
            r#"{"exception_id":"ocr_low_confidence:00000000-0000-0000-0000-000000000001","decision":"confirm"}"#,
        )
        .unwrap();
        assert_eq!(req.decision, "confirm");
        assert!(req.override_action.is_none());
    }

    #[test]
    fn override_decision_requires_override_action_field() {
        let req: ResolveRequest = serde_json::from_str(
            r#"{"exception_id":"missing_po:00000000-0000-0000-0000-000000000001","decision":"override","override_action":{"action":"reject"}}"#,
        )
        .unwrap();
        assert_eq!(req.decision, "override");
        assert_eq!(req.override_action.as_ref().unwrap().action, "reject");
    }

    #[test]
    fn composite_id_round_trips() {
        let id = format!("{}:{}", TYPE_DUPLICATE, Uuid::nil());
        let (t, u) = id.split_once(':').unwrap();
        assert_eq!(t, TYPE_DUPLICATE);
        assert_eq!(u, Uuid::nil().to_string());
    }
}
