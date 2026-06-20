//! Continuous learning API routes (#404).
//!
//! Backs the "What I Learned This Week" tenant panel and the unified
//! correction-ingestion endpoint used by routing override, autopilot
//! override, and duplicate-dismissal flows.

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use billforge_core::{domain::AuditAction, Error};
use billforge_invoice_processing::{
    continuous_learning::monday_of, ContinuousLearningEngine, CorrectionType, WeeklyInsights,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/weekly_insights", get(get_weekly_insights))
        .route("/corrections", post(record_correction))
}

#[derive(Debug, Deserialize)]
pub struct WeekQuery {
    /// `YYYY-MM-DD`. Defaults to the Monday of the current ISO week (UTC).
    pub week_start: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WeeklyInsightsResponse {
    pub week_start: NaiveDate,
    pub insights: WeeklyInsights,
}

async fn get_weekly_insights(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Query(query): Query<WeekQuery>,
) -> ApiResult<Json<WeeklyInsightsResponse>> {
    let week_start = match query.week_start {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|_| Error::Validation("week_start must be YYYY-MM-DD".to_string()))?,
        None => monday_of(Utc::now().date_naive()),
    };

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ContinuousLearningEngine::new(*tenant.tenant_id.as_uuid(), (*pool).clone());
    let insights = engine
        .get_weekly_insights(week_start)
        .await
        .map_err(|e| Error::Internal(format!("Failed to load weekly insights: {}", e)))?;

    // Best-effort audit. Read events are noisy, so we log via the existing
    // pattern used by other read endpoints rather than persisting one per
    // call.
    tracing::info!(
        actor = %user.user_id,
        actor_email = %user.email,
        tenant_id = %tenant.tenant_id,
        week_start = %week_start,
        action = ?AuditAction::Read,
        "learning.weekly_insights.read"
    );

    Ok(Json(WeeklyInsightsResponse { week_start, insights }))
}

#[derive(Debug, Deserialize)]
pub struct RecordCorrectionRequest {
    pub correction_type: String,
    pub entity_id: Option<Uuid>,
    pub entity_type: String,
    pub original_value: serde_json::Value,
    pub corrected_value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct RecordCorrectionResponse {
    pub recorded: bool,
}

async fn record_correction(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(body): Json<RecordCorrectionRequest>,
) -> ApiResult<Json<RecordCorrectionResponse>> {
    let kind = CorrectionType::from_str(&body.correction_type).ok_or_else(|| {
        Error::Validation(format!(
            "Unknown correction_type '{}'",
            body.correction_type
        ))
    })?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ContinuousLearningEngine::new(*tenant.tenant_id.as_uuid(), (*pool).clone());
    engine
        .ingest_correction(
            kind,
            body.original_value,
            body.corrected_value,
            Some(*user.user_id.as_uuid()),
            body.entity_id,
            &body.entity_type,
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to ingest correction: {}", e)))?;

    tracing::info!(
        actor = %user.user_id,
        actor_email = %user.email,
        tenant_id = %tenant.tenant_id,
        correction_type = %body.correction_type,
        entity_type = %body.entity_type,
        entity_id = ?body.entity_id,
        "learning.corrections.recorded"
    );

    Ok(Json(RecordCorrectionResponse { recorded: true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_correction_request_deserializes() {
        let body: RecordCorrectionRequest = serde_json::from_str(
            r#"{"correction_type":"gl_recode","entity_id":"00000000-0000-0000-0000-000000000001","entity_type":"invoice_line","original_value":{},"corrected_value":{}}"#,
        )
        .unwrap();
        assert_eq!(body.correction_type, "gl_recode");
        assert_eq!(body.entity_type, "invoice_line");
    }

    #[test]
    fn unknown_week_start_format_is_rejected() {
        let parsed = NaiveDate::parse_from_str("not-a-date", "%Y-%m-%d");
        assert!(parsed.is_err());
    }
}
