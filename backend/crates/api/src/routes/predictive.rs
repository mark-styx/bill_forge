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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use billforge_analytics::{
    forecasting::ArimaForecaster,
    anomaly_detection::{DuplicateDetector, InvoiceRecord, StatisticalAnomalyDetector},
    predictive_models::*,
};
use billforge_core::error::Error;
use billforge_db::Database;

use crate::auth::AuthenticatedUser;
use crate::routes::ApiResponse;
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
#[utoipa::path(
    get,
    path = "/api/v1/analytics/predictive/forecasts",
    responses(
        (status = 200, description = "Forecasts retrieved", body = Vec<Forecast>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
async fn get_forecasts(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<ApiResponse<Vec<Forecast>>>, Error> {
    let tenant_id = user.tenant_id();
    let pool = state.db.tenant(&tenant_id).await?;

    // Parse horizon
    let horizon = match query.horizon.as_str() {
        "30" | "days_30" => ForecastHorizon::Days30,
        "60" | "days_60" => ForecastHorizon::Days60,
        "90" | "days_90" => ForecastHorizon::Days90,
        _ => return Err(Error::Validation("Invalid horizon. Use 30, 60, or 90".to_string())),
    };

    // Fetch forecasts from database
    let forecasts = sqlx::query_as!(
        Forecast,
        r#"
        SELECT
            entity_id,
            entity_type as "entity_type: EntityType",
            metric_name,
            horizon as "horizon: ForecastHorizon",
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
        tenant_id,
        query.entity_id,
        query.entity_type,
        serde_json::to_string(&horizon).map_err(|e| Error::Internal(e.to_string()))?,
    )
    .fetch_optional(&*pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    let forecasts = forecasts.map(|f| vec![f]).unwrap_or_default();

    Ok(Json(ApiResponse::success(forecasts)))
}

/// Generate a new forecast
#[utoipa::path(
    post,
    path = "/api/v1/analytics/predictive/forecasts/generate",
    responses(
        (status = 200, description = "Forecast generated", body = Forecast),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
async fn generate_forecast(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<ForecastQuery>,
) -> Result<Json<ApiResponse<Forecast>>, Error> {
    let tenant_id = user.tenant_id();

    // Fetch historical data for the entity
    let entity_type = match payload.entity_type.as_str() {
        "vendor" => EntityType::Vendor,
        "department" => EntityType::Department,
        "gl_code" => EntityType::GlCode,
        "tenant" => EntityType::Tenant,
        "approver" => EntityType::Approver,
        _ => return Err(Error::Validation("Invalid entity_type".to_string())),
    };

    let horizon = match payload.horizon.as_str() {
        "30" | "days_30" => ForecastHorizon::Days30,
        "60" | "days_60" => ForecastHorizon::Days60,
        "90" | "days_90" => ForecastHorizon::Days90,
        _ => return Err(Error::Validation("Invalid horizon".to_string())),
    };

    // Fetch historical spend data
    let historical_data = fetch_historical_spend(&state.db, tenant_id, &payload.entity_id, entity_type)
        .await?;

    if historical_data.points.len() < 30 {
        return Err(Error::Validation(
            "Insufficient historical data. Need at least 30 days of data.".to_string(),
        ));
    }

    // Generate forecast using ARIMA model
    let mut forecaster = ArimaForecaster::new();
    forecaster
        .fit(&historical_data)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;

    let forecast = forecaster
        .forecast(horizon)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;

    // Store forecast in database
    store_forecast(&state.db, tenant_id, &forecast).await?;

    Ok(Json(ApiResponse::success(forecast)))
}

/// Get forecast by ID
async fn get_forecast_by_id(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(forecast_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Forecast>>, Error> {
    let tenant_id = user.tenant_id();

    let forecast = sqlx::query_as!(
        Forecast,
        r#"
        SELECT
            entity_id,
            entity_type as "entity_type: EntityType",
            metric_name,
            horizon as "horizon: ForecastHorizon",
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
        forecast_id,
        tenant_id,
    )
    .fetch_optional(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?
    .ok_or_else(|| Error::NotFound {
        resource_type: "Forecast".to_string(),
        id: forecast_id.to_string(),
    })?;

    Ok(Json(ApiResponse::success(forecast)))
}

/// Get anomalies for tenant
#[utoipa::path(
    get,
    path = "/api/v1/analytics/predictive/anomalies",
    responses(
        (status = 200, description = "Anomalies retrieved", body = Vec<Anomaly>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
async fn get_anomalies(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ApiResponse<Vec<Anomaly>>>, Error> {
    let tenant_id = user.tenant_id();

    let anomalies = sqlx::query_as!(
        Anomaly,
        r#"
        SELECT
            id,
            tenant_id,
            anomaly_type as "anomaly_type: AnomalyType",
            entity_id,
            entity_type as "entity_type: EntityType",
            severity as "severity: AnomalySeverity",
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
        tenant_id,
    )
    .fetch_all(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    // Transform to expected format
    let anomalies = anomalies
        .into_iter()
        .map(|a| Anomaly {
            expected_range: (a.expected_range_min, a.expected_range_max),
            ..a
        })
        .collect();

    Ok(Json(ApiResponse::success(anomalies)))
}

/// Acknowledge an anomaly
async fn acknowledge_anomaly(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(anomaly_id): Path<Uuid>,
    Json(payload): Json<AcknowledgeAnomalyRequest>,
) -> Result<Json<ApiResponse<()>>, Error> {
    let tenant_id = user.tenant_id();
    let user_id = user.user_id();

    let result = sqlx::query!(
        r#"
        UPDATE invoice_anomalies
        SET
            acknowledged = TRUE,
            acknowledged_at = NOW(),
            acknowledged_by = $1,
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        "#,
        user_id,
        anomaly_id,
        tenant_id,
    )
    .execute(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound {
            resource_type: "Anomaly".to_string(),
            id: anomaly_id.to_string(),
        });
    }

    Ok(Json(ApiResponse::success(())))
}

/// Detect anomalies (manual trigger)
async fn detect_anomalies(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ApiResponse<Vec<Anomaly>>>, Error> {
    let tenant_id = user.tenant_id();

    // Fetch recent invoices for anomaly detection
    let invoices = fetch_recent_invoices(&state.db, tenant_id).await?;

    // Run duplicate detection
    let duplicate_detector = DuplicateDetector::new(tenant_id);
    let duplicate_anomalies = duplicate_detector
        .detect_duplicates(&invoices)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;

    // Store anomalies in database
    for anomaly in &duplicate_anomalies {
        store_anomaly(&state.db, anomaly).await?;
    }

    Ok(Json(ApiResponse::success(duplicate_anomalies)))
}

/// Get budget alerts
async fn get_budget_alerts(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, Error> {
    let tenant_id = user.tenant_id();

    let alerts = sqlx::query!(
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
        tenant_id,
    )
    .fetch_all(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(Json(ApiResponse::success(serde_json::to_value(alerts).map_err(|e| Error::Internal(e.to_string()))?)))
}

/// Dismiss a budget alert
async fn dismiss_alert(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(alert_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, Error> {
    let tenant_id = user.tenant_id();
    let user_id = user.user_id();

    let result = sqlx::query!(
        r#"
        UPDATE budget_alerts
        SET dismissed = TRUE, dismissed_at = NOW(), dismissed_by = $1, updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        "#,
        user_id,
        alert_id,
        tenant_id,
    )
    .execute(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound {
            resource_type: "Alert".to_string(),
            id: alert_id.to_string(),
        });
    }

    Ok(Json(ApiResponse::success(())))
}

/// Get anomaly rules
async fn get_anomaly_rules(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, Error> {
    let tenant_id = user.tenant_id();

    let rules = sqlx::query!(
        r#"
        SELECT id, entity_type, entity_id, anomaly_type,
               zscore_threshold, iqr_multiplier, volume_spike_threshold,
               notification_channels, notify_on_severity, enabled
        FROM anomaly_rules
        WHERE tenant_id = $1
        ORDER BY created_at DESC
        "#,
        tenant_id,
    )
    .fetch_all(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(Json(ApiResponse::success(serde_json::to_value(rules).map_err(|e| Error::Internal(e.to_string()))?)))
}

/// Configure anomaly rule
async fn configure_anomaly_rule(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<ConfigureAnomalyRuleRequest>,
) -> Result<Json<ApiResponse<Uuid>>, Error> {
    let tenant_id = user.tenant_id();
    let user_id = user.user_id();
    let rule_id = Uuid::new_v4();

    sqlx::query!(
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
        rule_id,
        tenant_id,
        payload.entity_type,
        payload.entity_id,
        payload.anomaly_type,
        payload.zscore_threshold,
        payload.iqr_multiplier,
        payload.volume_spike_threshold,
        &payload.notification_channels.unwrap_or_default(),
        &payload.notify_on_severity.unwrap_or_default(),
        payload.enabled.unwrap_or(true),
        user_id,
    )
    .execute(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(Json(ApiResponse::success(rule_id)))
}

/// Get anomaly rule by ID
async fn get_anomaly_rule(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(rule_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, Error> {
    let tenant_id = user.tenant_id();

    let rule = sqlx::query!(
        r#"
        SELECT id, entity_type, entity_id, anomaly_type,
               zscore_threshold, iqr_multiplier, volume_spike_threshold,
               notification_channels, notify_on_severity, enabled
        FROM anomaly_rules
        WHERE id = $1 AND tenant_id = $2
        "#,
        rule_id,
        tenant_id,
    )
    .fetch_optional(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?
    .ok_or_else(|| Error::NotFound {
        resource_type: "AnomalyRule".to_string(),
        id: rule_id.to_string(),
    })?;

    Ok(Json(ApiResponse::success(serde_json::to_value(rule).map_err(|e| Error::Internal(e.to_string()))?)))
}

/// Update anomaly rule
async fn update_anomaly_rule(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(rule_id): Path<Uuid>,
    Json(payload): Json<ConfigureAnomalyRuleRequest>,
) -> Result<Json<ApiResponse<()>>, Error> {
    let tenant_id = user.tenant_id();

    let result = sqlx::query!(
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
        payload.zscore_threshold,
        payload.iqr_multiplier,
        payload.volume_spike_threshold,
        &payload.notification_channels.unwrap_or_default(),
        &payload.notify_on_severity.unwrap_or_default(),
        payload.enabled,
        rule_id,
        tenant_id,
    )
    .execute(&*state.db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound {
            resource_type: "AnomalyRule".to_string(),
            id: rule_id.to_string(),
        });
    }

    Ok(Json(ApiResponse::success(())))
}

// Helper functions

async fn fetch_historical_spend(
    db: &Database,
    tenant_id: Uuid,
    entity_id: &str,
    entity_type: EntityType,
) -> Result<TimeSeries, Error> {
    // Fetch last 90 days of spend data
    let rows = sqlx::query!(
        r#"
        SELECT
            DATE(created_at) as date,
            SUM(total_amount) as amount
        FROM invoices
        WHERE tenant_id = $1
            AND vendor_id = $2
            AND created_at > NOW() - INTERVAL '90 days'
            AND status != 'rejected'
        GROUP BY DATE(created_at)
        ORDER BY date
        "#,
        tenant_id,
        entity_id,
    )
    .fetch_all(&*db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    let points: Vec<TimeSeriesPoint> = rows
        .into_iter()
        .filter_map(|row| {
            Some(TimeSeriesPoint {
                timestamp: row.date?.and_utc(),
                value: row.amount?.try_into().ok()?,
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

async fn store_forecast(db: &Database, tenant_id: Uuid, forecast: &Forecast) -> Result<(), Error> {
    let valid_until = forecast.generated_at + chrono::Duration::days(forecast.horizon.days() as i64);

    sqlx::query!(
        r#"
        INSERT INTO spend_forecasts (
            tenant_id, entity_id, entity_type, metric_name, horizon,
            predicted_value, confidence_lower, confidence_upper, confidence_level,
            model_version, seasonality_detected, valid_until
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        tenant_id,
        forecast.entity_id,
        serde_json::to_string(&forecast.entity_type).map_err(|e| Error::Internal(e.to_string()))?,
        forecast.metric_name,
        serde_json::to_string(&forecast.horizon).map_err(|e| Error::Internal(e.to_string()))?,
        forecast.predicted_value,
        forecast.confidence_lower,
        forecast.confidence_upper,
        forecast.confidence_level,
        forecast.model_version,
        forecast.seasonality_detected,
        valid_until,
    )
    .execute(&*db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(())
}

async fn fetch_recent_invoices(db: &Database, tenant_id: Uuid) -> Result<Vec<InvoiceRecord>, Error> {
    let rows = sqlx::query!(
        r#"
        SELECT
            i.id::text as invoice_id,
            v.name as vendor_name,
            i.total_amount,
            i.invoice_date
        FROM invoices i
        JOIN vendors v ON i.vendor_id = v.id
        WHERE i.tenant_id = $1
            AND i.created_at > NOW() - INTERVAL '90 days'
        ORDER BY i.created_at DESC
        LIMIT 1000
        "#,
        tenant_id,
    )
    .fetch_all(&*db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    let invoices = rows
        .into_iter()
        .filter_map(|row| {
            Some(InvoiceRecord {
                invoice_id: row.invoice_id?,
                vendor_name: row.vendor_name?,
                amount: row.total_amount?.try_into().ok()?,
                invoice_date: row.invoice_date?,
            })
        })
        .collect();

    Ok(invoices)
}

async fn store_anomaly(db: &Database, anomaly: &Anomaly) -> Result<(), Error> {
    sqlx::query!(
        r#"
        INSERT INTO invoice_anomalies (
            id, tenant_id, anomaly_type, entity_id, entity_type, severity,
            detected_value, expected_range_min, expected_range_max, deviation_score,
            metadata, detected_at, acknowledged
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
        anomaly.id,
        anomaly.tenant_id,
        serde_json::to_string(&anomaly.anomaly_type).map_err(|e| Error::Internal(e.to_string()))?,
        anomaly.entity_id,
        serde_json::to_string(&anomaly.entity_type).map_err(|e| Error::Internal(e.to_string()))?,
        serde_json::to_string(&anomaly.severity).map_err(|e| Error::Internal(e.to_string()))?,
        anomaly.detected_value,
        anomaly.expected_range.0,
        anomaly.expected_range.1,
        anomaly.deviation_score,
        anomaly.metadata,
        anomaly.detected_at,
        anomaly.acknowledged,
    )
    .execute(&*db.pool)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(())
}
