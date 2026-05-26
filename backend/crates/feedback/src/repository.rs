//! Feedback Repository
//!
//! Database operations for customer feedback.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{Feedback, FeedbackAggregation, FeedbackTrend, SentimentBreakdown};

pub struct FeedbackRepository {
    pool: PgPool,
}

impl FeedbackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Submit new feedback
    pub async fn submit_feedback(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        category: &str,
        rating: i32,
        comment: Option<&str>,
    ) -> Result<Feedback> {
        let (sentiment, sentiment_score) = if let Some(text) = comment {
            Self::analyze_sentiment(text)
        } else {
            (None, None)
        };

        let feedback = sqlx::query_as::<_, Feedback>(
            r#"
            INSERT INTO feedback (tenant_id, user_id, category, rating, comment, sentiment, sentiment_score)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(category)
        .bind(rating)
        .bind(comment)
        .bind(&sentiment)
        .bind(sentiment_score)
        .fetch_one(&self.pool)
        .await
        .context("Failed to submit feedback")?;

        Ok(feedback)
    }

    /// Get feedback aggregation by category
    pub async fn get_aggregation_by_category(
        &self,
        tenant_id: &str,
        category: Option<&str>,
    ) -> Result<Vec<FeedbackAggregation>> {
        let aggregation = if let Some(cat) = category {
            sqlx::query_as::<_, FeedbackAggregation>(
                r#"
                SELECT
                    category,
                    COUNT(*) AS total_feedback,
                    AVG(rating) AS average_rating,
                    SUM(CASE WHEN sentiment = 'positive' THEN 1 ELSE 0 END) AS positive_count,
                    SUM(CASE WHEN sentiment = 'neutral' OR sentiment IS NULL THEN 1 ELSE 0 END) AS neutral_count,
                    SUM(CASE WHEN sentiment = 'negative' THEN 1 ELSE 0 END) AS negative_count,
                    MAX(created_at) AS last_feedback_at
                FROM feedback
                WHERE tenant_id = $1 AND category = $2
                GROUP BY category
                "#,
            )
            .bind(tenant_id)
            .bind(cat)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, FeedbackAggregation>(
                r#"
                SELECT
                    category,
                    COUNT(*) AS total_feedback,
                    AVG(rating) AS average_rating,
                    SUM(CASE WHEN sentiment = 'positive' THEN 1 ELSE 0 END) AS positive_count,
                    SUM(CASE WHEN sentiment = 'neutral' OR sentiment IS NULL THEN 1 ELSE 0 END) AS neutral_count,
                    SUM(CASE WHEN sentiment = 'negative' THEN 1 ELSE 0 END) AS negative_count,
                    MAX(created_at) AS last_feedback_at
                FROM feedback
                WHERE tenant_id = $1
                GROUP BY category
                ORDER BY total_feedback DESC
                "#,
            )
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
        }
        .context("Failed to get feedback aggregation")?;

        Ok(aggregation)
    }

    /// Get feedback trend over time
    pub async fn get_feedback_trend(
        &self,
        tenant_id: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<FeedbackTrend> {
        let period = format!(
            "{} to {}",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        let stats: (i64, Option<f64>, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*),
                AVG(rating),
                SUM(CASE WHEN sentiment = 'positive' THEN 1 ELSE 0 END),
                SUM(CASE WHEN sentiment = 'neutral' OR sentiment IS NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN sentiment = 'negative' THEN 1 ELSE 0 END)
            FROM feedback
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
        .context("Failed to get feedback trend")?;

        Ok(FeedbackTrend {
            period,
            start_date,
            end_date,
            average_rating: stats.1.unwrap_or(0.0),
            total_feedback: stats.0,
            sentiment_breakdown: SentimentBreakdown {
                positive: stats.2,
                neutral: stats.3,
                negative: stats.4,
            },
        })
    }

    /// List feedback with filters
    pub async fn list_feedback(
        &self,
        tenant_id: &str,
        category: Option<&str>,
        min_rating: Option<i32>,
        max_rating: Option<i32>,
        limit: i64,
    ) -> Result<Vec<Feedback>> {
        let feedback = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT *
            FROM feedback
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR category = $2)
              AND ($3::int IS NULL OR rating >= $3)
              AND ($4::int IS NULL OR rating <= $4)
            ORDER BY created_at DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(category)
        .bind(min_rating)
        .bind(max_rating)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list feedback")?;

        Ok(feedback)
    }

    /// Simple sentiment analysis (rule-based)
    fn analyze_sentiment(text: &str) -> (Option<String>, Option<f64>) {
        let text_lower = text.to_lowercase();

        let positive_words = [
            "great",
            "excellent",
            "amazing",
            "wonderful",
            "fantastic",
            "awesome",
            "good",
            "love",
            "perfect",
            "helpful",
            "fast",
            "easy",
            "intuitive",
            "satisfied",
            "happy",
            "pleased",
            "best",
            "recommend",
        ];

        let negative_words = [
            "bad",
            "terrible",
            "awful",
            "horrible",
            "worst",
            "hate",
            "slow",
            "difficult",
            "confusing",
            "frustrating",
            "disappointed",
            "unhappy",
            "poor",
            "broken",
            "bug",
            "error",
            "issue",
            "problem",
            "fail",
        ];

        let positive_count = positive_words
            .iter()
            .filter(|word| text_lower.contains(*word))
            .count();
        let negative_count = negative_words
            .iter()
            .filter(|word| text_lower.contains(*word))
            .count();

        let total = positive_count + negative_count;
        if total == 0 {
            return (Some("neutral".to_string()), Some(0.5));
        }

        let score = positive_count as f64 / total as f64;
        let sentiment = if score > 0.6 {
            "positive"
        } else if score < 0.4 {
            "negative"
        } else {
            "neutral"
        };

        (Some(sentiment.to_string()), Some(score))
    }
}
