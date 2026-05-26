//! Analytics Repository
//!
//! Database operations for analytics data.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{AnalyticsEvent, FeatureUsage, PerformanceMetric, TrendData};

pub struct AnalyticsRepository {
    pool: PgPool,
}

impl AnalyticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Record a new analytics event
    pub async fn record_event(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        event_type: &str,
        event_category: &str,
        event_data: serde_json::Value,
    ) -> Result<AnalyticsEvent> {
        let event = sqlx::query_as::<_, AnalyticsEvent>(
            r#"
            INSERT INTO analytics_events (tenant_id, user_id, event_type, event_category, event_data)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(event_type)
        .bind(event_category)
        .bind(event_data)
        .fetch_one(&self.pool)
        .await
        .context("Failed to record analytics event")?;

        Ok(event)
    }

    /// Get feature usage metrics for a time period
    pub async fn get_feature_usage(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<FeatureUsage>> {
        let usage = sqlx::query_as::<_, FeatureUsage>(
            r#"
            SELECT
                event_type AS feature_name,
                COUNT(*) AS usage_count,
                COUNT(DISTINCT user_id) AS unique_users,
                AVG((event_data->>'duration_ms')::float) AS avg_duration_ms,
                MAX(created_at) AS last_used
            FROM analytics_events
            WHERE tenant_id = $1
              AND created_at >= $2
              AND created_at <= $3
            GROUP BY event_type
            ORDER BY usage_count DESC
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get feature usage")?;

        Ok(usage)
    }

    /// Get performance metrics for API endpoints
    pub async fn get_performance_metrics(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<PerformanceMetric>> {
        let metrics = sqlx::query_as::<_, PerformanceMetric>(
            r#"
            SELECT
                event_type AS endpoint,
                AVG((event_data->>'response_time_ms')::float) AS avg_response_time_ms,
                PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (event_data->>'response_time_ms')::float) AS p95_response_time_ms,
                PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY (event_data->>'response_time_ms')::float) AS p99_response_time_ms,
                COUNT(*) AS request_count,
                SUM(CASE WHEN (event_data->>'status_code')::int >= 400 THEN 1 ELSE 0 END) AS error_count,
                AVG(CASE WHEN (event_data->>'status_code')::int >= 400 THEN 1.0 ELSE 0.0 END) AS error_rate
            FROM analytics_events
            WHERE tenant_id = $1
              AND event_category = 'api_request'
              AND created_at >= $2
              AND created_at <= $3
            GROUP BY event_type
            ORDER BY request_count DESC
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get performance metrics")?;

        Ok(metrics)
    }

    /// Get total event count for a period
    pub async fn get_event_count(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM analytics_events
            WHERE tenant_id = $1
              AND created_at >= $2
              AND created_at <= $3
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to get event count")?;

        Ok(count.0)
    }

    /// Get unique user count for a period
    pub async fn get_unique_user_count(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(DISTINCT user_id)
            FROM analytics_events
            WHERE tenant_id = $1
              AND created_at >= $2
              AND created_at <= $3
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to get unique user count")?;

        Ok(count.0)
    }

    /// Get trend data comparing current vs previous period
    pub async fn get_trend_data(
        &self,
        tenant_id: &str,
        metric_name: &str,
        current_start: DateTime<Utc>,
        current_end: DateTime<Utc>,
        previous_start: DateTime<Utc>,
        previous_end: DateTime<Utc>,
    ) -> Result<TrendData> {
        let current_value: f64 = self
            .get_metric_value(tenant_id, metric_name, current_start, current_end)
            .await?;
        let previous_value: f64 = self
            .get_metric_value(tenant_id, metric_name, previous_start, previous_end)
            .await?;

        let change_percentage = if previous_value > 0.0 {
            ((current_value - previous_value) / previous_value) * 100.0
        } else {
            0.0
        };

        let trend = if change_percentage > 5.0 {
            crate::models::Trend::Increasing
        } else if change_percentage < -5.0 {
            crate::models::Trend::Decreasing
        } else {
            crate::models::Trend::Stable
        };

        Ok(TrendData {
            metric_name: metric_name.to_string(),
            current_value,
            previous_value,
            change_percentage,
            trend,
        })
    }

    async fn get_metric_value(
        &self,
        tenant_id: &str,
        metric_name: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<f64> {
        let value: (f64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)::float
            FROM analytics_events
            WHERE tenant_id = $1
              AND event_type = $2
              AND created_at >= $3
              AND created_at <= $4
            "#,
        )
        .bind(tenant_id)
        .bind(metric_name)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to get metric value")?;

        Ok(value.0)
    }
}
