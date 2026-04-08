//! Background job for learning from categorization feedback
//!
//! Analyzes user corrections and updates ML model weights.
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use anyhow::{Context, Result};
use billforge_db::PgManager;
use tracing::{info, warn};
use std::sync::Arc;

use billforge_invoice_processing::feedback_loop::{FeedbackLearning, AccuracyMetrics};

/// Learn from recent feedback for all tenants
pub async fn learn_from_feedback(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting categorization feedback learning job");

    // Get all active tenants from metadata database
    let metadata_pool = pg_manager.metadata();
    let tenants = sqlx::query_as::<_, (String,)>(
        "SELECT id::text FROM tenants WHERE active = true",
    )
    .fetch_all(metadata_pool)
    .await
    .context("Failed to fetch tenants")?;

    info!("Learning from feedback for {} tenants", tenants.len());

    for (tenant_id,) in tenants {
        match process_tenant_feedback(&pg_manager, &tenant_id).await {
            Ok(metrics) => {
                info!(
                    tenant_id = %tenant_id,
                    total = metrics.total_suggestions,
                    accepted = metrics.accepted_suggestions,
                    corrected = metrics.corrected_suggestions,
                    accuracy = %metrics.accuracy_rate(),
                    "Feedback learning completed"
                );
            }
            Err(e) => {
                warn!(tenant_id = %tenant_id, error = %e, "Failed to process feedback for tenant");
            }
        }
    }

    info!("Feedback learning job completed");
    Ok(())
}

/// Process feedback for a single tenant
async fn process_tenant_feedback(pg_manager: &PgManager, tenant_id: &str) -> Result<AccuracyMetrics> {
    let tenant_id: billforge_core::TenantId = tenant_id.parse()
        .context("Invalid tenant ID format")?;
    let pool = pg_manager.tenant(&tenant_id).await?;

    let learning = FeedbackLearning::new((*pool).clone());

    let tenant_id_str = tenant_id.as_str();

    // Analyze feedback from last 7 days
    let insights = learning
        .analyze_feedback(&tenant_id_str, 7)
        .await
        .context("Failed to analyze feedback")?;

    info!(
        tenant_id = %tenant_id,
        adjustments = insights.category_adjustments.len(),
        calibration_samples = insights.confidence_calibration.total_samples,
        "Analyzed categorization feedback"
    );

    // Apply category corrections: upsert rules for patterns with freq >= 3
    let rules_applied = learning
        .apply_category_corrections(&tenant_id_str, &insights.category_adjustments, 3)
        .await
        .context("Failed to apply category corrections")?;

    if rules_applied > 0 {
        info!(
            tenant_id = %tenant_id,
            rules = rules_applied,
            "Applied correction rules from user feedback"
        );
    }

    // Boost usage_count for correct values in category_embeddings
    let boosted = learning
        .boost_category_usage(&tenant_id_str, &insights.category_adjustments)
        .await
        .context("Failed to boost category usage counts")?;

    if boosted > 0 {
        info!(
            tenant_id = %tenant_id,
            boosted = boosted,
            "Boosted embedding usage counts for corrected categories"
        );
    }

    // Persist confidence calibration for the ML model
    learning
        .apply_confidence_calibration(&tenant_id_str, &insights.confidence_calibration)
        .await
        .context("Failed to apply confidence calibration")?;

    // Log significant adjustments
    for adjustment in &insights.category_adjustments {
        if adjustment.frequency >= 5 {
            info!(
                tenant_id = %tenant_id,
                category_type = ?adjustment.category_type,
                suggested = %adjustment.suggested_value,
                correct = %adjustment.correct_value,
                frequency = adjustment.frequency,
                "Frequent correction pattern detected"
            );
        }
    }

    // Update daily metrics
    learning
        .update_daily_metrics(&tenant_id_str)
        .await
        .context("Failed to update daily metrics")?;

    // Get overall accuracy metrics for last 30 days
    let metrics = learning
        .get_accuracy_metrics(&tenant_id_str, 30)
        .await
        .context("Failed to get accuracy metrics")?;

    Ok(metrics)
}

/// Get categorization accuracy report for a tenant
pub async fn get_accuracy_report(pg_manager: &PgManager, tenant_id: &str, days: i32) -> Result<AccuracyReport> {
    let tenant_id: billforge_core::TenantId = tenant_id.parse()
        .context("Invalid tenant ID format")?;
    let pool = pg_manager.tenant(&tenant_id).await?;

    let learning = FeedbackLearning::new((*pool).clone());

    let tenant_id_str = tenant_id.as_str();

    let metrics = learning
        .get_accuracy_metrics(&tenant_id_str, days)
        .await
        .context("Failed to get accuracy metrics")?;

    let insights = learning
        .analyze_feedback(&tenant_id_str, days)
        .await
        .context("Failed to analyze feedback")?;

    Ok(AccuracyReport {
        tenant_id: tenant_id.to_string(),
        metrics,
        insights,
    })
}

/// Accuracy report for a tenant
#[derive(Debug, Clone)]
pub struct AccuracyReport {
    pub tenant_id: String,
    pub metrics: AccuracyMetrics,
    pub insights: billforge_invoice_processing::feedback_loop::LearningInsights,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_feedback_learning_structure() {
        // This would require a database
        // In production, use testcontainers or mock the pool
    }
}
