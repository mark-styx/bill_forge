//! Audit log API routes

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use billforge_core::{
    domain::AuditEntry,
    traits::AuditFilters,
    types::{Pagination, PaginatedResponse, Role},
    Error,
};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(list_audit_logs))
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    page: Option<u32>,
    per_page: Option<u32>,
    user_id: Option<String>,
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    from_date: Option<String>,
    to_date: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/audit",
    tag = "Audit",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page"),
        ("user_id" = Option<String>, Query, description = "Filter by user ID"),
        ("action" = Option<String>, Query, description = "Filter by action"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
    ),
    responses(
        (status = 200, description = "Audit log entries"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - admin only"),
    )
)]
async fn list_audit_logs(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(params): Query<AuditQueryParams>,
) -> ApiResult<Json<PaginatedResponse<AuditEntry>>> {
    // Only admins can view audit logs
    if !user.has_role(Role::TenantAdmin) {
        return Err(ApiError(Error::Forbidden(
            "Only administrators can view audit logs".to_string(),
        )));
    }

    let pagination = Pagination {
        page: params.page.unwrap_or(1),
        per_page: params.per_page.unwrap_or(50).min(100),
    };

    let filters = AuditFilters {
        user_id: params
            .user_id
            .and_then(|s| uuid::Uuid::parse_str(&s).ok())
            .map(billforge_core::types::UserId::from_uuid),
        action: params.action,
        resource_type: params.resource_type,
        resource_id: params.resource_id,
        from_date: params
            .from_date
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        to_date: params
            .to_date
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
    };

    let audit_service = state.audit_service();
    let result = audit_service.query(&user.tenant_id, filters, &pagination).await?;

    Ok(Json(result))
}
