//! HTTP handlers for health score API

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::models::{HealthBreakdown, HealthResponse, HealthScore};
use super::repository;
use super::scoring::calculate_health_score;

/// Application state for health module
#[derive(Clone)]
pub struct HealthState {
    pub pool: PgPool,
}

/// Create router for health score endpoints
pub fn create_router(pool: PgPool) -> Router {
    let state = HealthState { pool };

    Router::new()
        .route("/api/admin/tenants/health", get(list_health_handler))
        .route("/api/admin/tenants/:id/health", get(get_health_handler))
        .route(
            "/api/admin/tenants/:id/health/refresh",
            post(refresh_health_handler),
        )
        .with_state(state)
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/admin/tenants/health - List all tenant health scores
pub async fn list_health_handler(
    State(state): State<HealthState>,
) -> Result<Json<Vec<HealthScore>>, (StatusCode, Json<ErrorResponse>)> {
    match repository::list_health_scores(&state.pool).await {
        Ok(scores) => Ok(Json(scores)),
        Err(e) => {
            tracing::error!("List health scores error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to list health scores: {}", e),
                }),
            ))
        }
    }
}

/// GET /api/admin/tenants/:id/health - Get detailed health breakdown
pub async fn get_health_handler(
    State(state): State<HealthState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    match repository::get_health_score(&state.pool, &tenant_id).await {
        Ok(Some(score)) => {
            let response = HealthResponse {
                tenant_id: score.tenant_id,
                score: score.score,
                classification: score.classification,
                breakdown: HealthBreakdown {
                    usage_score: score.usage_score,
                    feature_adoption_score: score.feature_adoption_score,
                    error_rate_score: score.error_rate_score,
                    sentiment_score: score.sentiment_score,
                    payment_score: score.payment_score,
                },
                calculated_at: score.calculated_at,
            };
            Ok(Json(response))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Health score not found for tenant {}", tenant_id),
            }),
        )),
        Err(e) => {
            tracing::error!("Get health score error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get health score: {}", e),
                }),
            ))
        }
    }
}

/// POST /api/admin/tenants/:id/health/refresh - Recalculate health score
pub async fn refresh_health_handler(
    State(state): State<HealthState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Calculate new score
    let score = match calculate_health_score(&state.pool, &tenant_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Calculate health score error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to calculate health score: {}", e),
                }),
            ));
        }
    };

    // Save to database
    if let Err(e) = repository::save_health_score(&state.pool, &score).await {
        tracing::error!("Save health score error: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to save health score: {}", e),
            }),
        ));
    }

    // Return response
    let response = HealthResponse {
        tenant_id: score.tenant_id,
        score: score.score,
        classification: score.classification,
        breakdown: HealthBreakdown {
            usage_score: score.usage_score,
            feature_adoption_score: score.feature_adoption_score,
            error_rate_score: score.error_rate_score,
            sentiment_score: score.sentiment_score,
            payment_score: score.payment_score,
        },
        calculated_at: score.calculated_at,
    };

    Ok(Json(response))
}
