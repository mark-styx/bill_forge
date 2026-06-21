//! Invoice status state machine with tenant-scoped audit logging.
//!
//! Defines the allowed status transitions, validates them atomically inside a
//! database transaction, and persists every transition to `invoice_audit_log`
//! for SOX-compliant audit trails.

use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Status enum
// ---------------------------------------------------------------------------

/// Canonical invoice statuses.  Stored as TEXT in Postgres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Received,
    InReview,
    PendingApproval,
    Approved,
    Rejected,
    Paid,
    Void,
}

impl InvoiceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::InReview => "in_review",
            Self::PendingApproval => "pending_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Paid => "paid",
            Self::Void => "void",
        }
    }

    pub fn from_str_lossy(s: &str) -> Option<Self> {
        match s {
            "received" => Some(Self::Received),
            "in_review" => Some(Self::InReview),
            "pending_approval" => Some(Self::PendingApproval),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "paid" => Some(Self::Paid),
            "void" => Some(Self::Void),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Transition table
// ---------------------------------------------------------------------------

/// A single allowed transition.
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    pub from: InvoiceStatus,
    pub to: InvoiceStatus,
    pub event_type: &'static str,
}

/// The complete set of allowed transitions.
/// Any transition not in this table is invalid and will be rejected.
const ALLOWED_TRANSITIONS: &[Transition] = &[
    // Normal forward flow
    Transition {
        from: InvoiceStatus::Received,
        to: InvoiceStatus::InReview,
        event_type: "start_review",
    },
    Transition {
        from: InvoiceStatus::InReview,
        to: InvoiceStatus::PendingApproval,
        event_type: "submit_for_approval",
    },
    Transition {
        from: InvoiceStatus::PendingApproval,
        to: InvoiceStatus::Approved,
        event_type: "approve",
    },
    // Touchless auto-approval lanes (recurring-pattern match, ML-confidence).
    // Lets the auto-approval paths in WorkflowEngine flow through the same
    // state-machine writer used by interactive approvals, keeping
    // invoices.status and invoices.processing_status in sync and producing a
    // single canonical audit-log row per approval.
    Transition {
        from: InvoiceStatus::Received,
        to: InvoiceStatus::Approved,
        event_type: "auto_approve",
    },
    Transition {
        from: InvoiceStatus::Approved,
        to: InvoiceStatus::Paid,
        event_type: "mark_paid",
    },
    // Rejection / void (from several states)
    Transition {
        from: InvoiceStatus::Received,
        to: InvoiceStatus::Void,
        event_type: "void",
    },
    Transition {
        from: InvoiceStatus::InReview,
        to: InvoiceStatus::Rejected,
        event_type: "reject",
    },
    Transition {
        from: InvoiceStatus::InReview,
        to: InvoiceStatus::Void,
        event_type: "void",
    },
    Transition {
        from: InvoiceStatus::PendingApproval,
        to: InvoiceStatus::Rejected,
        event_type: "reject",
    },
    Transition {
        from: InvoiceStatus::PendingApproval,
        to: InvoiceStatus::Void,
        event_type: "void",
    },
    Transition {
        from: InvoiceStatus::Approved,
        to: InvoiceStatus::Void,
        event_type: "void",
    },
    // Rework: send back to review
    Transition {
        from: InvoiceStatus::PendingApproval,
        to: InvoiceStatus::InReview,
        event_type: "send_back",
    },
    Transition {
        from: InvoiceStatus::InReview,
        to: InvoiceStatus::Received,
        event_type: "send_back",
    },
    // Reopen rejected
    Transition {
        from: InvoiceStatus::Rejected,
        to: InvoiceStatus::InReview,
        event_type: "reopen",
    },
];

/// Check whether a transition from `from` to `to` is allowed.
/// Returns the matching `event_type` if found.
pub fn find_transition(from: InvoiceStatus, to: InvoiceStatus) -> Option<&'static str> {
    ALLOWED_TRANSITIONS
        .iter()
        .find(|t| t.from == from && t.to == to)
        .map(|t| t.event_type)
}

// ---------------------------------------------------------------------------
// Core transition function
// ---------------------------------------------------------------------------

/// Execute a state machine transition atomically.
///
/// 1. Loads current status with `SELECT ... FOR UPDATE`
/// 2. Validates the transition against the allowed table
/// 3. Updates `invoices.status`
/// 4. Inserts an `invoice_audit_log` row
/// 5. Commits
///
/// Returns `Err(InvalidTransition)` if the transition is not in the allowlist.
pub async fn transition(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: &Uuid,
    actor_id: &Uuid,
    target_status: InvoiceStatus,
    event_type: &str,
    metadata: serde_json::Value,
) -> Result<(), billforge_core::Error> {
    let mut tx = pool.begin().await.map_err(|e| {
        billforge_core::Error::Database(format!("Failed to begin transaction: {}", e))
    })?;

    // 1. Load current status with row lock
    let current: Option<(Option<String>,)> =
        sqlx::query_as("SELECT status FROM invoices WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(invoice_id)
            .bind(*tenant_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to query invoice: {}", e))
            })?;

    let current_status_str = current
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: invoice_id.to_string(),
        })?
        .0
        .unwrap_or_else(|| "received".to_string());

    let current_status = InvoiceStatus::from_str_lossy(&current_status_str).ok_or_else(|| {
        billforge_core::Error::Database(format!(
            "Unknown current status '{}' on invoice {}",
            current_status_str, invoice_id
        ))
    })?;

    // 2. Validate transition
    if find_transition(current_status, target_status).is_none() {
        // Roll back by dropping the transaction
        tx.rollback()
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to rollback: {}", e)))?;
        return Err(billforge_core::Error::Validation(format!(
            "Invalid transition from '{}' to '{}'",
            current_status.as_str(),
            target_status.as_str(),
        )));
    }

    let from_str = current_status.as_str();
    let to_str = target_status.as_str();

    // 3. Update invoice status
    sqlx::query(
        "UPDATE invoices SET status = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3",
    )
    .bind(to_str)
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to update invoice status: {}", e))
    })?;

    // 4. Insert audit log row
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(Uuid::new_v4())
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(actor_id)
    .bind(from_str)
    .bind(to_str)
    .bind(event_type)
    .bind(&metadata)
    .execute(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to insert audit log: {}", e)))?;

    // 5. Commit
    tx.commit().await.map_err(|e| {
        billforge_core::Error::Database(format!("Failed to commit transition: {}", e))
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new().route("/:id/transition", axum::routing::post(transition_handler))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransitionRequest {
    pub to_status: InvoiceStatus,
    pub event_type: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TransitionResponse {
    pub invoice_id: String,
    pub from_status: String,
    pub to_status: String,
    pub event_type: String,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

async fn transition_handler(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(body): Json<TransitionRequest>,
) -> ApiResult<impl IntoResponse> {
    let invoice_id: Uuid = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let metadata = body.metadata.unwrap_or(serde_json::json!({}));

    // Read current status + invoice date for the response and lock check
    let current: (String, Option<String>) = sqlx::query_as(
        "SELECT status, invoice_date::text FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice_id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to query invoice: {}", e)))?;

    let from_status = current.0.clone();

    // Period lock guard: reject transitions on invoices in locked periods
    if let Some(ref inv_date) = current.1 {
        if let Some(_locked_period_id) = crate::routes::close_periods::find_locked_period_for_date(
            &pool,
            &tenant.tenant_id,
            inv_date,
        )
        .await?
        {
            return Err(billforge_core::Error::Conflict(
                "period_locked: Invoice belongs to a locked period and cannot be modified"
                    .to_string(),
            )
            .into());
        }
    }

    // Budget guardrail check: block or warn on approval transitions
    if body.to_status == InvoiceStatus::Approved {
        let budget_check = crate::routes::budgets::check_invoice_against_budgets(
            &pool,
            *tenant.tenant_id.as_uuid(),
            invoice_id,
        )
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Budget check failed: {}", e)))?;

        if budget_check.blocked {
            return Err(billforge_core::Error::Conflict(format!(
                "BUDGET_EXCEEDED: {}",
                serde_json::to_string(&budget_check.violations)
                    .unwrap_or_else(|_| "budget exceeded".to_string())
            ))
            .into());
        }

        // Attach budget warnings to metadata for audit trail
        if !budget_check.warnings.is_empty() || !budget_check.results.is_empty() {
            // Log budget check audit entry
            if let Ok(mut meta) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
                metadata.clone(),
            ) {
                meta.insert(
                    "budget_check".to_string(),
                    serde_json::json!({
                        "warnings": budget_check.warnings,
                        "violations": budget_check.violations,
                    }),
                );
                let _ = sqlx::query(
                    "INSERT INTO invoice_audit_log \
                     (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata) \
                     VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'budget_check_performed', $5)",
                )
                .bind(Uuid::new_v4())
                .bind(*tenant.tenant_id.as_uuid())
                .bind(invoice_id)
                .bind(*user.user_id.as_uuid())
                .bind(serde_json::Value::Object(meta))
                .execute(&*pool)
                .await;
            }
        }
    }

    transition(
        &pool,
        &tenant.tenant_id,
        &invoice_id,
        user.user_id.as_uuid(),
        body.to_status,
        &body.event_type,
        metadata,
    )
    .await?;

    let response = TransitionResponse {
        invoice_id: id,
        from_status,
        to_status: body.to_status.as_str().to_string(),
        event_type: body.event_type,
    };

    Ok((StatusCode::OK, Json(response)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transition_received_to_in_review() {
        let etype = find_transition(InvoiceStatus::Received, InvoiceStatus::InReview);
        assert_eq!(etype, Some("start_review"));
    }

    #[test]
    fn test_valid_transition_pending_approval_to_approved() {
        let etype = find_transition(InvoiceStatus::PendingApproval, InvoiceStatus::Approved);
        assert_eq!(etype, Some("approve"));
    }

    #[test]
    fn test_auto_approve_received_to_approved_allowed() {
        let etype = find_transition(InvoiceStatus::Received, InvoiceStatus::Approved);
        assert_eq!(etype, Some("auto_approve"));
    }

    #[test]
    fn test_invalid_transition_paid_to_received() {
        let etype = find_transition(InvoiceStatus::Paid, InvoiceStatus::Received);
        assert!(etype.is_none());
    }

    #[test]
    fn test_invalid_transition_paid_to_in_review() {
        let etype = find_transition(InvoiceStatus::Paid, InvoiceStatus::InReview);
        assert!(etype.is_none());
    }

    #[test]
    fn test_valid_void_from_approved() {
        let etype = find_transition(InvoiceStatus::Approved, InvoiceStatus::Void);
        assert_eq!(etype, Some("void"));
    }

    #[test]
    fn test_valid_send_back() {
        let etype = find_transition(InvoiceStatus::PendingApproval, InvoiceStatus::InReview);
        assert_eq!(etype, Some("send_back"));
    }

    #[test]
    fn test_valid_reopen_rejected() {
        let etype = find_transition(InvoiceStatus::Rejected, InvoiceStatus::InReview);
        assert_eq!(etype, Some("reopen"));
    }

    #[test]
    fn test_status_round_trip() {
        for status in [
            InvoiceStatus::Received,
            InvoiceStatus::InReview,
            InvoiceStatus::PendingApproval,
            InvoiceStatus::Approved,
            InvoiceStatus::Rejected,
            InvoiceStatus::Paid,
            InvoiceStatus::Void,
        ] {
            assert_eq!(InvoiceStatus::from_str_lossy(status.as_str()), Some(status));
        }
    }
}
