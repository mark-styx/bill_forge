//! Database repository for health scores

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::models::{HealthClassification, HealthScore};

/// Save health score to database
pub async fn save_health_score(pool: &PgPool, score: &HealthScore) -> Result<()> {
    let classification_str = match score.classification {
        HealthClassification::AtRisk => "at_risk",
        HealthClassification::NeedsAttention => "needs_attention",
        HealthClassification::Healthy => "healthy",
    };

    sqlx::query(
        r#"
        INSERT INTO health_scores (
            tenant_id,
            score,
            classification,
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (tenant_id)
        DO UPDATE SET
            score = EXCLUDED.score,
            classification = EXCLUDED.classification,
            usage_score = EXCLUDED.usage_score,
            feature_adoption_score = EXCLUDED.feature_adoption_score,
            error_rate_score = EXCLUDED.error_rate_score,
            sentiment_score = EXCLUDED.sentiment_score,
            payment_score = EXCLUDED.payment_score,
            calculated_at = EXCLUDED.calculated_at
        "#,
    )
    .bind(&score.tenant_id)
    .bind(score.score)
    .bind(classification_str)
    .bind(score.usage_score)
    .bind(score.feature_adoption_score)
    .bind(score.error_rate_score)
    .bind(score.sentiment_score)
    .bind(score.payment_score)
    .bind(score.calculated_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get latest health score for a tenant
pub async fn get_health_score(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<Option<HealthScore>> {
    let row = sqlx::query_as::<_, (String, i32, String, f64, f64, f64, f64, f64, DateTime<Utc>)>(
        r#"
        SELECT
            tenant_id,
            score,
            classification,
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at
        FROM health_scores
        WHERE tenant_id = $1
        ORDER BY calculated_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(tenant_id, score, classification, usage_score, feature_adoption_score, error_rate_score, sentiment_score, payment_score, calculated_at)| {
        HealthScore {
            tenant_id,
            score,
            classification: match classification.as_str() {
                "at_risk" => HealthClassification::AtRisk,
                "needs_attention" => HealthClassification::NeedsAttention,
                "healthy" => HealthClassification::Healthy,
                _ => HealthClassification::NeedsAttention,
            },
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at,
        }
    }))
}

/// Get all tenant health scores
pub async fn list_health_scores(pool: &PgPool) -> Result<Vec<HealthScore>> {
    let rows = sqlx::query_as::<_, (String, i32, String, f64, f64, f64, f64, f64, DateTime<Utc>)>(
        r#"
        SELECT DISTINCT ON (tenant_id)
            tenant_id,
            score,
            classification,
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at
        FROM health_scores
        ORDER BY tenant_id, calculated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(tenant_id, score, classification, usage_score, feature_adoption_score, error_rate_score, sentiment_score, payment_score, calculated_at)| {
        HealthScore {
            tenant_id,
            score,
            classification: match classification.as_str() {
                "at_risk" => HealthClassification::AtRisk,
                "needs_attention" => HealthClassification::NeedsAttention,
                "healthy" => HealthClassification::Healthy,
                _ => HealthClassification::NeedsAttention,
            },
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at,
        }
    }).collect())
}

/// Get historical health scores for a tenant
pub async fn get_health_score_history(
    pool: &PgPool,
    tenant_id: &str,
    limit: i64,
) -> Result<Vec<HealthScore>> {
    let rows = sqlx::query_as::<_, (String, i32, String, f64, f64, f64, f64, f64, DateTime<Utc>)>(
        r#"
        SELECT
            tenant_id,
            score,
            classification,
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at
        FROM health_scores
        WHERE tenant_id = $1
        ORDER BY calculated_at DESC
        LIMIT $2
        "#,
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(tenant_id, score, classification, usage_score, feature_adoption_score, error_rate_score, sentiment_score, payment_score, calculated_at)| {
        HealthScore {
            tenant_id,
            score,
            classification: match classification.as_str() {
                "at_risk" => HealthClassification::AtRisk,
                "needs_attention" => HealthClassification::NeedsAttention,
                "healthy" => HealthClassification::Healthy,
                _ => HealthClassification::NeedsAttention,
            },
            usage_score,
            feature_adoption_score,
            error_rate_score,
            sentiment_score,
            payment_score,
            calculated_at,
        }
    }).collect())
}
