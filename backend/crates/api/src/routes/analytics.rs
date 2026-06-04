//! Analytics API Routes
//!
//! Authenticated, tenant-scoped analytics endpoints. Tenant identity is derived
//! from the authenticated user context (AuthUser) - never from path parameters.
//! All queries run against the tenant-scoped pool obtained via `state.db.tenant(...)`.

use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};

use crate::error::ApiResult;
use crate::extractors::AuthUser;
use crate::state::AppState;
use billforge_analytics::{
    models::{AnalyticsQuery, CreateEventRequest, PerformanceMetric, TrendData, UsageSummary},
    repository::AnalyticsRepository,
    service::AnalyticsService,
};

/// API routes for analytics (mounted under `/api/v1/analytics`)
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/events", post(track_event))
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/weekly", get(get_weekly_usage))
        .route("/usage/monthly", get(get_monthly_usage))
        .route("/usage", get(get_usage))
        .route("/performance", get(get_performance))
        .route("/trends", get(get_trends))
}

/// Track an analytics event. User identity comes from the auth context.
async fn track_event(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CreateEventRequest>,
) -> ApiResult<Json<billforge_analytics::models::AnalyticsEvent>> {
    let tenant_id = &user.0.tenant_id;
    let user_id = user.0.user_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let event = service
        .track_event(&tenant_id.to_string(), user_id.0, request)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(event))
}

/// Get daily usage summary (last 24 hours)
async fn get_daily_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_daily_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get weekly usage summary (last 7 days)
async fn get_weekly_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_weekly_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get monthly usage summary (last 30 days)
async fn get_monthly_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_monthly_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get custom usage summary with optional date range
async fn get_usage(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_usage_summary(&tenant_id.to_string(), start_date, end_date)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get performance metrics with optional date range
async fn get_performance(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<Vec<PerformanceMetric>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let metrics = service
        .get_performance_metrics(&tenant_id.to_string(), start_date, end_date)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(metrics))
}

/// Get trend analysis
async fn get_trends(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<TrendData>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let trends = service
        .get_trends(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(trends))
}
