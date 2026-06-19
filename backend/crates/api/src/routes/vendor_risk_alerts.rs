//! API routes for the per-tenant vendor-risk alert surface (refs #381).
//!
//! Endpoints:
//!   - GET  /api/v1/vendors/risk-alerts            : list open alerts, severity-sortable
//!   - POST /api/v1/vendors/risk-alerts/:id/acknowledge
//!
//! Also exposes two helpers consumed elsewhere in the API surface:
//!   - [`insert_banking_change_alert`] : called by the PUT /:id/banking hook
//!     so a real-time banking-change produces a `critical` alert.
//!   - [`vendor_has_open_critical_alert`] : payment-release guard that rejects
//!     while any open critical alert exists for the vendor.

use crate::error::{ApiError, ApiResult};
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    traits::AuditService,
    types::TenantId,
    Error, UserContext,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    // Mounted under /api/v1/vendors via routes/mod.rs. Paths must NOT collide
    // with vendors::routes() (which owns GET /) or Axum 0.7 panics at router
    // construction on startup. Frontend expects /risk-alerts and
    // /risk-alerts/:id/acknowledge (apps/web/src/lib/api.ts).
    Router::new()
        .route("/risk-alerts", get(list_alerts))
        .route("/risk-alerts/:id/acknowledge", post(acknowledge_alert))
}

// ---------------------------------------------------------------------------
// Shared alert writer (used by the banking-change hook + worker-shaped code)
// ---------------------------------------------------------------------------

/// Insert a `banking_change` critical alert for a vendor, idempotent on
/// `(vendor_id, alert_type, open + same payload hash)`. Sets `payment_hold`
/// when the alert is new and not already on hold.
///
/// `old_last_four` / `new_last_four` are masked into the payload (no raw PAN).
/// Called from `routes::vendors::update_banking` after the dual-approval row is
/// created.
pub async fn insert_banking_change_alert(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: Uuid,
    verification_id: Uuid,
    old_last_four: Option<&str>,
    new_last_four: &str,
) -> Result<(), Error> {
    let payload = serde_json::json!({
        "verification_id": verification_id.to_string(),
        "old_account_last_four": old_last_four,
        "new_account_last_four": new_last_four,
    });
    insert_alert(
        pool,
        tenant_id,
        vendor_id,
        "banking_change",
        "critical",
        payload,
        Some("Banking details changed - pending verification"),
    )
    .await
}

/// Generic idempotent insert for a vendor_risk_alert. Sets payment_hold=true
/// when severity == "critical" and a new row is created.
pub async fn insert_alert(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: Uuid,
    alert_type: &str,
    severity: &str,
    payload: serde_json::Value,
    hold_reason: Option<&str>,
) -> Result<(), Error> {
    let payload_hash = stable_payload_hash(&payload);

    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM vendor_risk_alerts
        WHERE vendor_id = $1
          AND tenant_id = $2
          AND alert_type = $3
          AND payload_hash = $4
          AND status = 'open'
        LIMIT 1
        "#,
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .bind(alert_type)
    .bind(&payload_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to look up open alert: {}", e)))?;

    if existing.is_some() {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO vendor_risk_alerts
            (tenant_id, vendor_id, alert_type, severity, payload, payload_hash)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .bind(alert_type)
    .bind(severity)
    .bind(&payload)
    .bind(&payload_hash)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to insert vendor_risk_alert: {}", e)))?;

    if severity == "critical" {
        let reason = hold_reason.unwrap_or("Vendor risk: open critical alert");
        sqlx::query(
            "UPDATE vendors SET payment_hold = true, payment_hold_reason = $3, updated_at = NOW() \
             WHERE id = $1 AND tenant_id = $2 AND payment_hold = false",
        )
        .bind(vendor_id)
        .bind(*tenant_id.as_uuid())
        .bind(reason)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to set payment_hold: {}", e)))?;
    }

    Ok(())
}

/// Payment-release guard: returns `true` when the vendor has any OPEN critical
/// vendor_risk_alert. Called from payment release / ERP sync paths alongside
/// the existing OFAC + pending-banking-verification checks.
pub async fn vendor_has_open_critical_alert(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: Uuid,
) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM vendor_risk_alerts \
         WHERE tenant_id = $1 AND vendor_id = $2 AND severity = 'critical' AND status = 'open'",
    )
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(c,)| c > 0).unwrap_or(false)
}

/// SHA-256 over a canonical (sorted-keys) JSON encoding so repeated producers
/// collapse to one open alert regardless of insertion order.
pub fn stable_payload_hash(payload: &serde_json::Value) -> String {
    let canonical = canonical_json(payload);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", k, canonical_json(&map[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        serde_json::Value::String(s) => format!("\"{}\"", s),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/vendors/risk-alerts
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListAlertsQuery {
    /// When provided, filter by severity (critical|high|medium|low).
    pub severity: Option<String>,
    /// When provided, filter by vendor id.
    pub vendor_id: Option<String>,
    /// When provided, filter by status (open|acknowledged). Defaults to open.
    pub status: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VendorRiskAlertItem {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub vendor_name: String,
    pub alert_type: String,
    pub severity: String,
    pub status: String,
    pub payload: serde_json::Value,
    pub acknowledged_by: Option<Uuid>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VendorRiskAlertList {
    pub items: Vec<VendorRiskAlertItem>,
}

#[derive(Debug, sqlx::FromRow)]
struct AlertRow {
    id: Uuid,
    vendor_id: Uuid,
    vendor_name: String,
    alert_type: String,
    severity: String,
    status: String,
    payload: serde_json::Value,
    acknowledged_by: Option<Uuid>,
    acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Severity sort key: critical > high > medium > low. Used in ORDER BY.
fn severity_rank(s: &str) -> i32 {
    match s {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

/// GET /api/v1/vendors/risk-alerts - list alerts for the tenant, severity-sorted.
#[utoipa::path(get, path = "/api/v1/vendors/risk-alerts", tag = "Vendors",
    params(
        ("severity" = Option<String>, Query, description = "Filter by severity: critical|high|medium|low"),
        ("vendor_id" = Option<String>, Query, description = "Filter by vendor id"),
        ("status" = Option<String>, Query, description = "Filter by status: open|acknowledged (default open)"),
    ),
    responses((status = 200, description = "Severity-sorted alerts", body = VendorRiskAlertList)))]
async fn list_alerts(
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    State(state): State<AppState>,
    Query(query): Query<ListAlertsQuery>,
) -> ApiResult<Json<VendorRiskAlertList>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Build the WHERE clause dynamically. RLS scopes by app.current_tenant_id,
    // and we narrow further with the optional filters the dashboard exposes.
    let mut where_clauses: Vec<String> = Vec::new();
    let mut bind_idx = 1u32;

    let mut severity_filter: Option<String> = None;
    let mut vendor_filter: Option<Uuid> = None;
    let mut status_filter = String::from("open");

    if let Some(s) = &query.severity {
        match s.as_str() {
            "critical" | "high" | "medium" | "low" => {
                where_clauses.push(format!("severity = ${}", bind_idx));
                severity_filter = Some(s.clone());
                bind_idx += 1;
            }
            _ => {}
        }
    }
    if let Some(vid) = &query.vendor_id {
        if let Ok(parsed) = vid.parse::<Uuid>() {
            where_clauses.push(format!("vendor_id = ${}", bind_idx));
            vendor_filter = Some(parsed);
            bind_idx += 1;
        }
    }
    if let Some(s) = &query.status {
        match s.as_str() {
            "open" | "acknowledged" => status_filter = s.clone(),
            _ => {}
        }
    }
    where_clauses.push(format!("status = ${}", bind_idx));

    let sql = format!(
        r#"
        SELECT a.id, a.vendor_id, v.name AS vendor_name, a.alert_type, a.severity,
               a.status, a.payload, a.acknowledged_by, a.acknowledged_at, a.created_at
        FROM vendor_risk_alerts a
        JOIN vendors v ON v.id = a.vendor_id
        WHERE {}
        ORDER BY
            CASE a.severity
                WHEN 'critical' THEN 0
                WHEN 'high'     THEN 1
                WHEN 'medium'   THEN 2
                WHEN 'low'      THEN 3
                ELSE 4
            END,
            a.created_at DESC
        LIMIT 200
        "#,
        where_clauses.join(" AND ")
    );

    let mut q = sqlx::query_as::<_, AlertRow>(&sql);
    if let Some(s) = severity_filter {
        q = q.bind(s);
    }
    if let Some(vid) = vendor_filter {
        q = q.bind(vid);
    }
    q = q.bind(status_filter);

    let rows = q
        .fetch_all(&*pool)
        .await
        .map_err(|e| ApiError(Error::Database(format!("Failed to list alerts: {}", e))))?;

    let items = rows
        .into_iter()
        .map(|r| VendorRiskAlertItem {
            id: r.id,
            vendor_id: r.vendor_id,
            vendor_name: r.vendor_name,
            alert_type: r.alert_type,
            severity: r.severity,
            status: r.status,
            payload: r.payload,
            acknowledged_by: r.acknowledged_by,
            acknowledged_at: r.acknowledged_at,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(VendorRiskAlertList { items }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/vendors/risk-alerts/:id/acknowledge
// ---------------------------------------------------------------------------

/// POST /api/v1/vendors/risk-alerts/{id}/acknowledge
///
/// Sets status='acknowledged', records the user, writes an audit entry, and
/// clears `payment_hold` when no remaining open critical alerts exist for the
/// vendor (preserving existing OFAC + pending-banking hold reasons by leaving
/// payment_hold alone when other holds remain).
#[utoipa::path(post, path = "/api/v1/vendors/risk-alerts/{id}/acknowledge", tag = "Vendors",
    params(("id" = Uuid, Path, description = "Alert id")),
    responses(
        (status = 200, description = "Alert acknowledged"),
        (status = 404, description = "Alert not found for this tenant"),
        (status = 409, description = "Alert already acknowledged"),
    ))]
async fn acknowledge_alert(
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Read the alert first so we can report correct status codes + reason about
    // post-acknowledge hold clearing. RLS scopes this query to the tenant.
    let row: Option<AlertRow> = sqlx::query_as::<_, AlertRow>(
        r#"
        SELECT a.id, a.vendor_id, v.name AS vendor_name, a.alert_type, a.severity,
               a.status, a.payload, a.acknowledged_by, a.acknowledged_at, a.created_at
        FROM vendor_risk_alerts a
        JOIN vendors v ON v.id = a.vendor_id
        WHERE a.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| ApiError(Error::Database(format!("Failed to read alert: {}", e))))?;

    let alert = row.ok_or_else(|| {
        ApiError(Error::NotFound {
            resource_type: "VendorRiskAlert".to_string(),
            id: id.to_string(),
        })
    })?;

    if alert.status == "acknowledged" {
        return Err(ApiError(Error::Conflict(
            "Alert already acknowledged".to_string(),
        )));
    }

    // Mark acknowledged.
    let result = sqlx::query(
        r#"
        UPDATE vendor_risk_alerts
        SET status = 'acknowledged',
            acknowledged_by = $2,
            acknowledged_at = NOW()
        WHERE id = $1 AND status = 'open'
        "#,
    )
    .bind(id)
    .bind(user.user_id.0)
    .execute(&*pool)
    .await
    .map_err(|e| {
        ApiError(Error::Database(format!(
            "Failed to acknowledge alert: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        // Lost a race with another acknowledger; treat as already done.
        return Err(ApiError(Error::Conflict(
            "Alert already acknowledged".to_string(),
        )));
    }

    // If this was a critical alert and no other open critical alert remains for
    // the vendor, clear payment_hold (but only if no banking verification is
    // still pending - that hold reason must survive on its own).
    let cleared_hold = if alert.severity == "critical" {
        let remaining =
            vendor_has_open_critical_alert(&pool, &tenant.tenant_id, alert.vendor_id).await;
        if !remaining {
            // Preserve existing pending-banking-verification hold (separate
            // surface); only clear the risk-alert-driven hold.
            let banking_repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
            let vid = billforge_core::domain::VendorId(alert.vendor_id);
            let banking_pending = banking_repo
                .has_pending_banking_verification(&tenant.tenant_id, &vid)
                .await
                .unwrap_or(false);
            if !banking_pending {
                sqlx::query(
                    "UPDATE vendors SET payment_hold = false, payment_hold_reason = NULL, updated_at = NOW() \
                     WHERE id = $1 AND tenant_id = $2",
                )
                .bind(alert.vendor_id)
                .bind(*tenant.tenant_id.as_uuid())
                .execute(&*pool)
                .await
                .map_err(|e| {
                    ApiError(Error::Database(format!(
                        "Failed to clear payment_hold: {}",
                        e
                    )))
                })?;
                true
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    // Audit entry.
    write_acknowledge_audit(&pool, &tenant.tenant_id, &user, &alert).await;

    Ok(Json(serde_json::json!({
        "status": "acknowledged",
        "alert_id": id,
        "payment_hold_cleared": cleared_hold,
    })))
}

async fn write_acknowledge_audit(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    user: &UserContext,
    alert: &AlertRow,
) {
    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Vendor,
        alert.vendor_id.to_string(),
        format!(
            "Acknowledged {} vendor-risk alert ({}) for vendor {}",
            alert.severity, alert.alert_type, alert.vendor_name
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "alert_id": alert.id.to_string(),
        "alert_type": alert.alert_type,
        "severity": alert.severity,
        "vendor_id": alert.vendor_id.to_string(),
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(Arc::new(pool.clone()));
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log vendor-risk acknowledge audit entry");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_rank_orders_critical_first() {
        assert!(severity_rank("critical") < severity_rank("high"));
        assert!(severity_rank("high") < severity_rank("medium"));
        assert!(severity_rank("medium") < severity_rank("low"));
    }

    #[test]
    fn stable_payload_hash_is_deterministic_across_key_order() {
        let a = serde_json::json!({"verification_id": "v1", "new_account_last_four": "1234"});
        let b = serde_json::json!({"new_account_last_four": "1234", "verification_id": "v1"});
        assert_eq!(stable_payload_hash(&a), stable_payload_hash(&b));
    }

    #[test]
    fn stable_payload_hash_distinguishes_payloads() {
        let a = serde_json::json!({"verification_id": "v1"});
        let b = serde_json::json!({"verification_id": "v2"});
        assert_ne!(stable_payload_hash(&a), stable_payload_hash(&b));
    }
}
