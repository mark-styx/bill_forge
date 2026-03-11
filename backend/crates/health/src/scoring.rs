//! Health score calculation algorithm

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use anyhow::Result;

use super::models::{HealthClassification, HealthScore};

/// Calculate health score for a tenant
pub async fn calculate_health_score(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<HealthScore> {
    // Calculate individual scores
    let usage_score = calculate_usage_score(pool, tenant_id).await?;
    let feature_adoption_score = calculate_feature_adoption_score(pool, tenant_id).await?;
    let error_rate_score = calculate_error_rate_score(pool, tenant_id).await?;
    let sentiment_score = calculate_sentiment_score(pool, tenant_id).await?;
    let payment_score = calculate_payment_score(pool, tenant_id).await?;

    // Weighted average
    let total_score = (usage_score * 0.30
        + feature_adoption_score * 0.25
        + error_rate_score * 0.20
        + sentiment_score * 0.15
        + payment_score * 0.10) as i32;

    let classification = HealthClassification::from_score(total_score);

    Ok(HealthScore {
        tenant_id: tenant_id.to_string(),
        score: total_score,
        classification,
        usage_score,
        feature_adoption_score,
        error_rate_score,
        sentiment_score,
        payment_score,
        calculated_at: Utc::now(),
    })
}

/// Calculate usage frequency score (0-100)
async fn calculate_usage_score(pool: &PgPool, tenant_id: &str) -> Result<f64> {
    // Check daily/weekly active users
    let active_users: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT user_id) as count
        FROM analytics_events
        WHERE tenant_id = $1
        AND created_at > NOW() - INTERVAL '7 days'
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    let active_users = active_users.unwrap_or(0);

    // Score based on active users (simplified)
    // In production, this would be more sophisticated
    let score = (active_users as f64 * 10.0).min(100.0);

    Ok(score)
}

/// Calculate feature adoption score (0-100)
async fn calculate_feature_adoption_score(pool: &PgPool, tenant_id: &str) -> Result<f64> {
    // Check usage of key features: OCR, approvals, QuickBooks sync
    let features_used: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT event_type) as count
        FROM analytics_events
        WHERE tenant_id = $1
        AND event_type IN ('ocr_process', 'approval_action', 'quickbooks_sync')
        AND created_at > NOW() - INTERVAL '30 days'
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    let features_used = features_used.unwrap_or(0);

    // Score: 3 features * 33.3 = 100
    let score = (features_used as f64 * 33.3).min(100.0);

    Ok(score)
}

/// Calculate error rate score (0-100, higher is better)
async fn calculate_error_rate_score(pool: &PgPool, tenant_id: &str) -> Result<f64> {
    // Check OCR failures, workflow failures
    let error_rate: Option<f64> = sqlx::query_scalar(
        r#"
        SELECT
            CASE
                WHEN COUNT(*) = 0 THEN 0.0
                ELSE SUM(CASE WHEN event_type LIKE '%error%' THEN 1 ELSE 0 END)::FLOAT / COUNT(*)
            END as error_rate
        FROM analytics_events
        WHERE tenant_id = $1
        AND created_at > NOW() - INTERVAL '7 days'
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    let error_rate = error_rate.unwrap_or(0.0);

    // Score: 0% error rate = 100, 100% error rate = 0
    let score = (1.0 - error_rate) * 100.0;

    Ok(score)
}

/// Calculate sentiment score from feedback (0-100)
async fn calculate_sentiment_score(pool: &PgPool, tenant_id: &str) -> Result<f64> {
    // Get average feedback rating
    let avg_rating: Option<f64> = sqlx::query_scalar(
        r#"
        SELECT AVG(rating) as avg
        FROM feedback
        WHERE tenant_id = $1
        AND created_at > NOW() - INTERVAL '30 days'
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    // Convert 1-5 rating to 0-100 score
    let score = match avg_rating {
        Some(rating) => (rating - 1.0) * 25.0, // 1->0, 5->100
        None => 50.0, // Default neutral score if no feedback
    };

    Ok(score)
}

/// Calculate payment score (0-100)
async fn calculate_payment_score(pool: &PgPool, tenant_id: &str) -> Result<f64> {
    // Check subscription status (simplified)
    // In production, this would check payment history, subscription tier, etc.
    let subscription_active: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT is_active
        FROM tenants
        WHERE id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    let subscription_active = subscription_active.unwrap_or(false);

    let score = if subscription_active { 100.0 } else { 0.0 };

    Ok(score)
}
