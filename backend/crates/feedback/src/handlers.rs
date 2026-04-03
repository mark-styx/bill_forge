//! Feedback API Handlers
//!
//! HTTP endpoints for customer feedback.

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
use crate::service::FeedbackService;

/// Feedback API state
#[derive(Clone)]
pub struct FeedbackState {
    pub service: Arc<FeedbackService>,
}

impl FeedbackState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            service: Arc::new(FeedbackService::new(pool)),
        }
    }
}

/// Create feedback API router
pub fn create_router(pool: PgPool) -> axum::Router {
    let state = FeedbackState::new(pool);

    axum::Router::new()
        .route("/api/feedback", axum::routing::post(submit_feedback))
        .route("/api/feedback", axum::routing::get(list_feedback))
        .route("/api/feedback/stats", axum::routing::get(get_stats))
        .route(
            "/api/feedback/aggregation",
            axum::routing::get(get_aggregation),
        )
        .route(
            "/api/feedback/trend/weekly",
            axum::routing::get(get_weekly_trend),
        )
        .route(
            "/api/feedback/trend/monthly",
            axum::routing::get(get_monthly_trend),
        )
        .with_state(state)
}

/// Submit feedback
pub async fn submit_feedback(
    State(state): State<FeedbackState>,
    Path((tenant_id, user_id)): Path<(String, Uuid)>,
    Json(request): Json<SubmitFeedbackRequest>,
) -> Result<Json<Feedback>, ApiError> {
    let feedback = state.service.submit(&tenant_id, user_id, request).await?;
    Ok(Json(feedback))
}

/// List feedback
pub async fn list_feedback(
    State(state): State<FeedbackState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<FeedbackQuery>,
) -> Result<Json<Vec<Feedback>>, ApiError> {
    let feedback = state.service.list(&tenant_id, query, 100).await?;
    Ok(Json(feedback))
}

/// Get overall feedback statistics
pub async fn get_stats(
    State(state): State<FeedbackState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<FeedbackAggregation>, ApiError> {
    let stats = state.service.get_overall_stats(&tenant_id).await?;
    Ok(Json(stats))
}

/// Get feedback aggregation by category
pub async fn get_aggregation(
    State(state): State<FeedbackState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<FeedbackQuery>,
) -> Result<Json<Vec<FeedbackAggregation>>, ApiError> {
    let aggregation = state
        .service
        .get_aggregation(&tenant_id, query.category.as_deref())
        .await?;
    Ok(Json(aggregation))
}

/// Get weekly feedback trend
pub async fn get_weekly_trend(
    State(state): State<FeedbackState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<FeedbackTrend>, ApiError> {
    let trend = state.service.get_weekly_trend(&tenant_id).await?;
    Ok(Json(trend))
}

/// Get monthly feedback trend
pub async fn get_monthly_trend(
    State(state): State<FeedbackState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<FeedbackTrend>, ApiError> {
    let trend = state.service.get_monthly_trend(&tenant_id).await?;
    Ok(Json(trend))
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
