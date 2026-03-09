//! Feedback Service
//!
//! Business logic for feedback collection and analysis.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::*;
use crate::repository::FeedbackRepository;

pub struct FeedbackService {
    repo: FeedbackRepository,
}

impl FeedbackService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: FeedbackRepository::new(pool),
        }
    }

    /// Submit customer feedback
    pub async fn submit(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        request: SubmitFeedbackRequest,
    ) -> Result<Feedback> {
        // Validate rating
        if request.rating < 1 || request.rating > 5 {
            anyhow::bail!("Rating must be between 1 and 5");
        }

        self.repo
            .submit_feedback(
                tenant_id,
                user_id,
                &request.category,
                request.rating,
                request.comment.as_deref(),
            )
            .await
    }

    /// Get feedback aggregation by category
    pub async fn get_aggregation(
        &self,
        tenant_id: &str,
        category: Option<&str>,
    ) -> Result<Vec<FeedbackAggregation>> {
        self.repo.get_aggregation_by_category(tenant_id, category).await
    }

    /// Get overall feedback statistics
    pub async fn get_overall_stats(&self, tenant_id: &str) -> Result<FeedbackAggregation> {
        let aggregations = self.repo.get_aggregation_by_category(tenant_id, None).await?;

        let total_feedback: i64 = aggregations.iter().map(|a| a.total_feedback).sum();
        let avg_rating: f64 = if !aggregations.is_empty() {
            aggregations.iter().map(|a| a.average_rating * a.total_feedback as f64).sum::<f64>()
                / total_feedback as f64
        } else {
            0.0
        };
        let positive_count: i64 = aggregations.iter().map(|a| a.positive_count).sum();
        let neutral_count: i64 = aggregations.iter().map(|a| a.neutral_count).sum();
        let negative_count: i64 = aggregations.iter().map(|a| a.negative_count).sum();
        let last_feedback_at = aggregations
            .iter()
            .map(|a| a.last_feedback_at)
            .max()
            .unwrap_or_else(|| Utc::now());

        Ok(FeedbackAggregation {
            category: "all".to_string(),
            total_feedback,
            average_rating: avg_rating,
            positive_count,
            neutral_count,
            negative_count,
            last_feedback_at,
        })
    }

    /// Get feedback trend over time
    pub async fn get_trend(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<FeedbackTrend> {
        self.repo.get_feedback_trend(tenant_id, start_date, end_date).await
    }

    /// Get weekly feedback trend
    pub async fn get_weekly_trend(&self, tenant_id: &str) -> Result<FeedbackTrend> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(7);
        self.get_trend(tenant_id, start_date, end_date).await
    }

    /// Get monthly feedback trend
    pub async fn get_monthly_trend(&self, tenant_id: &str) -> Result<FeedbackTrend> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(30);
        self.get_trend(tenant_id, start_date, end_date).await
    }

    /// List feedback with filters
    pub async fn list(
        &self,
        tenant_id: &str,
        query: FeedbackQuery,
        limit: i64,
    ) -> Result<Vec<Feedback>> {
        self.repo
            .list_feedback(
                tenant_id,
                query.category.as_deref(),
                query.min_rating,
                query.max_rating,
                limit,
            )
            .await
    }
}
