//! Vendor statement reconciliation routes

use crate::error::ApiResult;
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use billforge_core::domain::vendor_statement::*;
use billforge_core::types::PaginationMeta;
use billforge_db::repositories::VendorStatementRepositoryImpl;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/vendors/:vendor_id/statements",
            post(create_statement),
        )
        .route(
            "/vendors/:vendor_id/statements",
            get(list_statements),
        )
        .route(
            "/vendors/:vendor_id/statements/:statement_id",
            get(get_statement),
        )
        .route(
            "/vendors/:vendor_id/statements/:statement_id/match",
            post(run_auto_match),
        )
        .route(
            "/vendors/:vendor_id/statements/:statement_id/lines/:line_id",
            put(update_line),
        )
        .route(
            "/vendors/:vendor_id/statements/:statement_id/reconcile",
            post(reconcile_statement),
        )
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    page: Option<u32>,
    per_page: Option<u32>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct StatementListResponse {
    data: Vec<VendorStatement>,
    pagination: PaginationMeta,
}

#[derive(Debug, Serialize)]
struct StatementDetailResponse {
    statement: VendorStatement,
    lines: Vec<StatementLineItem>,
    summary: ReconciliationSummary,
}

#[derive(Debug, Serialize)]
struct MatchResponse {
    results: Vec<MatchResult>,
    summary: ReconciliationSummary,
}

#[utoipa::path(post, path = "/api/v1/vendors/{vendor_id}/statements", tag = "Vendor Statements", request_body = serde_json::Value,
    params(("vendor_id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Statement created")))]
async fn create_statement(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(vendor_id): Path<Uuid>,
    Json(input): Json<CreateStatementInput>,
) -> ApiResult<Json<StatementDetailResponse>> {
    if input.vendor_id != vendor_id {
        return Err(
            billforge_core::Error::Validation(
                "Vendor ID in path does not match input".to_string(),
            )
            .into(),
        );
    }

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool.clone());

    let statement = repo
        .create_statement(&tenant.tenant_id, input, user.user_id.0)
        .await?;

    let lines = repo.get_lines(&tenant.tenant_id, statement.id.0).await?;

    let invoices = repo
        .get_vendor_invoices_in_range(
            &tenant.tenant_id,
            vendor_id,
            statement.period_start,
            statement.period_end,
        )
        .await?;

    let match_results = auto_match_lines(&lines, &invoices);
    repo.apply_match_results(&tenant.tenant_id, &match_results).await?;

    let lines = repo.get_lines(&tenant.tenant_id, statement.id.0).await?;
    let summary = compute_reconciliation_summary(&lines);

    Ok(Json(StatementDetailResponse {
        statement,
        lines,
        summary,
    }))
}

#[utoipa::path(get, path = "/api/v1/vendors/{vendor_id}/statements", tag = "Vendor Statements",
    params(("vendor_id" = String, Path,)),
    responses((status = 200, description = "Statement list")))]
async fn list_statements(
    State(state): State<AppState>,
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    Path(vendor_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<StatementListResponse>> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool);

    let (statements, total) = repo
        .list_statements(
            &tenant.tenant_id,
            vendor_id,
            page,
            per_page,
            query.status.as_deref(),
        )
        .await?;

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;

    Ok(Json(StatementListResponse {
        data: statements,
        pagination: PaginationMeta {
            page,
            per_page,
            total_items: total,
            total_pages,
        },
    }))
}

#[utoipa::path(get, path = "/api/v1/vendors/{vendor_id}/statements/{statement_id}", tag = "Vendor Statements",
    params(("vendor_id" = String, Path,), ("statement_id" = String, Path,)),
    responses((status = 200, description = "Statement details"), (status = 404, description = "Not found")))]
async fn get_statement(
    State(state): State<AppState>,
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    Path((_vendor_id, statement_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<StatementDetailResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool);

    let statement = repo
        .get_statement(&tenant.tenant_id, statement_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "VendorStatement".to_string(),
            id: statement_id.to_string(),
        })?;

    let lines = repo.get_lines(&tenant.tenant_id, statement_id).await?;
    let summary = compute_reconciliation_summary(&lines);

    Ok(Json(StatementDetailResponse {
        statement,
        lines,
        summary,
    }))
}

#[utoipa::path(post, path = "/api/v1/vendors/{vendor_id}/statements/{statement_id}/match", tag = "Vendor Statements", request_body = serde_json::Value,
    params(("vendor_id" = String, Path,), ("statement_id" = String, Path,)),
    responses((status = 200, description = "Match results")))]
async fn run_auto_match(
    State(state): State<AppState>,
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    Path((vendor_id, statement_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<MatchResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool.clone());

    let statement = repo
        .get_statement(&tenant.tenant_id, statement_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "VendorStatement".to_string(),
            id: statement_id.to_string(),
        })?;

    let lines = repo.get_lines(&tenant.tenant_id, statement_id).await?;
    let invoices = repo
        .get_vendor_invoices_in_range(
            &tenant.tenant_id,
            vendor_id,
            statement.period_start,
            statement.period_end,
        )
        .await?;

    let results = auto_match_lines(&lines, &invoices);
    repo.apply_match_results(&tenant.tenant_id, &results).await?;

    let lines = repo.get_lines(&tenant.tenant_id, statement_id).await?;
    let summary = compute_reconciliation_summary(&lines);

    Ok(Json(MatchResponse { results, summary }))
}

#[utoipa::path(put, path = "/api/v1/vendors/{vendor_id}/statements/{statement_id}/lines/{line_id}", tag = "Vendor Statements", request_body = serde_json::Value,
    params(("vendor_id" = String, Path,), ("statement_id" = String, Path,), ("line_id" = String, Path,)),
    responses((status = 200, description = "Line updated")))]
async fn update_line(
    State(state): State<AppState>,
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    Path((vendor_id, statement_id, line_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(input): Json<UpdateLineMatchInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool);

    // Verify the statement belongs to this tenant before modifying its line
    let _statement = repo
        .get_statement(&tenant.tenant_id, statement_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "VendorStatement".to_string(),
            id: statement_id.to_string(),
        })?;

    // Fetch the line to get its amount for variance calculation
    let line = repo
        .get_line(&tenant.tenant_id, line_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "StatementLine".to_string(),
            id: line_id.to_string(),
        })?;

    // Validate invoice ownership and compute variance
    let variance = if let Some(invoice_id) = input.matched_invoice_id {
        let invoice_amount = repo
            .validate_invoice_ownership(&tenant.tenant_id, invoice_id, vendor_id)
            .await?
            .ok_or_else(|| billforge_core::Error::Validation(
                "Matched invoice not found or does not belong to this vendor".to_string(),
            ))?;
        line.amount_cents - invoice_amount
    } else {
        0
    };

    repo.update_line_match(
        &tenant.tenant_id,
        line_id,
        input.matched_invoice_id,
        variance,
        &input.match_status,
        "manual",
    )
    .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/vendors/{vendor_id}/statements/{statement_id}/reconcile", tag = "Vendor Statements", request_body = serde_json::Value,
    params(("vendor_id" = String, Path,), ("statement_id" = String, Path,)),
    responses((status = 200, description = "Statement reconciled")))]
async fn reconcile_statement(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path((_vendor_id, statement_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = VendorStatementRepositoryImpl::new(pool);

    // Verify statement exists
    let _statement = repo
        .get_statement(&tenant.tenant_id, statement_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "VendorStatement".to_string(),
            id: statement_id.to_string(),
        })?;

    // Check all lines are matched, discrepancy, or ignored
    let lines = repo.get_lines(&tenant.tenant_id, statement_id).await?;
    let has_unresolved = lines.iter().any(|l| {
        l.match_status != LineMatchStatus::Matched
            && l.match_status != LineMatchStatus::Ignored
            && l.match_status != LineMatchStatus::Discrepancy
    });

    if has_unresolved {
        return Err(billforge_core::Error::Validation(
            "Cannot reconcile: unresolved lines remain. All lines must be matched, discrepancy, or ignored."
                .to_string(),
        )
        .into());
    }

    repo.update_statement_status(
        &tenant.tenant_id,
        statement_id,
        &StatementStatus::Reconciled,
        Some(user.user_id.0),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "status": "reconciled"
    })))
}
