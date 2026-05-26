//! Daily Analytics Aggregation Job
//!
//! Pre-calculates daily analytics summaries for faster queries.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use tracing::{info, warn};

pub struct DailyAggregationJob {
    pool: PgPool,
}

impl DailyAggregationJob {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run daily aggregation for all tenants
    pub async fn run(&self) -> Result<()> {
        info!("Starting daily analytics aggregation job");

        // Get all tenant IDs
        let tenants: Vec<String> = sqlx::query_scalar("SELECT id FROM tenants")
            .fetch_all(&self.pool)
            .await
            .context("Failed to fetch tenants")?;

        info!("Aggregating analytics for {} tenants", tenants.len());

        let yesterday = Utc::now() - Duration::days(1);
        let start_of_day = yesterday.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end_of_day = yesterday.date_naive().and_hms_opt(23, 59, 59).unwrap();

        let start_datetime = DateTime::from_naive_utc_and_offset(start_of_day, Utc);
        let end_datetime = DateTime::from_naive_utc_and_offset(end_of_day, Utc);

        for tenant_id in tenants {
            match self
                .aggregate_tenant(&tenant_id, start_datetime, end_datetime)
                .await
            {
                Ok(_) => info!("Aggregated analytics for tenant: {}", tenant_id),
                Err(e) => warn!(
                    "Failed to aggregate analytics for tenant {}: {}",
                    tenant_id, e
                ),
            }
        }

        info!("Daily analytics aggregation job completed");
        Ok(())
    }

    /// Aggregate analytics for a single tenant for a specific date
    async fn aggregate_tenant(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<()> {
        let summary_date = start_date.date_naive();

        // Calculate total events
        let total_events: i64 = sqlx::query_scalar(
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
        .context("Failed to count events")?;

        // Calculate unique users
        let unique_users: i64 = sqlx::query_scalar(
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
        .context("Failed to count unique users")?;

        // Calculate top features
        let top_features_json: serde_json::Value = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                jsonb_agg(jsonb_build_object(
                    'feature_name', event_type,
                    'usage_count', count,
                    'unique_users', unique_users,
                    'avg_duration_ms', avg_duration_ms,
                    'last_used', last_used
                )),
                '[]'::jsonb
            )
            FROM (
                SELECT
                    event_type,
                    COUNT(*) as count,
                    COUNT(DISTINCT user_id) as unique_users,
                    AVG((event_data->>'duration_ms')::float) as avg_duration_ms,
                    MAX(created_at) as last_used
                FROM analytics_events
                WHERE tenant_id = $1
                  AND created_at >= $2
                  AND created_at <= $3
                GROUP BY event_type
                ORDER BY count DESC
                LIMIT 10
            ) features
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to calculate top features")?;

        // Calculate performance metrics
        let performance_metrics_json: serde_json::Value = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                jsonb_agg(jsonb_build_object(
                    'endpoint', event_type,
                    'avg_response_time_ms', avg_time,
                    'p95_response_time_ms', p95_time,
                    'p99_response_time_ms', p99_time,
                    'request_count', count,
                    'error_count', errors,
                    'error_rate', error_rate
                )),
                '[]'::jsonb
            )
            FROM (
                SELECT
                    event_type,
                    AVG((event_data->>'response_time_ms')::float) as avg_time,
                    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (event_data->>'response_time_ms')::float) as p95_time,
                    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY (event_data->>'response_time_ms')::float) as p99_time,
                    COUNT(*) as count,
                    SUM(CASE WHEN (event_data->>'status_code')::int >= 400 THEN 1 ELSE 0 END) as errors,
                    AVG(CASE WHEN (event_data->>'status_code')::int >= 400 THEN 1.0 ELSE 0.0 END) as error_rate
                FROM analytics_events
                WHERE tenant_id = $1
                  AND event_category = 'api_request'
                  AND created_at >= $2
                  AND created_at <= $3
                GROUP BY event_type
                ORDER BY count DESC
            ) metrics
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .context("Failed to calculate performance metrics")?;

        // Insert or update daily summary
        sqlx::query(
            r#"
            INSERT INTO analytics_daily_summaries (
                tenant_id,
                summary_date,
                total_events,
                unique_users,
                top_features,
                performance_metrics
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, summary_date)
            DO UPDATE SET
                total_events = EXCLUDED.total_events,
                unique_users = EXCLUDED.unique_users,
                top_features = EXCLUDED.top_features,
                performance_metrics = EXCLUDED.performance_metrics,
                updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(summary_date)
        .bind(total_events)
        .bind(unique_users)
        .bind(top_features_json)
        .bind(performance_metrics_json)
        .execute(&self.pool)
        .await
        .context("Failed to upsert daily summary")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daily_aggregation_job_creation() {
        // This test would require a database connection
        // For now, just verify the struct can be created
        let pool = PgPool::connect_lazy("postgresql://localhost/test").unwrap();
        let _job = DailyAggregationJob::new(pool);
    }
}
