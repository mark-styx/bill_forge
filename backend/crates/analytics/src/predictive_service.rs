//! Predictive Analytics Service
//!
//! Business logic for forecasting, anomaly detection, and proactive alerts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::{info, warn};
use uuid::Uuid;

use crate::anomaly_detection::{DuplicateDetector, InvoiceRecord, StatisticalAnomalyDetector};
use crate::forecasting::ArimaForecaster;
use crate::predictive_models::*;
use crate::predictive_repository::PredictiveRepository;

pub struct PredictiveService {
    repo: PredictiveRepository,
}

impl PredictiveService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: PredictiveRepository::new(pool),
        }
    }

    /// Generate forecasts for a list of vendors
    pub async fn generate_vendor_forecasts(
        &self,
        tenant_id: Uuid,
        vendor_ids: &[String],
    ) -> Result<Vec<Forecast>> {
        info!(
            "Generating forecasts for {} vendors in tenant {}",
            vendor_ids.len(),
            tenant_id
        );

        let mut forecasts = Vec::new();

        for vendor_id in vendor_ids {
            // Fetch historical spend data (last 90 days)
            let time_series = self
                .repo
                .get_vendor_spend_timeseries(tenant_id, vendor_id, 90)
                .await
                .context("Failed to fetch vendor time series")?;

            // Need at least 30 data points for ARIMA
            if time_series.points.len() < 30 {
                warn!(
                    "Insufficient data for vendor {}: {} points (need 30)",
                    vendor_id,
                    time_series.points.len()
                );
                continue;
            }

            // Fit ARIMA model
            let mut forecaster = ArimaForecaster::new();
            forecaster
                .fit(&time_series)
                .await
                .context("Failed to fit ARIMA model")?;

            // Generate forecasts for all horizons
            for horizon in &[
                ForecastHorizon::Days30,
                ForecastHorizon::Days60,
                ForecastHorizon::Days90,
            ] {
                match forecaster.forecast(*horizon).await {
                    Ok(forecast) => {
                        // Save to database
                        match self.repo.save_forecast(tenant_id, &forecast).await {
                            Ok(id) => {
                                info!(
                                    "Saved forecast {} for vendor {} with horizon {:?}",
                                    id, vendor_id, horizon
                                );
                                forecasts.push(forecast);
                            }
                            Err(e) => {
                                warn!("Failed to save forecast for vendor {}: {}", vendor_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to forecast for vendor {}: {}", vendor_id, e);
                    }
                }
            }
        }

        info!("Generated {} forecasts total", forecasts.len());
        Ok(forecasts)
    }

    /// Generate forecasts for departments
    pub async fn generate_department_forecasts(
        &self,
        tenant_id: Uuid,
        departments: &[String],
    ) -> Result<Vec<Forecast>> {
        info!(
            "Generating forecasts for {} departments in tenant {}",
            departments.len(),
            tenant_id
        );

        let mut forecasts = Vec::new();

        for department in departments {
            // Fetch historical spend data
            let time_series = self
                .repo
                .get_department_spend_timeseries(tenant_id, department, 90)
                .await
                .context("Failed to fetch department time series")?;

            if time_series.points.len() < 30 {
                warn!(
                    "Insufficient data for department {}: {} points",
                    department,
                    time_series.points.len()
                );
                continue;
            }

            // Fit ARIMA model
            let mut forecaster = ArimaForecaster::new();
            if let Err(e) = forecaster.fit(&time_series).await {
                warn!(
                    "Failed to fit ARIMA model for department {}: {}",
                    department, e
                );
                continue;
            }

            // Generate 30-day forecast
            match forecaster.forecast(ForecastHorizon::Days30).await {
                Ok(forecast) => {
                    if let Err(e) = self.repo.save_forecast(tenant_id, &forecast).await {
                        warn!(
                            "Failed to save forecast for department {}: {}",
                            department, e
                        );
                    } else {
                        forecasts.push(forecast);
                    }
                }
                Err(e) => {
                    warn!("Failed to forecast for department {}: {}", department, e);
                }
            }
        }

        Ok(forecasts)
    }

    /// Detect anomalies in recent invoices
    pub async fn detect_invoice_anomalies(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        days: i32,
    ) -> Result<Vec<Anomaly>> {
        info!(
            "Running anomaly detection for tenant {} over last {} days",
            tenant_id, days
        );

        // Fetch recent invoices for anomaly detection
        let invoices = self.fetch_recent_invoices(pool, tenant_id, days).await?;

        if invoices.is_empty() {
            info!("No invoices found for anomaly detection");
            return Ok(Vec::new());
        }

        let mut all_anomalies = Vec::new();

        // 1. Detect duplicate invoices
        let duplicate_detector = DuplicateDetector::new(tenant_id);
        match duplicate_detector.detect_duplicates(&invoices) {
            Ok(duplicates) => {
                info!("Detected {} duplicate invoice anomalies", duplicates.len());
                for anomaly in &duplicates {
                    if let Err(e) = self.repo.save_anomaly(anomaly).await {
                        warn!("Failed to save duplicate anomaly: {}", e);
                    }
                }
                all_anomalies.extend(duplicates);
            }
            Err(e) => {
                warn!("Duplicate detection failed: {}", e);
            }
        }

        // 2. Detect amount outliers (statistical)
        let stat_detector = StatisticalAnomalyDetector::new(tenant_id);

        // Group invoices by vendor for outlier detection
        let mut vendor_invoices: std::collections::HashMap<String, Vec<&InvoiceRecord>> =
            std::collections::HashMap::new();
        for invoice in &invoices {
            vendor_invoices
                .entry(invoice.vendor_name.clone())
                .or_insert_with(Vec::new)
                .push(invoice);
        }

        // Detect outliers per vendor
        for (vendor_name, vendor_invoice_list) in vendor_invoices {
            if vendor_invoice_list.len() < 10 {
                continue; // Need enough data for meaningful outlier detection
            }

            // Convert to time series
            let time_series = TimeSeries {
                entity_id: vendor_name.clone(),
                entity_type: EntityType::Vendor,
                metric_name: "invoice_amount".to_string(),
                points: vendor_invoice_list
                    .iter()
                    .map(|inv| TimeSeriesPoint {
                        timestamp: inv.invoice_date,
                        value: inv.amount,
                    })
                    .collect(),
            };

            match stat_detector.detect_amount_outliers(&time_series) {
                Ok(outliers) => {
                    info!(
                        "Detected {} amount outliers for vendor {}",
                        outliers.len(),
                        vendor_name
                    );
                    for anomaly in &outliers {
                        if let Err(e) = self.repo.save_anomaly(anomaly).await {
                            warn!("Failed to save outlier anomaly: {}", e);
                        }
                    }
                    all_anomalies.extend(outliers);
                }
                Err(e) => {
                    warn!("Outlier detection failed for vendor {}: {}", vendor_name, e);
                }
            }
        }

        info!("Total anomalies detected: {}", all_anomalies.len());
        Ok(all_anomalies)
    }

    /// Check budget thresholds and generate alerts
    pub async fn check_budget_thresholds(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        info!("Checking budget thresholds for tenant {}", tenant_id);

        let mut alert_ids = Vec::new();

        // Check vendor concentration (top vendor spend > 50% of total)
        let vendor_stats = sqlx::query(
            r#"
            WITH vendor_spend AS (
                SELECT
                    vendor_id::text,
                    SUM(total_amount_cents) as total_spend
                FROM invoices
                WHERE tenant_id = $1
                    AND created_at > NOW() - INTERVAL '30 days'
                    AND status != 'rejected'
                GROUP BY vendor_id
            ),
            totals AS (
                SELECT SUM(total_spend) as grand_total FROM vendor_spend
            )
            SELECT
                v.vendor_id,
                v.total_spend,
                t.grand_total,
                (v.total_spend / NULLIF(t.grand_total, 0) * 100) as percentage
            FROM vendor_spend v
            CROSS JOIN totals t
            WHERE (v.total_spend / NULLIF(t.grand_total, 0) * 100) > 50
            ORDER BY percentage DESC
            LIMIT 5
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .context("Failed to check vendor concentration")?;

        for row in vendor_stats {
            let vendor_id: String = row.try_get("vendor_id").unwrap_or_default();
            let percentage: f64 = row.try_get("percentage").unwrap_or(0.0);

            let alert_id = self
                .repo
                .save_budget_alert(
                    tenant_id,
                    "vendor_concentration",
                    "high",
                    Some(&vendor_id),
                    Some("vendor"),
                    "High Vendor Concentration Risk",
                    &format!(
                        "Vendor {} accounts for {:.1}% of total spend (>50% threshold)",
                        vendor_id, percentage
                    ),
                    Some(50.0),
                    Some(percentage),
                    Some(percentage),
                    Some("Consider diversifying vendor base to reduce dependency"),
                )
                .await?;

            alert_ids.push(alert_id);
        }

        // Check department budget thresholds (if budgets configured)
        // This would require a budgets table - placeholder for now

        info!("Generated {} budget alerts", alert_ids.len());
        Ok(alert_ids)
    }

    /// Calculate forecast accuracy by comparing past forecasts to actuals
    pub async fn calculate_forecast_accuracy(&self, pool: &PgPool, tenant_id: Uuid) -> Result<()> {
        info!("Calculating forecast accuracy for tenant {}", tenant_id);

        // Get forecasts that are now in the past (horizon has passed)
        let old_forecasts = sqlx::query(
            r#"
            SELECT
                id,
                entity_id,
                entity_type,
                metric_name,
                horizon,
                predicted_value,
                generated_at
            FROM spend_forecasts
            WHERE tenant_id = $1
                AND valid_until < NOW()
                AND generated_at > NOW() - INTERVAL '180 days'
            ORDER BY generated_at DESC
            LIMIT 100
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .context("Failed to fetch old forecasts")?;

        for row in old_forecasts {
            let forecast_id: Uuid = row.try_get("id")?;
            let entity_id: String = row.try_get("entity_id")?;
            let entity_type: String = row.try_get("entity_type")?;
            let metric_name: String = row.try_get("metric_name")?;
            let horizon: String = row.try_get("horizon")?;
            let predicted_value: f64 = row.try_get("predicted_value").unwrap_or(0.0);
            let forecast_date: DateTime<Utc> = row.try_get("generated_at")?;

            // Calculate actual value for the forecast period
            // This is simplified - would need to sum spend over the forecast horizon
            let actual_value = match entity_type.as_str() {
                "\"Vendor\"" | "Vendor" => {
                    self.calculate_actual_spend_for_vendor(
                        pool,
                        tenant_id,
                        &entity_id,
                        &forecast_date,
                    )
                    .await?
                }
                _ => {
                    warn!("Unknown entity type: {}", entity_type);
                    continue;
                }
            };

            let actual_date = Utc::now();

            // Log accuracy
            if let Err(e) = self
                .repo
                .log_forecast_accuracy(
                    tenant_id,
                    forecast_id,
                    &entity_id,
                    &entity_type,
                    &metric_name,
                    &horizon,
                    predicted_value,
                    actual_value,
                    forecast_date,
                    actual_date,
                )
                .await
            {
                warn!("Failed to log forecast accuracy: {}", e);
            }
        }

        Ok(())
    }

    /// Helper: Fetch recent invoices for anomaly detection
    async fn fetch_recent_invoices(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        days: i32,
    ) -> Result<Vec<InvoiceRecord>> {
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
                AND i.created_at > NOW() - INTERVAL '1 day' * $2
            ORDER BY i.created_at DESC
            LIMIT 1000
            "#,
        )
        .bind(tenant_id)
        .bind(days)
        .fetch_all(pool)
        .await
        .context("Failed to fetch recent invoices")?;

        let invoices = rows
            .into_iter()
            .filter_map(|row| {
                // Get BigDecimal from database and convert to f64
                let amount_str: String = row.try_get("total_amount_cents").ok()?;
                let amount = amount_str.parse::<f64>().ok()?;

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

    /// Helper: Calculate actual spend for a vendor over a forecast period
    async fn calculate_actual_spend_for_vendor(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        vendor_id: &str,
        forecast_date: &DateTime<Utc>,
    ) -> Result<f64> {
        // Calculate 30 days of actual spend from forecast date
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(total_amount_cents), 0)::float as actual_spend
            FROM invoices
            WHERE tenant_id = $1
                AND vendor_id = $2
                AND created_at >= $3
                AND created_at < $3 + INTERVAL '30 days'
                AND status != 'rejected'
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(forecast_date)
        .fetch_one(pool)
        .await
        .context("Failed to calculate actual spend")?;

        let actual_spend: f64 = row.try_get("actual_spend").unwrap_or(0.0);
        Ok(actual_spend)
    }
}
