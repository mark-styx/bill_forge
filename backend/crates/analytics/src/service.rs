//! Analytics Service
//!
//! Business logic for analytics aggregation and reporting.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::*;
use crate::repository::AnalyticsRepository;

pub struct AnalyticsService {
    repo: AnalyticsRepository,
}

impl AnalyticsService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: AnalyticsRepository::new(pool),
        }
    }

    /// Record an analytics event
    pub async fn track_event(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        request: CreateEventRequest,
    ) -> Result<AnalyticsEvent> {
        self.repo
            .record_event(
                tenant_id,
                user_id,
                &request.event_type,
                &request.event_category,
                request.event_data,
            )
            .await
    }

    /// Get usage summary for a time period
    pub async fn get_usage_summary(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<UsageSummary> {
        let period = format!(
            "{} to {}",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        let total_events = self
            .repo
            .get_event_count(tenant_id, start_date, end_date)
            .await?;
        let unique_users = self
            .repo
            .get_unique_user_count(tenant_id, start_date, end_date)
            .await?;
        let top_features = self
            .repo
            .get_feature_usage(tenant_id, start_date, end_date)
            .await?;
        let performance_metrics = self
            .repo
            .get_performance_metrics(tenant_id, start_date, end_date)
            .await?;

        Ok(UsageSummary {
            period,
            start_date,
            end_date,
            total_events,
            unique_users,
            top_features,
            performance_metrics,
        })
    }

    /// Get daily usage summary (last 24 hours)
    pub async fn get_daily_summary(&self, tenant_id: &str) -> Result<UsageSummary> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::hours(24);
        self.get_usage_summary(tenant_id, start_date, end_date)
            .await
    }

    /// Get weekly usage summary (last 7 days)
    pub async fn get_weekly_summary(&self, tenant_id: &str) -> Result<UsageSummary> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(7);
        self.get_usage_summary(tenant_id, start_date, end_date)
            .await
    }

    /// Get monthly usage summary (last 30 days)
    pub async fn get_monthly_summary(&self, tenant_id: &str) -> Result<UsageSummary> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(30);
        self.get_usage_summary(tenant_id, start_date, end_date)
            .await
    }

    /// Get trend analysis for key metrics
    pub async fn get_trends(&self, tenant_id: &str) -> Result<Vec<TrendData>> {
        let now = Utc::now();
        let current_start = now - Duration::days(7);
        let previous_start = now - Duration::days(14);
        let previous_end = now - Duration::days(7);

        let metrics = vec![
            "invoice_uploaded",
            "invoice_approved",
            "invoice_rejected",
            "vendor_created",
            "report_generated",
        ];

        let mut trends = Vec::new();
        for metric in metrics {
            if let Ok(trend) = self
                .repo
                .get_trend_data(
                    tenant_id,
                    metric,
                    current_start,
                    now,
                    previous_start,
                    previous_end,
                )
                .await
            {
                trends.push(trend);
            }
        }

        Ok(trends)
    }

    /// Get feature usage metrics
    pub async fn get_feature_usage(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<FeatureUsage>> {
        self.repo
            .get_feature_usage(tenant_id, start_date, end_date)
            .await
    }

    /// Get performance metrics
    pub async fn get_performance_metrics(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<PerformanceMetric>> {
        self.repo
            .get_performance_metrics(tenant_id, start_date, end_date)
            .await
    }
}
