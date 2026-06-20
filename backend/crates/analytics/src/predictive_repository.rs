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
        let valid_until =
            forecast.generated_at + chrono::Duration::days(forecast.horizon.days() as i64);

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
                    anomaly_type: serde_json::from_str(
                        &row.try_get::<String, _>("anomaly_type").ok()?,
                    )
                    .ok()?,
                    entity_id: row.try_get("entity_id").ok()?,
                    entity_type: serde_json::from_str(
                        &row.try_get::<String, _>("entity_type").ok()?,
                    )
                    .ok()?,
                    severity: serde_json::from_str(&row.try_get::<String, _>("severity").ok()?)
                        .ok()?,
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

        Ok(anomalies)
    }

    /// Acknowledge an anomaly. The `false_positive` flag is the actual learning
    /// signal: when set to TRUE the threshold-recalibration loop counts this
    /// row against the relevant detector's false-positive rate. When FALSE the
    /// acknowledgement is treated as a "true positive, I saw it" and is not
    /// used to nudge thresholds.
    pub async fn acknowledge_anomaly(
        &self,
        anomaly_id: Uuid,
        user_id: Uuid,
        false_positive: bool,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE invoice_anomalies
            SET
                acknowledged = TRUE,
                acknowledged_at = NOW(),
                acknowledged_by = $1,
                false_positive = $3,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(user_id)
        .bind(anomaly_id)
        .bind(false_positive)
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

    /// Aggregate forecast accuracy for the last `days` days.
    ///
    /// Returns (mape, signed_bias_pct, sample_count) where:
    /// - `mape` is the mean of `percentage_error` over rows with a non-null
    ///   percentage (i.e. excluding zero-actual rows).
    /// - `signed_bias_pct` is the mean signed deviation
    ///   `((predicted - actual) / actual) * 100`. Positive means forecasts
    ///   overshoot actuals; negative means they undershoot.
    ///
    /// Used by the forecast-tuning worker to drive per-tenant parameter
    /// overrides from realized outcomes.
    pub async fn get_recent_forecast_accuracy(
        &self,
        tenant_id: Uuid,
        days: i32,
    ) -> Result<ForecastAccuracyAggregate> {
        let row = sqlx::query(
            r#"
            SELECT
                AVG(percentage_error) AS mape,
                AVG(
                    CASE
                        WHEN actual_value <> 0
                        THEN ((predicted_value - actual_value) / actual_value) * 100.0
                    END
                ) AS signed_bias_pct,
                COUNT(*)::bigint AS sample_count
            FROM forecast_accuracy_log
            WHERE tenant_id = $1
                AND calculated_at > NOW() - INTERVAL '1 day' * $2
            "#,
        )
        .bind(tenant_id)
        .bind(days)
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch recent forecast accuracy")?;

        let sample_count: i64 = row.try_get("sample_count").unwrap_or(0);

        Ok(ForecastAccuracyAggregate {
            mape: row.try_get("mape").ok().unwrap_or(0.0),
            signed_bias_pct: row.try_get("signed_bias_pct").ok().unwrap_or(0.0),
            sample_count,
        })
    }

    /// Fetch persisted ArimaForecaster parameter overrides for a tenant, if any.
    /// PredictiveService uses this to construct `ArimaForecaster::with_tuning`.
    pub async fn get_forecast_tuning(&self, tenant_id: Uuid) -> Result<Option<ForecastTuningRow>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                seasonality_threshold_override,
                ci_width_multiplier,
                level_bias_correction,
                mape_30d
            FROM forecast_model_tuning
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch forecast_model_tuning row")?;

        Ok(row.map(|r| ForecastTuningRow {
            tenant_id: r.try_get("tenant_id").unwrap_or(tenant_id),
            seasonality_threshold_override: row_to_f64(r.try_get("seasonality_threshold_override").ok()),
            ci_width_multiplier: row_to_f64(r.try_get("ci_width_multiplier").ok()),
            level_bias_correction: row_to_f64(r.try_get("level_bias_correction").ok()),
            mape_30d: row_to_f64(r.try_get("mape_30d").ok()),
        }))
    }

    /// Upsert per-tenant ArimaForecaster parameter overrides. Returns the
    /// previous row if one existed (for audit-logging purposes).
    pub async fn upsert_forecast_tuning(
        &self,
        tenant_id: Uuid,
        seasonality_threshold_override: Option<f64>,
        ci_width_multiplier: Option<f64>,
        level_bias_correction: Option<f64>,
        mape_30d: Option<f64>,
    ) -> Result<Option<ForecastTuningRow>> {
        let previous = self.get_forecast_tuning(tenant_id).await?;

        sqlx::query(
            r#"
            INSERT INTO forecast_model_tuning (
                tenant_id,
                seasonality_threshold_override,
                ci_width_multiplier,
                level_bias_correction,
                mape_30d,
                updated_at,
                created_at
            ) VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            ON CONFLICT (tenant_id) DO UPDATE SET
                seasonality_threshold_override = EXCLUDED.seasonality_threshold_override,
                ci_width_multiplier = EXCLUDED.ci_width_multiplier,
                level_bias_correction = EXCLUDED.level_bias_correction,
                mape_30d = EXCLUDED.mape_30d,
                updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(seasonality_threshold_override)
        .bind(ci_width_multiplier)
        .bind(level_bias_correction)
        .bind(mape_30d)
        .execute(&self.pool)
        .await
        .context("Failed to upsert forecast_model_tuning row")?;

        Ok(previous)
    }

    /// Load the per-tenant anomaly/duplicate threshold configuration. Returns
    /// the row from `anomaly_rules` that applies to all entities (entity_id IS
    /// NULL) for the `invoice_amount_outlier` rule type. The columns map to:
    ///   - zscore_threshold / iqr_multiplier -> StatisticalAnomalyDetector
    ///   - amount_tolerance / date_tolerance_days -> DuplicateDetector
    /// When no row exists the caller falls back to the detector defaults.
    pub async fn load_anomaly_rule(&self, tenant_id: Uuid) -> Result<Option<AnomalyRuleConfig>> {
        let row = sqlx::query(
            r#"
            SELECT
                zscore_threshold,
                iqr_multiplier,
                amount_tolerance,
                date_tolerance_days
            FROM anomaly_rules
            WHERE tenant_id = $1
                AND entity_id IS NULL
                AND enabled = TRUE
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load anomaly_rules row")?;

        Ok(row.map(|r| AnomalyRuleConfig {
            tenant_id,
            zscore_threshold: r.try_get("zscore_threshold").ok(),
            iqr_multiplier: r.try_get("iqr_multiplier").ok(),
            amount_tolerance: r.try_get("amount_tolerance").ok(),
            date_tolerance_days: r.try_get::<i32, _>("date_tolerance_days").ok().map(|v| v as i64),
        }))
    }

    /// Recalibrate the per-tenant anomaly/duplicate thresholds based on the
    /// acknowledged-false-positive rate observed over the last
    /// `RECALIBRATION_WINDOW_DAYS` days. For each detector type:
    ///   - false-positive rate > 0.25 -> widen the threshold (less sensitive)
    ///   - false-positive rate < 0.05 with low volume -> narrow it (more
    ///     sensitive), floored at safe minimums
    /// All adjustments are bounded by per-detector step and clamp values, and
    /// every change is written to `threshold_calibration_history`.
    pub async fn recalibrate_anomaly_thresholds(
        &self,
        tenant_id: Uuid,
        recalibrated_by: Uuid,
    ) -> Result<RecalibrationOutcome> {
        let current = self
            .load_anomaly_rule(tenant_id)
            .await?
            .unwrap_or_else(|| AnomalyRuleConfig::defaults(tenant_id));

        let mut outcome = RecalibrationOutcome::default();
        outcome.tenant_id = tenant_id;

        let detector_types = [
            ("zscore", AnomalyType::InvoiceAmountOutlier),
            ("iqr", AnomalyType::InvoiceAmountOutlier),
            ("duplicate", AnomalyType::DuplicateInvoice),
        ];

        let mut adjustments: Vec<DetectorAdjustment> = Vec::new();
        for (detector_label, anomaly_type) in detector_types {
            let stats = self
                .detector_fp_stats(tenant_id, anomaly_type)
                .await
                .with_context(|| {
                    format!("Failed to compute fp stats for {}", detector_label)
                })?;

            let adjustment = compute_adjustment(detector_label, &current, &stats);
            adjustments.push(DetectorAdjustment {
                detector_type: detector_label.to_string(),
                stats,
                old_zscore: current.effective_zscore(),
                old_iqr: current.effective_iqr(),
                old_amount_tolerance: current.effective_amount_tolerance(),
                old_date_tolerance_days: current.effective_date_tolerance_days(),
                new_zscore: adjustment.new_zscore,
                new_iqr: adjustment.new_iqr,
                new_amount_tolerance: adjustment.new_amount_tolerance,
                new_date_tolerance_days: adjustment.new_date_tolerance_days,
            });
        }

        // Fold per-detector adjustments into the next persisted row. Each
        // detector only updates the field(s) it owns, otherwise the current
        // value (or default) is preserved.
        let mut next = current.clone();
        for adj in &adjustments {
            if let Some(z) = adj.new_zscore {
                next.zscore_threshold = Some(z);
            }
            if let Some(i) = adj.new_iqr {
                next.iqr_multiplier = Some(i);
            }
            if let Some(a) = adj.new_amount_tolerance {
                next.amount_tolerance = Some(a);
            }
            if let Some(d) = adj.new_date_tolerance_days {
                next.date_tolerance_days = Some(d);
            }
        }

        // Persist the new thresholds. Skip writing when nothing changed.
        let changed = next.effective_zscore() != current.effective_zscore()
            || next.effective_iqr() != current.effective_iqr()
            || next.effective_amount_tolerance() != current.effective_amount_tolerance()
            || next.effective_date_tolerance_days() != current.effective_date_tolerance_days();

        if changed {
            self.upsert_anomaly_rule(&next, recalibrated_by)
                .await
                .context("Failed to upsert recalibrated anomaly_rules row")?;
        }

        // Audit each detector's evaluation. We write history for *moved*
        // thresholds; unchanged detectors are still surfaced in the outcome
        // for observability but not persisted to history (keeps the audit
        // table focused on actual changes).
        for adj in &adjustments {
            for (label, old, new) in adj.movements() {
                self.write_calibration_history(
                    tenant_id,
                    label,
                    Some(old),
                    new,
                    Some(adj.stats.fp_rate),
                    adj.stats.sample_size,
                )
                .await
                .context("Failed to write threshold_calibration_history row")?;
            }
        }

        outcome.zscore_threshold = next.effective_zscore();
        outcome.iqr_multiplier = next.effective_iqr();
        outcome.amount_tolerance = next.effective_amount_tolerance();
        outcome.date_tolerance_days = next.effective_date_tolerance_days();
        outcome.adjustments = adjustments;
        outcome.persisted = changed;
        Ok(outcome)
    }

    async fn detector_fp_stats(
        &self,
        tenant_id: Uuid,
        anomaly_type: AnomalyType,
    ) -> Result<DetectorFpStats> {
        let type_str = serde_json::to_string(&anomaly_type)?;
        let type_str = type_str.trim_matches('"').to_string();
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*)::bigint AS total,
                SUM(CASE WHEN false_positive THEN 1 ELSE 0 END)::bigint AS fp_count
            FROM invoice_anomalies
            WHERE tenant_id = $1
                AND anomaly_type = $2
                AND detected_at > NOW() - INTERVAL '1 day' * $3
            "#,
        )
        .bind(tenant_id)
        .bind(type_str)
        .bind(RECALIBRATION_WINDOW_DAYS)
        .fetch_one(&self.pool)
        .await
        .context("Failed to query anomaly fp stats")?;

        let total: i64 = row.try_get("total").unwrap_or(0);
        let fp_count: i64 = row.try_get("fp_count").unwrap_or(0);
        let fp_rate = if total > 0 {
            fp_count as f64 / total as f64
        } else {
            0.0
        };
        Ok(DetectorFpStats {
            sample_size: total as i32,
            fp_count: fp_count as i32,
            fp_rate,
        })
    }

    async fn upsert_anomaly_rule(
        &self,
        cfg: &AnomalyRuleConfig,
        recalibrated_by: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO anomaly_rules (
                id, tenant_id, anomaly_type, entity_type, entity_id,
                zscore_threshold, iqr_multiplier, amount_tolerance,
                date_tolerance_days, enabled, created_by, updated_at
            ) VALUES (
                gen_random_uuid(), $1, 'invoice_amount_outlier', NULL, NULL,
                $2, $3, $4, $5, TRUE,
                $6,
                NOW()
            )
            ON CONFLICT (tenant_id, anomaly_type, entity_type, entity_id)
            DO UPDATE SET
                zscore_threshold = EXCLUDED.zscore_threshold,
                iqr_multiplier = EXCLUDED.iqr_multiplier,
                amount_tolerance = EXCLUDED.amount_tolerance,
                date_tolerance_days = EXCLUDED.date_tolerance_days,
                updated_at = NOW()
            "#,
        )
        .bind(cfg.tenant_id)
        .bind(cfg.effective_zscore())
        .bind(cfg.effective_iqr())
        .bind(cfg.effective_amount_tolerance())
        .bind(cfg.effective_date_tolerance_days() as i32)
        .bind(recalibrated_by)
        .execute(&self.pool)
        .await
        .context("Failed to upsert anomaly_rules row")?;
        Ok(())
    }

    async fn write_calibration_history(
        &self,
        tenant_id: Uuid,
        detector_type: &str,
        old_value: Option<f64>,
        new_value: f64,
        fp_rate: Option<f64>,
        sample_size: i32,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO threshold_calibration_history
                (tenant_id, detector_type, old_value, new_value, fp_rate, sample_size)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(tenant_id)
        .bind(detector_type)
        .bind(old_value)
        .bind(new_value)
        .bind(fp_rate)
        .bind(sample_size)
        .execute(&self.pool)
        .await
        .context("Failed to insert threshold_calibration_history row")?;
        Ok(())
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
    pub mape: f64, // Mean Absolute Percentage Error
    pub mae: f64,  // Mean Absolute Error
    pub rmse: f64, // Root Mean Squared Error
    pub total_forecasts: i32,
}

/// Aggregated accuracy metrics for a recent time window, used by the
/// forecast-tuning worker to drive per-tenant parameter overrides.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ForecastAccuracyAggregate {
    /// Mean of `percentage_error` over rows with non-null percentage.
    pub mape: f64,
    /// Mean signed deviation `((predicted - actual) / actual) * 100`.
    pub signed_bias_pct: f64,
    /// Number of rows included in the aggregate.
    pub sample_count: i64,
}

/// Persisted ArimaForecaster parameter overrides for a tenant. Returned to
/// PredictiveService via [`PredictiveRepository::get_forecast_tuning`].
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastTuningRow {
    pub tenant_id: Uuid,
    pub seasonality_threshold_override: Option<f64>,
    pub ci_width_multiplier: Option<f64>,
    /// Multiplicative correction applied to ArimaForecaster predicted_value
    /// (issue #398). Learned from signed bias; flows into `ForecasterTuning`.
    pub level_bias_correction: Option<f64>,
    pub mape_30d: Option<f64>,
}

/// Coerce a sqlx value (DECIMAL/f8/etc.) to a plain f64. Returns `None` when
/// the underlying SQL value was NULL.
fn row_to_f64(v: Option<f64>) -> Option<f64> {
    v
}

// --- Anomaly threshold learning (issue #397) ----------------------------------

/// Window the recalibration loop looks back over when computing the
/// false-positive rate. 30 days mirrors the forecast-tuning worker's MAPE
/// window and gives enough acknowledged samples to dampen noise.
pub const RECALIBRATION_WINDOW_DAYS: i32 = 30;
/// False-positive rate above which the relevant detector is widened (made
/// less sensitive) on the next recalibration tick.
pub const RECALIBRATION_HIGH_FP_BAND: f64 = 0.25;
/// False-positive rate below which the detector is nudged tighter when total
/// volume is also low (i.e. we are likely missing real anomalies).
pub const RECALIBRATION_LOW_FP_BAND: f64 = 0.05;
/// Minimum total sample size required before any adjustment is applied.
pub const RECALIBRATION_MIN_SAMPLE: i32 = 5;
/// Minimum sample size required before *narrowing* a threshold. Narrowing
/// without enough evidence can over-fire, so we hold the line until we have
/// at least this many observations.
pub const RECALIBRATION_NARROW_MIN_SAMPLE: i32 = 20;

/// Per-detector bounded adjustment steps and clamps.
pub const ZSCORE_STEP: f64 = 0.25;
pub const ZSCORE_MIN: f64 = 2.0;
pub const ZSCORE_MAX: f64 = 5.0;
pub const IQR_STEP: f64 = 0.1;
pub const IQR_MIN: f64 = 1.0;
pub const IQR_MAX: f64 = 3.0;
pub const AMOUNT_TOLERANCE_STEP: f64 = 0.005;
pub const AMOUNT_TOLERANCE_MIN: f64 = 0.005;
pub const AMOUNT_TOLERANCE_MAX: f64 = 0.10;
pub const DATE_TOLERANCE_STEP: i64 = 1;
pub const DATE_TOLERANCE_MIN: i64 = 3;
pub const DATE_TOLERANCE_MAX: i64 = 30;

/// Loaded per-tenant detector configuration. Mirrors the persisted columns
/// (which are nullable when no rule has been configured) and exposes
/// `effective_*` accessors that fall back to the detector defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomalyRuleConfig {
    pub tenant_id: Uuid,
    pub zscore_threshold: Option<f64>,
    pub iqr_multiplier: Option<f64>,
    pub amount_tolerance: Option<f64>,
    pub date_tolerance_days: Option<i64>,
}

impl AnomalyRuleConfig {
    pub fn defaults(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            zscore_threshold: None,
            iqr_multiplier: None,
            amount_tolerance: None,
            date_tolerance_days: None,
        }
    }

    pub fn effective_zscore(&self) -> f64 {
        self.zscore_threshold
            .unwrap_or(crate::anomaly_detection::DEFAULT_ZSCORE_THRESHOLD)
    }

    pub fn effective_iqr(&self) -> f64 {
        self.iqr_multiplier
            .unwrap_or(crate::anomaly_detection::DEFAULT_IQR_MULTIPLIER)
    }

    pub fn effective_amount_tolerance(&self) -> f64 {
        self.amount_tolerance
            .unwrap_or(crate::anomaly_detection::DEFAULT_AMOUNT_TOLERANCE)
    }

    pub fn effective_date_tolerance_days(&self) -> i64 {
        self.date_tolerance_days
            .unwrap_or(crate::anomaly_detection::DEFAULT_DATE_TOLERANCE_DAYS)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetectorFpStats {
    pub sample_size: i32,
    pub fp_count: i32,
    pub fp_rate: f64,
}

/// Result of a single detector's recalibration tick. Pure data carrier so the
/// caller (and tests) can see what moved and what triggered it.
#[derive(Debug, Clone)]
pub struct DetectorAdjustment {
    pub detector_type: String,
    pub stats: DetectorFpStats,
    pub old_zscore: f64,
    pub old_iqr: f64,
    pub old_amount_tolerance: f64,
    pub old_date_tolerance_days: i64,
    pub new_zscore: Option<f64>,
    pub new_iqr: Option<f64>,
    pub new_amount_tolerance: Option<f64>,
    pub new_date_tolerance_days: Option<i64>,
}

impl DetectorAdjustment {
    /// Iterate the (label, old, new) movements this adjustment actually made.
    /// Used to write `threshold_calibration_history` rows only for real moves.
    pub fn movements(&self) -> Vec<(&'static str, f64, f64)> {
        let mut out = Vec::new();
        if let Some(z) = self.new_zscore {
            if (z - self.old_zscore).abs() > f64::EPSILON {
                out.push(("zscore", self.old_zscore, z));
            }
        }
        if let Some(i) = self.new_iqr {
            if (i - self.old_iqr).abs() > f64::EPSILON {
                out.push(("iqr", self.old_iqr, i));
            }
        }
        if let Some(a) = self.new_amount_tolerance {
            if (a - self.old_amount_tolerance).abs() > f64::EPSILON {
                out.push(("amount_tolerance", self.old_amount_tolerance, a));
            }
        }
        if let Some(d) = self.new_date_tolerance_days {
            if d != self.old_date_tolerance_days {
                out.push((
                    "date_tolerance_days",
                    self.old_date_tolerance_days as f64,
                    d as f64,
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct RecalibrationOutcome {
    pub tenant_id: Uuid,
    pub zscore_threshold: f64,
    pub iqr_multiplier: f64,
    pub amount_tolerance: f64,
    pub date_tolerance_days: i64,
    pub adjustments: Vec<DetectorAdjustment>,
    /// TRUE when the new thresholds were written back to anomaly_rules
    /// (i.e. at least one detector actually moved).
    pub persisted: bool,
}

/// Compute a single detector's threshold movement from its observed
/// false-positive stats. Pure function so it can be unit-tested without a
/// database.
pub(crate) fn compute_adjustment(
    detector_type: &str,
    current: &AnomalyRuleConfig,
    stats: &DetectorFpStats,
) -> ProposedThresholds {
    let mut proposed = ProposedThresholds::default();
    if stats.sample_size < RECALIBRATION_MIN_SAMPLE {
        return proposed;
    }

    let widen = stats.fp_rate > RECALIBRATION_HIGH_FP_BAND;
    let narrow = stats.fp_rate < RECALIBRATION_LOW_FP_BAND
        && stats.sample_size >= RECALIBRATION_NARROW_MIN_SAMPLE;

    match detector_type {
        "zscore" => {
            let cur = current.effective_zscore();
            let next = if widen {
                (cur + ZSCORE_STEP).min(ZSCORE_MAX)
            } else if narrow {
                (cur - ZSCORE_STEP).max(ZSCORE_MIN)
            } else {
                cur
            };
            proposed.new_zscore = Some(next);
        }
        "iqr" => {
            let cur = current.effective_iqr();
            let next = if widen {
                (cur + IQR_STEP).min(IQR_MAX)
            } else if narrow {
                (cur - IQR_STEP).max(IQR_MIN)
            } else {
                cur
            };
            proposed.new_iqr = Some(next);
        }
        "duplicate" => {
            // Duplicate detector has two knobs. Both move in the
            // less-sensitive direction when FPs are high:
            //   - amount_tolerance widens (more tolerant of off-by-cents)
            //   - date_tolerance_days narrows (tighter date window so fewer
            //     unrelated invoices collide)
            let cur_amt = current.effective_amount_tolerance();
            let next_amt = if widen {
                (cur_amt + AMOUNT_TOLERANCE_STEP).min(AMOUNT_TOLERANCE_MAX)
            } else if narrow {
                (cur_amt - AMOUNT_TOLERANCE_STEP).max(AMOUNT_TOLERANCE_MIN)
            } else {
                cur_amt
            };
            proposed.new_amount_tolerance = Some(next_amt);

            let cur_days = current.effective_date_tolerance_days();
            let next_days = if widen {
                (cur_days - DATE_TOLERANCE_STEP).max(DATE_TOLERANCE_MIN)
            } else if narrow {
                (cur_days + DATE_TOLERANCE_STEP).min(DATE_TOLERANCE_MAX)
            } else {
                cur_days
            };
            proposed.new_date_tolerance_days = Some(next_days);
        }
        _ => {}
    }

    proposed
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProposedThresholds {
    pub new_zscore: Option<f64>,
    pub new_iqr: Option<f64>,
    pub new_amount_tolerance: Option<f64>,
    pub new_date_tolerance_days: Option<i64>,
}

#[cfg(test)]
mod threshold_learning_tests {
    use super::*;

    fn cfg(z: f64, i: f64, amt: f64, days: i64) -> AnomalyRuleConfig {
        AnomalyRuleConfig {
            tenant_id: Uuid::new_v4(),
            zscore_threshold: Some(z),
            iqr_multiplier: Some(i),
            amount_tolerance: Some(amt),
            date_tolerance_days: Some(days),
        }
    }

    #[test]
    fn high_fp_rate_widens_zscore() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 40,
            fp_count: 15,
            fp_rate: 0.375,
        };
        let p = compute_adjustment("zscore", &current, &stats);
        assert_eq!(p.new_zscore, Some(3.25));
    }

    #[test]
    fn low_fp_rate_with_volume_narrows_iqr() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 50,
            fp_count: 1,
            fp_rate: 0.02,
        };
        let p = compute_adjustment("iqr", &current, &stats);
        assert_eq!(p.new_iqr, Some(1.4));
    }

    #[test]
    fn narrow_blocked_when_sample_below_floor() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 10,
            fp_count: 0,
            fp_rate: 0.0,
        };
        let p = compute_adjustment("zscore", &current, &stats);
        // Sample meets MIN but is below NARROW_MIN -> no movement.
        assert_eq!(p.new_zscore, Some(3.0));
    }

    #[test]
    fn no_adjustment_below_min_sample() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 2,
            fp_count: 2,
            fp_rate: 1.0,
        };
        let p = compute_adjustment("zscore", &current, &stats);
        assert_eq!(p, ProposedThresholds::default());
    }

    #[test]
    fn zscore_clamped_to_max() {
        let current = cfg(ZSCORE_MAX, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 40,
            fp_count: 20,
            fp_rate: 0.5,
        };
        let p = compute_adjustment("zscore", &current, &stats);
        assert_eq!(p.new_zscore, Some(ZSCORE_MAX));
    }

    #[test]
    fn zscore_clamped_to_min_when_narrowing() {
        let current = cfg(ZSCORE_MIN, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 60,
            fp_count: 0,
            fp_rate: 0.0,
        };
        let p = compute_adjustment("zscore", &current, &stats);
        assert_eq!(p.new_zscore, Some(ZSCORE_MIN));
    }

    #[test]
    fn duplicate_widens_amount_and_tightens_date_when_high_fp() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 20,
            fp_count: 12,
            fp_rate: 0.6,
        };
        let p = compute_adjustment("duplicate", &current, &stats);
        assert_eq!(p.new_amount_tolerance, Some(0.025));
        assert_eq!(p.new_date_tolerance_days, Some(13));
    }

    #[test]
    fn duplicate_amount_tolerance_clamped_to_max() {
        let current = cfg(3.0, 1.5, AMOUNT_TOLERANCE_MAX, 14);
        let stats = DetectorFpStats {
            sample_size: 20,
            fp_count: 12,
            fp_rate: 0.6,
        };
        let p = compute_adjustment("duplicate", &current, &stats);
        assert_eq!(p.new_amount_tolerance, Some(AMOUNT_TOLERANCE_MAX));
    }

    #[test]
    fn duplicate_date_tolerance_clamped_to_min() {
        let current = cfg(3.0, 1.5, 0.02, DATE_TOLERANCE_MIN);
        let stats = DetectorFpStats {
            sample_size: 20,
            fp_count: 12,
            fp_rate: 0.6,
        };
        let p = compute_adjustment("duplicate", &current, &stats);
        assert_eq!(p.new_date_tolerance_days, Some(DATE_TOLERANCE_MIN));
    }

    #[test]
    fn unknown_detector_no_movement() {
        let current = cfg(3.0, 1.5, 0.02, 14);
        let stats = DetectorFpStats {
            sample_size: 50,
            fp_count: 25,
            fp_rate: 0.5,
        };
        let p = compute_adjustment("nonexistent", &current, &stats);
        assert_eq!(p, ProposedThresholds::default());
    }

    #[test]
    fn adjustment_movements_reports_only_actual_changes() {
        let adj = DetectorAdjustment {
            detector_type: "zscore".to_string(),
            stats: DetectorFpStats::default(),
            old_zscore: 3.0,
            old_iqr: 1.5,
            old_amount_tolerance: 0.02,
            old_date_tolerance_days: 14,
            new_zscore: Some(3.25),
            new_iqr: Some(1.5), // unchanged
            new_amount_tolerance: None,
            new_date_tolerance_days: None,
        };
        let moves = adj.movements();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].0, "zscore");
        assert!((moves[0].1 - 3.0).abs() < f64::EPSILON);
        assert!((moves[0].2 - 3.25).abs() < f64::EPSILON);
    }

    #[test]
    fn defaults_fall_back_to_detector_constants() {
        let c = AnomalyRuleConfig::defaults(Uuid::new_v4());
        assert_eq!(
            c.effective_zscore(),
            crate::anomaly_detection::DEFAULT_ZSCORE_THRESHOLD
        );
        assert_eq!(
            c.effective_iqr(),
            crate::anomaly_detection::DEFAULT_IQR_MULTIPLIER
        );
        assert_eq!(
            c.effective_amount_tolerance(),
            crate::anomaly_detection::DEFAULT_AMOUNT_TOLERANCE
        );
        assert_eq!(
            c.effective_date_tolerance_days(),
            crate::anomaly_detection::DEFAULT_DATE_TOLERANCE_DAYS
        );
    }
}
