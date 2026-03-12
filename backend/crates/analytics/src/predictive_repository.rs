//! Predictive Analytics Repository
//!
//! Database operations for forecasting, anomaly detection, and budget alerts.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::predictive_models::*;

/// Helper: Convert NaiveDate to DateTime<Utc> at midnight
fn naive_date_to_utc(date: NaiveDate) -> DateTime<Utc> {
    DateTime::from_utc(date.and_time(NaiveTime::from_hms(0, 0, 0)), Utc)
}

pub struct PredictiveRepository {
    pool: PgPool,
}

impl PredictiveRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Save a forecast to the database
    pub async fn save_forecast(&self, tenant_id: Uuid, forecast: &Forecast) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let valid_until = forecast.generated_at + chrono::Duration::days(forecast.horizon.days() as i64);

        sqlx::query(
            r#"
            INSERT INTO spend_forecasts (
                id, tenant_id, entity_id, entity_type, metric_name, horizon,
                predicted_value, confidence_lower, confidence_upper, confidence_level,
                model_version, seasonality_detected, valid_until
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (tenant_id, entity_id, entity_type, metric_name, horizon, generated_at)
            DO UPDATE SET
                predicted_value = EXCLUDED.predicted_value,
                confidence_lower = EXCLUDED.confidence_lower,
                confidence_upper = EXCLUDED.confidence_upper,
                updated_at = NOW()
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&forecast.entity_id)
        .bind(&serde_json::to_string(&forecast.entity_type)?)
        .bind(&forecast.metric_name)
        .bind(&serde_json::to_string(&forecast.horizon)?)
        .bind(forecast.predicted_value)
        .bind(forecast.confidence_lower)
        .bind(forecast.confidence_upper)
        .bind(forecast.confidence_level)
        .bind(&forecast.model_version)
        .bind(forecast.seasonality_detected)
        .bind(valid_until)
        .execute(&self.pool)
        .await
        .context("Failed to save forecast")?;

        Ok(id)
    }

    /// Get forecasts for an entity type and horizon
    pub async fn get_forecasts(
        &self,
        tenant_id: Uuid,
        entity_type: EntityType,
        horizon: ForecastHorizon,
    ) -> Result<Vec<Forecast>> {
        let rows = sqlx::query(
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
                AND entity_type = $2
                AND horizon = $3
                AND valid_until > NOW()
            ORDER BY generated_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(&serde_json::to_string(&entity_type)?)
        .bind(&serde_json::to_string(&horizon)?)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get forecasts")?;

        let forecasts = rows
            .iter()
            .filter_map(|row| {
                let entity_id: String = row.try_get("entity_id").ok()?;
                let entity_type_str: String = row.try_get("entity_type").ok()?;
                let metric_name: String = row.try_get("metric_name").ok()?;
                let horizon_str: String = row.try_get("horizon").ok()?;
                let predicted_value: f64 = row.try_get("predicted_value").ok()?;
                let confidence_lower: f64 = row.try_get("confidence_lower").ok()?;
                let confidence_upper: f64 = row.try_get("confidence_upper").ok()?;
                let confidence_level: f64 = row.try_get("confidence_level").ok()?;
                let generated_at: DateTime<Utc> = row.try_get("generated_at").ok()?;
                let model_version: String = row.try_get("model_version").ok()?;
                let seasonality_detected: bool = row.try_get("seasonality_detected").ok()?;

                Some(Forecast {
                    entity_id,
                    entity_type: serde_json::from_str(&entity_type_str).ok()?,
                    metric_name,
                    horizon: serde_json::from_str(&horizon_str).ok()?,
                    predicted_value,
                    confidence_lower,
                    confidence_upper,
                    confidence_level,
                    generated_at,
                    model_version,
                    seasonality_detected,
                })
            })
            .collect();

        Ok(forecasts)
    }

    /// Get the latest forecast for a specific entity and metric
    pub async fn get_latest_forecast(
        &self,
        tenant_id: Uuid,
        entity_id: &str,
        metric_name: &str,
    ) -> Result<Option<Forecast>> {
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
                AND metric_name = $3
                AND valid_until > NOW()
            ORDER BY generated_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(entity_id)
        .bind(metric_name)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get latest forecast")?;

        Ok(row.and_then(|r| {
            let entity_id: String = r.try_get("entity_id").ok()?;
            let entity_type_str: String = r.try_get("entity_type").ok()?;
            let metric_name: String = r.try_get("metric_name").ok()?;
            let horizon_str: String = r.try_get("horizon").ok()?;
            let predicted_value: f64 = r.try_get("predicted_value").ok()?;
            let confidence_lower: f64 = r.try_get("confidence_lower").ok()?;
            let confidence_upper: f64 = r.try_get("confidence_upper").ok()?;
            let confidence_level: f64 = r.try_get("confidence_level").ok()?;
            let generated_at: DateTime<Utc> = r.try_get("generated_at").ok()?;
            let model_version: String = r.try_get("model_version").ok()?;
            let seasonality_detected: bool = r.try_get("seasonality_detected").ok()?;

            Some(Forecast {
                entity_id,
                entity_type: serde_json::from_str(&entity_type_str).ok()?,
                metric_name,
                horizon: serde_json::from_str(&horizon_str).ok()?,
                predicted_value,
                confidence_lower,
                confidence_upper,
                confidence_level,
                generated_at,
                model_version,
                seasonality_detected,
            })
        }))
    }

    /// Save an anomaly to the database
    pub async fn save_anomaly(&self, anomaly: &Anomaly) -> Result<Uuid> {
        sqlx::query(
            r#"
            INSERT INTO invoice_anomalies (
                id, tenant_id, anomaly_type, entity_id, entity_type, severity,
                detected_value, expected_range_min, expected_range_max, deviation_score,
                metadata, detected_at, acknowledged
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(anomaly.id)
        .bind(anomaly.tenant_id)
        .bind(&serde_json::to_string(&anomaly.anomaly_type)?)
        .bind(&anomaly.entity_id)
        .bind(&serde_json::to_string(&anomaly.entity_type)?)
        .bind(&serde_json::to_string(&anomaly.severity)?)
        .bind(anomaly.detected_value)
        .bind(anomaly.expected_range.0)
        .bind(anomaly.expected_range.1)
        .bind(anomaly.deviation_score)
        .bind(anomaly.metadata.clone())
        .bind(anomaly.detected_at)
        .bind(anomaly.acknowledged)
        .execute(&self.pool)
        .await
        .context("Failed to save anomaly")?;

        Ok(anomaly.id)
    }

    /// Get anomalies with optional filters
    pub async fn get_anomalies(
        &self,
        tenant_id: Uuid,
        anomaly_type: Option<AnomalyType>,
        severity: Option<AnomalySeverity>,
        unacknowledged_only: bool,
    ) -> Result<Vec<Anomaly>> {
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
                AND ($2::text IS NULL OR anomaly_type = $2)
                AND ($3::text IS NULL OR severity = $3)
                AND ($4 = FALSE OR acknowledged = FALSE)
            ORDER BY detected_at DESC
            LIMIT 100
            "#,
        )
        .bind(tenant_id)
        .bind(anomaly_type.map(|t| serde_json::to_string(&t).unwrap()))
        .bind(severity.map(|s| serde_json::to_string(&s).unwrap()))
        .bind(unacknowledged_only)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get anomalies")?;

        let anomalies = rows
            .iter()
            .filter_map(|row| {
                Some(Anomaly {
                    id: row.try_get("id").ok()?,
                    tenant_id: row.try_get("tenant_id").ok()?,
                    anomaly_type: serde_json::from_str(&row.try_get::<String, _>("anomaly_type").ok()?).ok()?,
                    entity_id: row.try_get("entity_id").ok()?,
                    entity_type: serde_json::from_str(&row.try_get::<String, _>("entity_type").ok()?).ok()?,
                    severity: serde_json::from_str(&row.try_get::<String, _>("severity").ok()?).ok()?,
                    detected_value: row.try_get("detected_value").ok()?,
                    expected_range: (row.try_get("expected_range_min").ok()?, row.try_get("expected_range_max").ok()?),
                    deviation_score: row.try_get("deviation_score").ok()?,
                    detected_at: row.try_get("detected_at").ok()?,
                    metadata: row.try_get("metadata").ok()?,
                    acknowledged: row.try_get("acknowledged").ok()?,
                    acknowledged_at: row.try_get("acknowledged_at").ok()?,
                    acknowledged_by: row.try_get("acknowledged_by").ok()?,
                })
            })
            .collect();

        Ok(anomalies)
    }

    /// Acknowledge an anomaly
    pub async fn acknowledge_anomaly(
        &self,
        anomaly_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE invoice_anomalies
            SET
                acknowledged = TRUE,
                acknowledged_at = NOW(),
                acknowledged_by = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(user_id)
        .bind(anomaly_id)
        .execute(&self.pool)
        .await
        .context("Failed to acknowledge anomaly")?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Anomaly not found"));
        }

        Ok(())
    }

    /// Save a budget alert
    pub async fn save_budget_alert(
        &self,
        tenant_id: Uuid,
        alert_type: &str,
        severity: &str,
        entity_id: Option<&str>,
        entity_type: Option<&str>,
        title: &str,
        message: &str,
        threshold_value: Option<f64>,
        current_value: Option<f64>,
        threshold_percentage: Option<f64>,
        recommended_action: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO budget_alerts (
                id, tenant_id, alert_type, severity, entity_id, entity_type,
                title, message, threshold_value, current_value, threshold_percentage,
                recommended_action
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(alert_type)
        .bind(severity)
        .bind(entity_id)
        .bind(entity_type)
        .bind(title)
        .bind(message)
        .bind(threshold_value)
        .bind(current_value)
        .bind(threshold_percentage)
        .bind(recommended_action)
        .execute(&self.pool)
        .await
        .context("Failed to save budget alert")?;

        Ok(id)
    }

    /// Get active (non-dismissed) alerts for a tenant
    pub async fn get_active_alerts(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>> {
        let alerts = sqlx::query(
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
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get active alerts")?;

        Ok(alerts
            .iter()
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
                    "recommended_action": row.try_get::<String, _>("recommended_action").ok(),
                    "triggered_at": row.try_get::<DateTime<Utc>, _>("triggered_at").ok(),
                    "dismissed": row.try_get::<bool, _>("dismissed").ok(),
                })
            })
            .collect())
    }

    /// Dismiss an alert
    pub async fn dismiss_alert(&self, alert_id: Uuid, user_id: Uuid) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE budget_alerts
            SET dismissed = TRUE, dismissed_at = NOW(), dismissed_by = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(user_id)
        .bind(alert_id)
        .execute(&self.pool)
        .await
        .context("Failed to dismiss alert")?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Alert not found"));
        }

        Ok(())
    }

    /// Log forecast accuracy for model evaluation
    pub async fn log_forecast_accuracy(
        &self,
        tenant_id: Uuid,
        forecast_id: Uuid,
        entity_id: &str,
        entity_type: &str,
        metric_name: &str,
        horizon: &str,
        predicted_value: f64,
        actual_value: f64,
        forecast_date: DateTime<Utc>,
        actual_date: DateTime<Utc>,
    ) -> Result<()> {
        let absolute_error = (predicted_value - actual_value).abs();
        let percentage_error = if actual_value != 0.0 {
            Some((absolute_error / actual_value.abs()) * 100.0)
        } else {
            None
        };
        let squared_error = (predicted_value - actual_value).powi(2);

        sqlx::query(
            r#"
            INSERT INTO forecast_accuracy_log (
                tenant_id, forecast_id, entity_id, entity_type, metric_name, horizon,
                predicted_value, actual_value, absolute_error, percentage_error, squared_error,
                forecast_date, actual_date
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(tenant_id)
        .bind(forecast_id)
        .bind(entity_id)
        .bind(entity_type)
        .bind(metric_name)
        .bind(horizon)
        .bind(predicted_value)
        .bind(actual_value)
        .bind(absolute_error)
        .bind(percentage_error)
        .bind(squared_error)
        .bind(forecast_date)
        .bind(actual_date)
        .execute(&self.pool)
        .await
        .context("Failed to log forecast accuracy")?;

        Ok(())
    }

    /// Get forecast accuracy summary (MAPE, MAE, RMSE) for a tenant
    pub async fn get_model_performance(&self, tenant_id: Uuid) -> Result<ForecastAccuracySummary> {
        let row = sqlx::query(
            r#"
            SELECT
                AVG(percentage_error) as mape,
                AVG(absolute_error) as mae,
                SQRT(AVG(squared_error)) as rmse,
                COUNT(*) as total_forecasts
            FROM forecast_accuracy_log
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .context("Failed to get model performance")?;

        Ok(ForecastAccuracySummary {
            mape: row.try_get("mape").ok().unwrap_or(0.0),
            mae: row.try_get("mae").ok().unwrap_or(0.0),
            rmse: row.try_get("rmse").ok().unwrap_or(0.0),
            total_forecasts: row.try_get("total_forecasts").ok().unwrap_or(0) as i32,
        })
    }

    /// Fetch time-series data for vendor spend
    pub async fn get_vendor_spend_timeseries(
        &self,
        tenant_id: Uuid,
        vendor_id: &str,
        days: i32,
    ) -> Result<TimeSeries> {
        let rows = sqlx::query(
            r#"
            SELECT
                DATE(created_at) as date,
                SUM(total_amount_cents) as amount
            FROM invoices
            WHERE tenant_id = $1
                AND vendor_id = $2
                AND created_at > NOW() - INTERVAL '1 day' * $3
                AND status != 'rejected'
            GROUP BY DATE(created_at)
            ORDER BY date
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(days)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get vendor spend time series")?;

        let points: Vec<TimeSeriesPoint> = rows
            .iter()
            .filter_map(|row| {
                Some(TimeSeriesPoint {
                    timestamp: naive_date_to_utc(row.try_get::<chrono::NaiveDate, _>("date").ok()?),
                    value: row.try_get("amount").ok()?,
                })
            })
            .collect();

        Ok(TimeSeries {
            entity_id: vendor_id.to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "spend".to_string(),
            points,
        })
    }

    /// Fetch time-series data for department spend
    pub async fn get_department_spend_timeseries(
        &self,
        tenant_id: Uuid,
        department: &str,
        days: i32,
    ) -> Result<TimeSeries> {
        let rows = sqlx::query(
            r#"
            SELECT
                DATE(created_at) as date,
                SUM(total_amount_cents) as amount
            FROM invoices
            WHERE tenant_id = $1
                AND department = $2
                AND created_at > NOW() - INTERVAL '1 day' * $3
                AND status != 'rejected'
            GROUP BY DATE(created_at)
            ORDER BY date
            "#,
        )
        .bind(tenant_id)
        .bind(department)
        .bind(days)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get department spend time series")?;

        let points: Vec<TimeSeriesPoint> = rows
            .iter()
            .filter_map(|row| {
                Some(TimeSeriesPoint {
                    timestamp: naive_date_to_utc(row.try_get::<chrono::NaiveDate, _>("date").ok()?),
                    value: row.try_get("amount").ok()?,
                })
            })
            .collect();

        Ok(TimeSeries {
            entity_id: department.to_string(),
            entity_type: EntityType::Department,
            metric_name: "spend".to_string(),
            points,
        })
    }
}

/// Forecast accuracy summary for model performance tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForecastAccuracySummary {
    pub mape: f64,           // Mean Absolute Percentage Error
    pub mae: f64,            // Mean Absolute Error
    pub rmse: f64,           // Root Mean Squared Error
    pub total_forecasts: i32,
}
