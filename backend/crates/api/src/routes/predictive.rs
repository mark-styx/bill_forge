//! Predictive Analytics API Routes
//!
//! HTTP endpoints for forecasting, anomaly detection, and proactive alerts.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc, TimeZone};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use billforge_analytics::{
    forecasting::ArimaForecaster,
    anomaly_detection::{DuplicateDetector, InvoiceRecord, StatisticalAnomalyDetector},
    predictive_models::*,
};
// Error type is accessed via billforge_core::Error to allow From trait conversion

use crate::error::ApiResult;
use crate::extractors::AuthUser;
use crate::state::AppState;

/// Query parameters for forecast requests
#[derive(Debug, Deserialize)]
pub struct ForecastQuery {
    pub entity_type: String,
    pub entity_id: String,
    pub horizon: String,
}

/// Request to acknowledge an anomaly
#[derive(Debug, Deserialize)]
pub struct AcknowledgeAnomalyRequest {
    pub notes: Option<String>,
}

/// Request to configure anomaly rules
#[derive(Debug, Deserialize)]
pub struct ConfigureAnomalyRuleRequest {
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub anomaly_type: String,
    pub zscore_threshold: Option<f64>,
    pub iqr_multiplier: Option<f64>,
    pub volume_spike_threshold: Option<f64>,
    pub notification_channels: Option<Vec<String>>,
    pub notify_on_severity: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// API routes for predictive analytics
pub fn routes() -> Router<AppState> {
    Router::new()
        // Forecasts
        .route("/forecasts", get(get_forecasts))
        .route("/forecasts/generate", post(generate_forecast))
        .route("/forecasts/:forecast_id", get(get_forecast_by_id))
        // Anomalies
        .route("/anomalies", get(get_anomalies))
        .route("/anomalies/:anomaly_id/acknowledge", post(acknowledge_anomaly))
        .route("/anomalies/detect", post(detect_anomalies))
        // Budget alerts
        .route("/alerts", get(get_budget_alerts))
        .route("/alerts/:alert_id/dismiss", post(dismiss_alert))
        // Anomaly rules configuration
        .route("/rules", get(get_anomaly_rules))
        .route("/rules", post(configure_anomaly_rule))
        .route("/rules/:rule_id", get(get_anomaly_rule))
        .route("/rules/:rule_id", post(update_anomaly_rule))
}

/// Get forecasts for tenant
// TODO: Add utoipa documentation once ToSchema is implemented for Forecast
// #[utoipa::path(
//     get,
//     path = "/api/v1/analytics/predictive/forecasts",
//     responses(
//         (status = 200, description = "Forecasts retrieved", body = Vec<Forecast>),
//         (status = 401, description = "Unauthorized"),
//         (status = 500, description = "Internal server error")
//     ),
//     security(("bearer_auth" = []))
// )]
#[utoipa::path(get, path = "/api/v1/analytics/predictive/forecasts", tag = "Predictive Analytics", responses((status = 200, description = "Forecasts")))]
async fn get_forecasts(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ForecastQuery>,
) -> ApiResult<Json<Vec<Forecast>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    // Parse horizon
    let horizon = match query.horizon.as_str() {
        "30" | "days_30" => ForecastHorizon::Days30,
        "60" | "days_60" => ForecastHorizon::Days60,
        "90" | "days_90" => ForecastHorizon::Days90,
        _ => return Err(billforge_core::Error::Validation("Invalid horizon. Use 30, 60, or 90".to_string()).into()),
    };

    // Fetch forecasts from database
    let horizon_str = serde_json::to_string(&horizon).map_err(|e| billforge_core::Error::Internal(e.to_string()))?;
    let row = sqlx::query(
        r#"
        SELECT
            entity_id,
            entity_type,
            metric_name,
            horizon,
            predicted_value,
            confidence_lower,
            confidence_upper,
            confidence_level,
            generated_at,
            model_version,
            seasonality_detected
        FROM spend_forecasts
        WHERE tenant_id = $1
            AND entity_id = $2
            AND entity_type = $3
            AND horizon = $4
            AND valid_until > NOW()
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id.0)
    .bind(&query.entity_id)
    .bind(&query.entity_type)
    .bind(&horizon_str)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let forecasts = match row {
        Some(row) => {
            let entity_type: String = row.try_get("entity_type").map_err(|e| billforge_core::Error::Database(e.to_string()))?;
            let entity_type: EntityType = serde_json::from_str(&format!("\"{}\"", entity_type))
                .map_err(|e| billforge_core::Error::Internal(format!("Invalid entity_type: {}", e)))?;

            let horizon_str: String = row.try_get("horizon").map_err(|e| billforge_core::Error::Database(e.to_string()))?;
            let horizon: ForecastHorizon = serde_json::from_str(&horizon_str)
                .map_err(|e| billforge_core::Error::Internal(format!("Invalid horizon: {}", e)))?;

            vec![Forecast {
                entity_id: row.try_get("entity_id").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                entity_type,
                metric_name: row.try_get("metric_name").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                horizon,
                predicted_value: row.try_get("predicted_value").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                confidence_lower: row.try_get("confidence_lower").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                confidence_upper: row.try_get("confidence_upper").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                confidence_level: row.try_get("confidence_level").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                generated_at: row.try_get("generated_at").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                model_version: row.try_get("model_version").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
                seasonality_detected: row.try_get("seasonality_detected").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
            }]
        }
        None => vec![],
    };

    Ok(Json(forecasts))
}

/// Generate a new forecast
// TODO: Add utoipa documentation once ToSchema is implemented
#[utoipa::path(post, path = "/api/v1/analytics/predictive/forecasts", tag = "Predictive Analytics", request_body = serde_json::Value, responses((status = 200, description = "Forecast generated")))]
async fn generate_forecast(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ForecastQuery>,
) -> ApiResult<Json<Forecast>> {
    let tenant_id = &user.0.tenant_id;

    // Fetch historical data for the entity
    let entity_type = match payload.entity_type.as_str() {
        "vendor" => EntityType::Vendor,
        "department" => EntityType::Department,
        "gl_code" => EntityType::GlCode,
        "tenant" => EntityType::Tenant,
        "approver" => EntityType::Approver,
        _ => return Err(billforge_core::Error::Validation("Invalid entity_type".to_string()).into()),
    };

    let horizon = match payload.horizon.as_str() {
        "30" | "days_30" => ForecastHorizon::Days30,
        "60" | "days_60" => ForecastHorizon::Days60,
        "90" | "days_90" => ForecastHorizon::Days90,
        _ => return Err(billforge_core::Error::Validation("Invalid horizon".to_string()).into()),
    };

    // Fetch historical spend data
    let historical_data = fetch_historical_spend(&state.db, tenant_id.0, &payload.entity_id, entity_type)
        .await?;

    if historical_data.points.len() < 30 {
        return Err(billforge_core::Error::Validation(
            "Insufficient historical data. Need at least 30 days of data.".to_string(),
        ).into());
    }

    // Generate forecast using ARIMA model
    let mut forecaster = ArimaForecaster::new();
    forecaster
        .fit(&historical_data)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    let forecast = forecaster
        .forecast(horizon)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    // Store forecast in database
    store_forecast(&state.db, tenant_id.0, &forecast).await?;

    Ok(Json(forecast))
}

/// Get forecast by ID
#[utoipa::path(get, path = "/api/v1/analytics/predictive/forecasts/{id}", tag = "Predictive Analytics", params(("id" = String, Path,)), responses((status = 200, description = "Forecast detail")))]
async fn get_forecast_by_id(
    State(state): State<AppState>,
    user: AuthUser,
    Path(forecast_id): Path<Uuid>,
) -> ApiResult<Json<Forecast>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let row = sqlx::query(
        r#"
        SELECT
            entity_id,
            entity_type,
            metric_name,
            horizon,
            predicted_value,
            confidence_lower,
            confidence_upper,
            confidence_level,
            generated_at,
            model_version,
            seasonality_detected
        FROM spend_forecasts
        WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(forecast_id)
    .bind(tenant_id.0)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Forecast".to_string(),
        id: forecast_id.to_string(),
    })?;

    let entity_type: String = row.try_get("entity_type").map_err(|e| billforge_core::Error::Database(e.to_string()))?;
    let entity_type: EntityType = serde_json::from_str(&format!("\"{}\"", entity_type))
        .map_err(|e| billforge_core::Error::Internal(format!("Invalid entity_type: {}", e)))?;

    let horizon_str: String = row.try_get("horizon").map_err(|e| billforge_core::Error::Database(e.to_string()))?;
    let horizon: ForecastHorizon = serde_json::from_str(&horizon_str)
        .map_err(|e| billforge_core::Error::Internal(format!("Invalid horizon: {}", e)))?;

    let forecast = Forecast {
        entity_id: row.try_get("entity_id").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        entity_type,
        metric_name: row.try_get("metric_name").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        horizon,
        predicted_value: row.try_get("predicted_value").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        confidence_lower: row.try_get("confidence_lower").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        confidence_upper: row.try_get("confidence_upper").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        confidence_level: row.try_get("confidence_level").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        generated_at: row.try_get("generated_at").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        model_version: row.try_get("model_version").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        seasonality_detected: row.try_get("seasonality_detected").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
    };

    Ok(Json(forecast))
}

/// Get anomalies for tenant
// TODO: Add utoipa documentation once ToSchema is implemented
#[utoipa::path(get, path = "/api/v1/analytics/predictive/anomalies", tag = "Predictive Analytics", responses((status = 200, description = "Detected anomalies")))]
async fn get_anomalies(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<Anomaly>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT
            id,
            tenant_id,
            anomaly_type,
            entity_id,
            entity_type,
            severity,
            detected_value,
            expected_range_min,
            expected_range_max,
            deviation_score,
            detected_at,
            metadata,
            acknowledged,
            acknowledged_at,
            acknowledged_by
        FROM invoice_anomalies
        WHERE tenant_id = $1
        ORDER BY detected_at DESC
        LIMIT 100
        "#,
    )
    .bind(tenant_id.0)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Transform to expected format
    let anomalies = rows
        .into_iter()
        .filter_map(|row| {
            let anomaly_type_str: String = row.try_get("anomaly_type").ok()?;
            let anomaly_type: AnomalyType = serde_json::from_str(&format!("\"{}\"", anomaly_type_str)).ok()?;

            let entity_type_str: String = row.try_get("entity_type").ok()?;
            let entity_type: EntityType = serde_json::from_str(&format!("\"{}\"", entity_type_str)).ok()?;

            let severity_str: String = row.try_get("severity").ok()?;
            let severity: AnomalySeverity = serde_json::from_str(&format!("\"{}\"", severity_str)).ok()?;

            Some(Anomaly {
                id: row.try_get("id").ok()?,
                tenant_id: row.try_get("tenant_id").ok()?,
                anomaly_type,
                entity_id: row.try_get("entity_id").ok()?,
                entity_type,
                severity,
                detected_value: row.try_get("detected_value").ok()?,
                expected_range: (
                    row.try_get("expected_range_min").ok()?,
                    row.try_get("expected_range_max").ok()?,
                ),
                deviation_score: row.try_get("deviation_score").ok()?,
                detected_at: row.try_get("detected_at").ok()?,
                metadata: row.try_get("metadata").ok()?,
                acknowledged: row.try_get("acknowledged").ok()?,
                acknowledged_at: row.try_get("acknowledged_at").ok()?,
                acknowledged_by: row.try_get("acknowledged_by").ok()?,
            })
        })
        .collect();

    Ok(Json(anomalies))
}

/// Acknowledge an anomaly
#[utoipa::path(post, path = "/api/v1/analytics/predictive/anomalies/{id}/acknowledge", tag = "Predictive Analytics", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Anomaly acknowledged")))]
async fn acknowledge_anomaly(
    State(state): State<AppState>,
    user: AuthUser,
    Path(anomaly_id): Path<Uuid>,
    Json(_payload): Json<AcknowledgeAnomalyRequest>,
) -> ApiResult<Json<()>> {
    let tenant_id = &user.0.tenant_id;
    let user_id = &user.0.user_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let result = sqlx::query(
        r#"
        UPDATE invoice_anomalies
        SET
            acknowledged = TRUE,
            acknowledged_at = NOW(),
            acknowledged_by = $1,
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        "#,
    )
    .bind(user_id.0)
    .bind(anomaly_id)
    .bind(tenant_id.0)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "Anomaly".to_string(),
            id: anomaly_id.to_string(),
        }.into());
    }

    Ok(Json(()))
}

/// Detect anomalies (manual trigger)
#[utoipa::path(post, path = "/api/v1/analytics/predictive/anomalies/detect", tag = "Predictive Analytics", request_body = serde_json::Value, responses((status = 200, description = "Anomaly detection triggered")))]
async fn detect_anomalies(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<Anomaly>>> {
    let tenant_id = &user.0.tenant_id;

    // Fetch recent invoices for anomaly detection
    let invoices = fetch_recent_invoices(&state.db, tenant_id.0).await?;

    // Run duplicate detection
    let duplicate_detector = DuplicateDetector::new(tenant_id.0);
    let duplicate_anomalies = duplicate_detector
        .detect_duplicates(&invoices)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    // Store anomalies in database
    for anomaly in &duplicate_anomalies {
        store_anomaly(&state.db, anomaly).await?;
    }

    Ok(Json(duplicate_anomalies))
}

/// Get budget alerts
#[utoipa::path(get, path = "/api/v1/analytics/predictive/budget-alerts", tag = "Predictive Analytics", responses((status = 200, description = "Budget alerts")))]
async fn get_budget_alerts(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT id, alert_type, severity, entity_id, entity_type,
               title, message, threshold_value, current_value,
               threshold_percentage, recommended_action,
               triggered_at, dismissed
        FROM budget_alerts
        WHERE tenant_id = $1 AND dismissed = FALSE
        ORDER BY triggered_at DESC
        LIMIT 50
        "#,
    )
    .bind(tenant_id.0)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let alerts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.try_get::<Uuid, _>("id").ok(),
                "alert_type": row.try_get::<String, _>("alert_type").ok(),
                "severity": row.try_get::<String, _>("severity").ok(),
                "entity_id": row.try_get::<String, _>("entity_id").ok(),
                "entity_type": row.try_get::<String, _>("entity_type").ok(),
                "title": row.try_get::<String, _>("title").ok(),
                "message": row.try_get::<String, _>("message").ok(),
                "threshold_value": row.try_get::<f64, _>("threshold_value").ok(),
                "current_value": row.try_get::<f64, _>("current_value").ok(),
                "threshold_percentage": row.try_get::<f64, _>("threshold_percentage").ok(),
                "recommended_action": row.try_get::<Option<String>, _>("recommended_action").ok(),
                "triggered_at": row.try_get::<DateTime<Utc>, _>("triggered_at").ok(),
                "dismissed": row.try_get::<bool, _>("dismissed").ok(),
            })
        })
        .collect();

    Ok(Json(serde_json::to_value(alerts).map_err(|e| billforge_core::Error::Internal(e.to_string()))?))
}

/// Dismiss a budget alert
#[utoipa::path(post, path = "/api/v1/analytics/predictive/budget-alerts/{id}/dismiss", tag = "Predictive Analytics", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Alert dismissed")))]
async fn dismiss_alert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(alert_id): Path<Uuid>,
) -> ApiResult<Json<()>> {
    let tenant_id = &user.0.tenant_id;
    let user_id = &user.0.user_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let result = sqlx::query(
        r#"
        UPDATE budget_alerts
        SET dismissed = TRUE, dismissed_at = NOW(), dismissed_by = $1, updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        "#,
    )
    .bind(user_id.0)
    .bind(alert_id)
    .bind(tenant_id.0)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "Alert".to_string(),
            id: alert_id.to_string(),
        }.into());
    }

    Ok(Json(()))
}

/// Get anomaly rules
#[utoipa::path(get, path = "/api/v1/analytics/predictive/anomaly-rules", tag = "Predictive Analytics", responses((status = 200, description = "Anomaly rules")))]
async fn get_anomaly_rules(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT id, entity_type, entity_id, anomaly_type,
               zscore_threshold, iqr_multiplier, volume_spike_threshold,
               notification_channels, notify_on_severity, enabled
        FROM anomaly_rules
        WHERE tenant_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant_id.0)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let rules: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.try_get::<Uuid, _>("id").ok(),
                "entity_type": row.try_get::<Option<String>, _>("entity_type").ok(),
                "entity_id": row.try_get::<Option<String>, _>("entity_id").ok(),
                "anomaly_type": row.try_get::<String, _>("anomaly_type").ok(),
                "zscore_threshold": row.try_get::<Option<f64>, _>("zscore_threshold").ok(),
                "iqr_multiplier": row.try_get::<Option<f64>, _>("iqr_multiplier").ok(),
                "volume_spike_threshold": row.try_get::<Option<f64>, _>("volume_spike_threshold").ok(),
                "notification_channels": row.try_get::<Option<serde_json::Value>, _>("notification_channels").ok(),
                "notify_on_severity": row.try_get::<Option<serde_json::Value>, _>("notify_on_severity").ok(),
                "enabled": row.try_get::<bool, _>("enabled").ok(),
            })
        })
        .collect();

    Ok(Json(serde_json::to_value(rules).map_err(|e| billforge_core::Error::Internal(e.to_string()))?))
}

/// Configure anomaly rule
#[utoipa::path(post, path = "/api/v1/analytics/predictive/anomaly-rules", tag = "Predictive Analytics", request_body = serde_json::Value, responses((status = 200, description = "Rule configured")))]
async fn configure_anomaly_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ConfigureAnomalyRuleRequest>,
) -> ApiResult<Json<Uuid>> {
    let tenant_id = &user.0.tenant_id;
    let user_id = &user.0.user_id;
    let rule_id = Uuid::new_v4();
    let pool = state.db.tenant(&tenant_id).await?;

    let notification_channels = serde_json::to_value(payload.notification_channels.unwrap_or_default())
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;
    let notify_on_severity = serde_json::to_value(payload.notify_on_severity.unwrap_or_default())
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO anomaly_rules (
            id, tenant_id, entity_type, entity_id, anomaly_type,
            zscore_threshold, iqr_multiplier, volume_spike_threshold,
            notification_channels, notify_on_severity, enabled, created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (tenant_id, anomaly_type, entity_type, entity_id)
        DO UPDATE SET
            zscore_threshold = EXCLUDED.zscore_threshold,
            iqr_multiplier = EXCLUDED.iqr_multiplier,
            volume_spike_threshold = EXCLUDED.volume_spike_threshold,
            notification_channels = EXCLUDED.notification_channels,
            notify_on_severity = EXCLUDED.notify_on_severity,
            enabled = EXCLUDED.enabled,
            updated_at = NOW()
        "#,
    )
    .bind(rule_id)
    .bind(tenant_id.0)
    .bind(&payload.entity_type)
    .bind(&payload.entity_id)
    .bind(&payload.anomaly_type)
    .bind(payload.zscore_threshold)
    .bind(payload.iqr_multiplier)
    .bind(payload.volume_spike_threshold)
    .bind(&notification_channels)
    .bind(&notify_on_severity)
    .bind(payload.enabled.unwrap_or(true))
    .bind(user_id.0)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(Json(rule_id))
}

/// Get anomaly rule by ID
#[utoipa::path(get, path = "/api/v1/analytics/predictive/anomaly-rules/{id}", tag = "Predictive Analytics", params(("id" = String, Path,)), responses((status = 200, description = "Anomaly rule")))]
async fn get_anomaly_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Path(rule_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let row = sqlx::query(
        r#"
        SELECT id, entity_type, entity_id, anomaly_type,
               zscore_threshold, iqr_multiplier, volume_spike_threshold,
               notification_channels, notify_on_severity, enabled
        FROM anomaly_rules
        WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(rule_id)
    .bind(tenant_id.0)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "AnomalyRule".to_string(),
        id: rule_id.to_string(),
    })?;

    let rule = serde_json::json!({
        "id": row.try_get::<Uuid, _>("id").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "entity_type": row.try_get::<Option<String>, _>("entity_type").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "entity_id": row.try_get::<Option<String>, _>("entity_id").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "anomaly_type": row.try_get::<String, _>("anomaly_type").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "zscore_threshold": row.try_get::<Option<f64>, _>("zscore_threshold").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "iqr_multiplier": row.try_get::<Option<f64>, _>("iqr_multiplier").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "volume_spike_threshold": row.try_get::<Option<f64>, _>("volume_spike_threshold").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "notification_channels": row.try_get::<Option<serde_json::Value>, _>("notification_channels").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "notify_on_severity": row.try_get::<Option<serde_json::Value>, _>("notify_on_severity").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
        "enabled": row.try_get::<bool, _>("enabled").map_err(|e| billforge_core::Error::Database(e.to_string()))?,
    });

    Ok(Json(rule))
}

/// Update anomaly rule
#[utoipa::path(put, path = "/api/v1/analytics/predictive/anomaly-rules/{id}", tag = "Predictive Analytics", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Rule updated")))]
async fn update_anomaly_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Path(rule_id): Path<Uuid>,
    Json(payload): Json<ConfigureAnomalyRuleRequest>,
) -> ApiResult<Json<()>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let notification_channels = payload.notification_channels
        .map(|c| serde_json::to_value(c).map_err(|e| billforge_core::Error::Internal(e.to_string())))
        .transpose()?;
    let notify_on_severity = payload.notify_on_severity
        .map(|s| serde_json::to_value(s).map_err(|e| billforge_core::Error::Internal(e.to_string())))
        .transpose()?;

    let result = sqlx::query(
        r#"
        UPDATE anomaly_rules
        SET
            zscore_threshold = COALESCE($1, zscore_threshold),
            iqr_multiplier = COALESCE($2, iqr_multiplier),
            volume_spike_threshold = COALESCE($3, volume_spike_threshold),
            notification_channels = COALESCE($4, notification_channels),
            notify_on_severity = COALESCE($5, notify_on_severity),
            enabled = COALESCE($6, enabled),
            updated_at = NOW()
        WHERE id = $7 AND tenant_id = $8
        "#,
    )
    .bind(payload.zscore_threshold)
    .bind(payload.iqr_multiplier)
    .bind(payload.volume_spike_threshold)
    .bind(&notification_channels)
    .bind(&notify_on_severity)
    .bind(payload.enabled)
    .bind(rule_id)
    .bind(tenant_id.0)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "AnomalyRule".to_string(),
            id: rule_id.to_string(),
        }.into());
    }

    Ok(Json(()))
}

// Helper functions

async fn fetch_historical_spend(
    db: &billforge_db::DatabaseManager,
    tenant_id: Uuid,
    entity_id: &str,
    entity_type: EntityType,
) -> Result<TimeSeries, billforge_core::Error> {
    // Fetch last 90 days of spend data
    let tenant_id_str = tenant_id.to_string();
    let pool = db.tenant(&tenant_id_str.parse().map_err(|e| billforge_core::Error::Internal(format!("Invalid tenant ID: {}", e)))?).await?;

    let rows = sqlx::query(
        r#"
        SELECT
            DATE(created_at) as date,
            SUM(total_amount_cents) as amount
        FROM invoices
        WHERE tenant_id = $1
            AND vendor_id = $2
            AND created_at > NOW() - INTERVAL '90 days'
            AND status != 'rejected'
        GROUP BY DATE(created_at)
        ORDER BY date
        "#,
    )
    .bind(tenant_id)
    .bind(entity_id)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let points: Vec<TimeSeriesPoint> = rows
        .into_iter()
        .filter_map(|row| {
            let date: chrono::NaiveDate = row.try_get("date").ok()?;
            let amount_str: String = row.try_get("amount").ok()?;
            let amount: f64 = amount_str.parse().ok()?;

            // Convert NaiveDate to DateTime<Utc>
            let timestamp: DateTime<Utc> = Utc.from_utc_date(&date).and_hms_opt(0, 0, 0)?;

            Some(TimeSeriesPoint {
                timestamp,
                value: amount,
            })
        })
        .collect();

    Ok(TimeSeries {
        entity_id: entity_id.to_string(),
        entity_type,
        metric_name: "spend".to_string(),
        points,
    })
}

async fn store_forecast(db: &billforge_db::DatabaseManager, tenant_id: Uuid, forecast: &Forecast) -> Result<(), billforge_core::Error> {
    let valid_until = forecast.generated_at + chrono::Duration::days(forecast.horizon.days() as i64);
    let tenant_id_str = tenant_id.to_string();
    let pool = db.tenant(&tenant_id_str.parse().map_err(|e| billforge_core::Error::Internal(format!("Invalid tenant ID: {}", e)))?).await?;

    let entity_type_str = serde_json::to_string(&forecast.entity_type)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;
    let horizon_str = serde_json::to_string(&forecast.horizon)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO spend_forecasts (
            tenant_id, entity_id, entity_type, metric_name, horizon,
            predicted_value, confidence_lower, confidence_upper, confidence_level,
            model_version, seasonality_detected, valid_until
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(tenant_id)
    .bind(&forecast.entity_id)
    .bind(&entity_type_str)
    .bind(&forecast.metric_name)
    .bind(&horizon_str)
    .bind(forecast.predicted_value)
    .bind(forecast.confidence_lower)
    .bind(forecast.confidence_upper)
    .bind(forecast.confidence_level)
    .bind(&forecast.model_version)
    .bind(forecast.seasonality_detected)
    .bind(valid_until)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(())
}

async fn fetch_recent_invoices(db: &billforge_db::DatabaseManager, tenant_id: Uuid) -> Result<Vec<InvoiceRecord>, billforge_core::Error> {
    let tenant_id_str = tenant_id.to_string();
    let pool = db.tenant(&tenant_id_str.parse().map_err(|e| billforge_core::Error::Internal(format!("Invalid tenant ID: {}", e)))?).await?;

    let rows = sqlx::query(
        r#"
        SELECT
            i.id::text as invoice_id,
            v.name as vendor_name,
            i.total_amount_cents,
            i.invoice_date
        FROM invoices i
        JOIN vendors v ON i.vendor_id = v.id
        WHERE i.tenant_id = $1
            AND i.created_at > NOW() - INTERVAL '90 days'
        ORDER BY i.created_at DESC
        LIMIT 1000
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let invoices = rows
        .into_iter()
        .filter_map(|row| {
            let amount_str: String = row.try_get("total_amount_cents").ok()?;
            let amount: f64 = amount_str.parse().ok()?;

            Some(InvoiceRecord {
                invoice_id: row.try_get("invoice_id").ok()?,
                vendor_name: row.try_get("vendor_name").ok()?,
                amount,
                invoice_date: row.try_get("invoice_date").ok()?,
            })
        })
        .collect();

    Ok(invoices)
}

async fn store_anomaly(db: &billforge_db::DatabaseManager, anomaly: &Anomaly) -> Result<(), billforge_core::Error> {
    let tenant_id_str = anomaly.tenant_id.to_string();
    let pool = db.tenant(&tenant_id_str.parse().map_err(|e| billforge_core::Error::Internal(format!("Invalid tenant ID: {}", e)))?).await?;

    let anomaly_type_str = serde_json::to_string(&anomaly.anomaly_type)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;
    let entity_type_str = serde_json::to_string(&anomaly.entity_type)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;
    let severity_str = serde_json::to_string(&anomaly.severity)
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO invoice_anomalies (
            id, tenant_id, anomaly_type, entity_id, entity_type, severity,
            detected_value, expected_range_min, expected_range_max, deviation_score,
            metadata, detected_at, acknowledged
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(anomaly.id)
    .bind(anomaly.tenant_id)
    .bind(&anomaly_type_str)
    .bind(&anomaly.entity_id)
    .bind(&entity_type_str)
    .bind(&severity_str)
    .bind(anomaly.detected_value)
    .bind(anomaly.expected_range.0)
    .bind(anomaly.expected_range.1)
    .bind(anomaly.deviation_score)
    .bind(&anomaly.metadata)
    .bind(anomaly.detected_at)
    .bind(anomaly.acknowledged)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    Ok(())
}
