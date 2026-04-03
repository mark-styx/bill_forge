//! Analytics API Handlers
//!
//! HTTP endpoints for analytics data.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::*;
use crate::service::AnalyticsService;

/// Analytics API state
#[derive(Clone)]
pub struct AnalyticsState {
    pub service: Arc<AnalyticsService>,
}

impl AnalyticsState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            service: Arc::new(AnalyticsService::new(pool)),
        }
    }
}

/// Create analytics API router
pub fn create_router(pool: PgPool) -> axum::Router {
    let state = AnalyticsState::new(pool);

    axum::Router::new()
        .route("/api/analytics/events", axum::routing::post(track_event))
        .route(
            "/api/analytics/usage/daily",
            axum::routing::get(get_daily_usage),
        )
        .route(
            "/api/analytics/usage/weekly",
            axum::routing::get(get_weekly_usage),
        )
        .route(
            "/api/analytics/usage/monthly",
            axum::routing::get(get_monthly_usage),
        )
        .route("/api/analytics/usage", axum::routing::get(get_usage))
        .route(
            "/api/analytics/performance",
            axum::routing::get(get_performance),
        )
        .route("/api/analytics/trends", axum::routing::get(get_trends))
        .with_state(state)
}

/// Track an analytics event
pub async fn track_event(
    State(state): State<AnalyticsState>,
    Path((tenant_id, user_id)): Path<(String, Uuid)>,
    Json(request): Json<CreateEventRequest>,
) -> Result<Json<AnalyticsEvent>, ApiError> {
    let event = state
        .service
        .track_event(&tenant_id, user_id, request)
        .await?;
    Ok(Json(event))
}

/// Get daily usage summary
pub async fn get_daily_usage(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<UsageSummary>, ApiError> {
    let summary = state.service.get_daily_summary(&tenant_id).await?;
    Ok(Json(summary))
}

/// Get weekly usage summary
pub async fn get_weekly_usage(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<UsageSummary>, ApiError> {
    let summary = state.service.get_weekly_summary(&tenant_id).await?;
    Ok(Json(summary))
}

/// Get monthly usage summary
pub async fn get_monthly_usage(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<UsageSummary>, ApiError> {
    let summary = state.service.get_monthly_summary(&tenant_id).await?;
    Ok(Json(summary))
}

/// Get custom usage summary
pub async fn get_usage(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<UsageSummary>, ApiError> {
    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let summary = state
        .service
        .get_usage_summary(&tenant_id, start_date, end_date)
        .await?;
    Ok(Json(summary))
}

/// Get performance metrics
pub async fn get_performance(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<Vec<PerformanceMetric>>, ApiError> {
    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let metrics = state
        .service
        .get_performance_metrics(&tenant_id, start_date, end_date)
        .await?;
    Ok(Json(metrics))
}

/// Get trend analysis
pub async fn get_trends(
    State(state): State<AnalyticsState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<TrendData>>, ApiError> {
    let trends = state.service.get_trends(&tenant_id).await?;
    Ok(Json(trends))
}

/// API Error type
#[derive(Debug)]
pub struct ApiError(pub anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": self.0.to_string()
        });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
    }
}
