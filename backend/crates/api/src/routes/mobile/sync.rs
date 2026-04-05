//! Mobile sync endpoints for offline support

use crate::extractors::{AuthUser, TenantCtx};
use crate::routes::mobile::{MobileInvoiceSummary, MobileVendorSummary, MobileApprovalRequest, MobileInvoiceStatus};
use crate::state::AppState;
use crate::ApiResult;
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Delta sync response
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub sync_timestamp: DateTime<Utc>,
    pub changes: EntityChanges,
    pub deleted: EntityDeletions,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct EntityChanges {
    pub invoices: Vec<MobileInvoiceSummary>,
    pub vendors: Vec<MobileVendorSummary>,
    pub approval_requests: Vec<MobileApprovalRequest>,
}

#[derive(Debug, Serialize)]
pub struct EntityDeletions {
    pub invoice_ids: Vec<Uuid>,
    pub vendor_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    pub last_sync_at: Option<DateTime<Utc>>,
    pub entity_types: Option<String>,
    pub limit: Option<i32>,
}

/// Delta sync - fetch changes since last sync
pub async fn sync_invoices(
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> ApiResult<Json<SyncResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let sync_timestamp = Utc::now();
    let limit = query.limit.unwrap_or(100).min(100);

    // If no last_sync_at, return empty (client should use bulk fetch first)
    let last_sync_at = match query.last_sync_at {
        Some(ts) => ts,
        None => {
            return Ok(Json(SyncResponse {
                sync_timestamp,
                changes: EntityChanges {
                    invoices: vec![],
                    vendors: vec![],
                    approval_requests: vec![],
                },
                deleted: EntityDeletions {
                    invoice_ids: vec![],
                    vendor_ids: vec![],
                },
                has_more: false,
            }));
        }
    };

    // Fetch modified invoices
    let invoice_rows = sqlx::query!(
        r#"
        SELECT id, vendor_name, invoice_number, total_amount_cents, currency, due_date, processing_status, modified_at
        FROM invoices
        WHERE tenant_id = $1
          AND modified_at > $2
        ORDER BY modified_at DESC
        LIMIT $3
        "#,
        &tenant.tenant_id.0,
        last_sync_at,
        limit as i64,
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let invoices: Vec<MobileInvoiceSummary> = invoice_rows
        .into_iter()
        .map(|row| MobileInvoiceSummary {
            id: row.id,
            vendor_name: row.vendor_name,
            invoice_number: row.invoice_number,
            total_amount_cents: row.total_amount_cents,
            currency: row.currency,
            due_date: row.due_date,
            status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
            days_until_due: row.due_date.map(|d| {
                (d - Utc::now().date_naive()).num_days() as i32
            }),
            requires_action: row.processing_status == "pending_approval",
            created_at: row.modified_at,
        })
        .collect();

    // Fetch deleted invoices from audit log
    // TODO: Implement audit log query for deleted invoices
    let deleted_invoice_ids: Vec<Uuid> = vec![];

    let has_more = invoices.len() == limit as usize;

    Ok(Json(SyncResponse {
        sync_timestamp,
        changes: EntityChanges {
            invoices,
            vendors: vec![],
            approval_requests: vec![],
        },
        deleted: EntityDeletions {
            invoice_ids: deleted_invoice_ids,
            vendor_ids: vec![],
        },
        has_more,
    }))
}

#[derive(Debug, Deserialize)]
pub struct BulkQuery {
    pub entity_types: Option<String>,
}

/// Bulk fetch - initial sync or full refresh
pub async fn sync_bulk(
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    State(state): State<AppState>,
    Query(query): Query<BulkQuery>,
) -> ApiResult<Json<SyncResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let sync_timestamp = Utc::now();

    // Parse entity types (default to all)
    let entity_types: Vec<&str> = query
        .entity_types
        .as_ref()
        .map(|s| s.split(',').collect())
        .unwrap_or_else(|| vec!["invoices", "vendors", "approval_requests"]);

    let should_fetch_invoices = entity_types.contains(&"invoices");
    let should_fetch_vendors = entity_types.contains(&"vendors");
    let should_fetch_approvals = entity_types.contains(&"approval_requests");

    // Fetch invoices
    let invoices = if should_fetch_invoices {
        let rows = sqlx::query!(
            r#"
            SELECT id, vendor_name, invoice_number, total_amount_cents, currency, due_date, processing_status, modified_at
            FROM invoices
            WHERE tenant_id = $1
            ORDER BY modified_at DESC
            LIMIT 500
            "#,
            &tenant.tenant_id.0,
        )
        .fetch_all(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| MobileInvoiceSummary {
                id: row.id,
                vendor_name: row.vendor_name,
                invoice_number: row.invoice_number,
                total_amount_cents: row.total_amount_cents,
                currency: row.currency,
                due_date: row.due_date,
                status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
                days_until_due: row.due_date.map(|d| {
                    (d - Utc::now().date_naive()).num_days() as i32
                }),
                requires_action: row.processing_status == "pending_approval",
                created_at: row.modified_at,
            })
            .collect()
    } else {
        vec![]
    };

    // Fetch vendors
    let vendors = if should_fetch_vendors {
        let rows = sqlx::query!(
            r#"
            SELECT id, name
            FROM vendors
            WHERE tenant_id = $1
            ORDER BY modified_at DESC
            LIMIT 500
            "#,
            &tenant.tenant_id.0,
        )
        .fetch_all(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| MobileVendorSummary {
                id: row.id,
                name: row.name,
                total_invoices: 0,  // TODO: Calculate
                total_amount_cents: 0,  // TODO: Calculate
            })
            .collect()
    } else {
        vec![]
    };

    // Fetch approval requests
    let approval_requests = if should_fetch_approvals {
        let rows = sqlx::query!(
            r#"
            SELECT
                ar.id,
                ar.invoice_id,
                i.vendor_name,
                i.invoice_number,
                i.total_amount_cents,
                i.currency,
                i.due_date,
                i.processing_status,
                ar.created_at
            FROM approval_requests ar
            JOIN invoices i ON ar.invoice_id = i.id
            WHERE ar.tenant_id = $1
              AND ar.requested_from->>'user_id' = $2
              AND ar.status = 'pending'
            ORDER BY ar.modified_at DESC
            LIMIT 100
            "#,
            tenant.tenant_id.0,
            user.user_id.0.to_string(),
        )
        .fetch_all(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| MobileApprovalRequest {
                id: row.id,
                invoice: MobileInvoiceSummary {
                    id: row.invoice_id,
                    vendor_name: row.vendor_name,
                    invoice_number: row.invoice_number,
                    total_amount_cents: row.total_amount_cents,
                    currency: row.currency,
                    due_date: row.due_date,
                    status: MobileInvoiceStatus::from_processing_status(&row.processing_status),
                    days_until_due: row.due_date.map(|d| {
                        (d - Utc::now().date_naive()).num_days() as i32
                    }),
                    requires_action: true,
                    created_at: row.created_at,
                },
                requested_at: row.created_at,
                expires_at: None,
                can_approve: true,
            })
            .collect()
    } else {
        vec![]
    };

    Ok(Json(SyncResponse {
        sync_timestamp,
        changes: EntityChanges {
            invoices,
            vendors,
            approval_requests,
        },
        deleted: EntityDeletions {
            invoice_ids: vec![],
            vendor_ids: vec![],
        },
        has_more: false,
    }))
}
