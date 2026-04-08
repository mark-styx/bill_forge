//! Feedback Learning for Categorization
//!
//! Tracks user corrections and learns from them to improve suggestions.
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::categorization::CategoryType;

/// Feedback learning system
pub struct FeedbackLearning {
    pool: PgPool,
}

/// User feedback on categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizationFeedback {
    pub tenant_id: String,
    pub invoice_id: Uuid,
    pub vendor_id: Option<Uuid>,
    pub vendor_name: String,

    // What was suggested
    pub suggested_gl_code: Option<String>,
    pub suggested_department: Option<String>,
    pub suggested_cost_center: Option<String>,
    pub suggestion_confidence: Option<f32>,
    pub suggestion_source: Option<String>,

    // What user chose
    pub accepted_gl_code: Option<String>,
    pub accepted_department: Option<String>,
    pub accepted_cost_center: Option<String>,

    // Context
    pub line_items_summary: String,
    pub total_amount_cents: i64,

    // Feedback type
    pub feedback_type: FeedbackType,
}

/// Type of feedback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeedbackType {
    Acceptance,  // User accepted the suggestion
    Correction,  // User changed the suggestion
    Rejection,   // User rejected without choosing alternative
}

/// Learning insights from feedback
#[derive(Debug, Clone, Serialize)]
pub struct LearningInsights {
    pub vendor_id: Option<Uuid>,
    pub category_adjustments: Vec<CategoryAdjustment>,
    pub confidence_calibration: ConfidenceCalibration,
}

/// Category adjustment based on feedback
#[derive(Debug, Clone, Serialize)]
pub struct CategoryAdjustment {
    pub category_type: CategoryType,
    pub suggested_value: String,
    pub correct_value: String,
    pub frequency: i32,
}

/// Confidence calibration data
#[derive(Debug, Clone, Serialize)]
pub struct ConfidenceCalibration {
    pub avg_confidence_when_correct: f32,
    pub avg_confidence_when_wrong: f32,
    pub total_samples: i32,
}

/// A stored correction rule derived from repeated user corrections
#[derive(Debug, Clone, Serialize)]
pub struct CorrectionRule {
    pub category_type: CategoryType,
    pub suggested_value: String,
    pub correct_value: String,
    pub frequency: i32,
}

impl FeedbackLearning {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Record user feedback on a categorization suggestion
    pub async fn record_feedback(&self, feedback: CategorizationFeedback) -> Result<()> {
        let feedback_type_str = match feedback.feedback_type {
            FeedbackType::Acceptance => "acceptance",
            FeedbackType::Correction => "correction",
            FeedbackType::Rejection => "rejection",
        };

        // Determine which suggestions were accepted
        let accepted_gl = feedback.suggested_gl_code == feedback.accepted_gl_code;
        let accepted_dept = feedback.suggested_department == feedback.accepted_department;
        let accepted_cc = feedback.suggested_cost_center == feedback.accepted_cost_center;

        sqlx::query(
            r#"
            INSERT INTO categorization_feedback (
                tenant_id, invoice_id, vendor_id, vendor_name,
                suggested_gl_code, suggested_department, suggested_cost_center,
                suggestion_confidence, suggestion_source,
                accepted_gl_code, accepted_department, accepted_cost_center,
                line_items_summary, total_amount_cents, feedback_type
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
        )
        .bind(&feedback.tenant_id)
        .bind(feedback.invoice_id)
        .bind(feedback.vendor_id)
        .bind(&feedback.vendor_name)
        .bind(&feedback.suggested_gl_code)
        .bind(&feedback.suggested_department)
        .bind(&feedback.suggested_cost_center)
        .bind(feedback.suggestion_confidence)
        .bind(&feedback.suggestion_source)
        .bind(&feedback.accepted_gl_code)
        .bind(&feedback.accepted_department)
        .bind(&feedback.accepted_cost_center)
        .bind(&feedback.line_items_summary)
        .bind(feedback.total_amount_cents)
        .bind(feedback_type_str)
        .execute(&self.pool)
        .await
        .context("Failed to record categorization feedback")?;

        Ok(())
    }

    /// Learn from recent feedback and extract insights
    pub async fn analyze_feedback(&self, tenant_id: &str, days: i32) -> Result<LearningInsights> {
        // Get recent feedback for analysis
        let since = Utc::now() - Duration::days(days as i64);

        // Analyze category adjustments
        let adjustments = self.analyze_category_adjustments(tenant_id, since).await?;

        // Analyze confidence calibration
        let calibration = self.analyze_confidence_calibration(tenant_id, since).await?;

        Ok(LearningInsights {
            vendor_id: None, // Would be filled for vendor-specific analysis
            category_adjustments: adjustments,
            confidence_calibration: calibration,
        })
    }

    /// Analyze patterns in category corrections
    async fn analyze_category_adjustments(
        &self,
        tenant_id: &str,
        since: chrono::DateTime<Utc>,
    ) -> Result<Vec<CategoryAdjustment>> {
        let rows = sqlx::query_as::<_, (String, String, String, i32)>(
            r#"
            SELECT
                'gl_code' as category_type,
                suggested_gl_code as suggested,
                accepted_gl_code as correct,
                COUNT(*) as frequency
            FROM categorization_feedback
            WHERE tenant_id = $1
            AND created_at >= $2
            AND feedback_type = 'correction'
            AND suggested_gl_code IS NOT NULL
            AND suggested_gl_code != accepted_gl_code
            GROUP BY suggested_gl_code, accepted_gl_code

            UNION ALL

            SELECT
                'department' as category_type,
                suggested_department as suggested,
                accepted_department as correct,
                COUNT(*) as frequency
            FROM categorization_feedback
            WHERE tenant_id = $1
            AND created_at >= $2
            AND feedback_type = 'correction'
            AND suggested_department IS NOT NULL
            AND suggested_department != accepted_department
            GROUP BY suggested_department, accepted_department

            ORDER BY frequency DESC
            LIMIT 50
            "#,
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .context("Failed to analyze category adjustments")?;

        Ok(rows
            .into_iter()
            .map(|(cat_type, suggested, correct, freq)| {
                let category_type = match cat_type.as_str() {
                    "gl_code" => CategoryType::GlCode,
                    "department" => CategoryType::Department,
                    "cost_center" => CategoryType::CostCenter,
                    _ => CategoryType::GlCode,
                };

                CategoryAdjustment {
                    category_type,
                    suggested_value: suggested,
                    correct_value: correct,
                    frequency: freq,
                }
            })
            .collect())
    }

    /// Analyze confidence calibration (how well confidence predicts accuracy)
    async fn analyze_confidence_calibration(
        &self,
        tenant_id: &str,
        since: chrono::DateTime<Utc>,
    ) -> Result<ConfidenceCalibration> {
        let row = sqlx::query_as::<_, (Option<f32>, Option<f32>, Option<i64>)>(
            r#"
            SELECT
                AVG(CASE WHEN accepted_gl_code THEN suggestion_confidence ELSE NULL END) as avg_conf_correct,
                AVG(CASE WHEN NOT accepted_gl_code THEN suggestion_confidence ELSE NULL END) as avg_conf_wrong,
                COUNT(*) as total_samples
            FROM categorization_feedback
            WHERE tenant_id = $1
            AND created_at >= $2
            AND suggestion_confidence IS NOT NULL
            "#,
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .context("Failed to analyze confidence calibration")?;

        Ok(ConfidenceCalibration {
            avg_confidence_when_correct: row.0.unwrap_or(0.0),
            avg_confidence_when_wrong: row.1.unwrap_or(0.0),
            total_samples: row.2.unwrap_or(0) as i32,
        })
    }

    /// Get vendor-specific learning insights
    pub async fn get_vendor_insights(&self, tenant_id: &str, vendor_id: Uuid) -> Result<LearningInsights> {
        let since = Utc::now() - Duration::days(30);

        let adjustments = sqlx::query_as::<_, (String, String, String, i32)>(
            r#"
            SELECT
                'gl_code' as category_type,
                suggested_gl_code as suggested,
                accepted_gl_code as correct,
                COUNT(*) as frequency
            FROM categorization_feedback
            WHERE tenant_id = $1
            AND vendor_id = $2
            AND created_at >= $3
            AND feedback_type = 'correction'
            GROUP BY suggested_gl_code, accepted_gl_code
            ORDER BY frequency DESC
            LIMIT 10
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get vendor-specific adjustments")?;

        let calibration = self.analyze_confidence_calibration(tenant_id, since).await?;

        Ok(LearningInsights {
            vendor_id: Some(vendor_id),
            category_adjustments: adjustments
                .into_iter()
                .map(|(cat_type, suggested, correct, freq)| CategoryAdjustment {
                    category_type: CategoryType::GlCode,
                    suggested_value: suggested,
                    correct_value: correct,
                    frequency: freq,
                })
                .collect(),
            confidence_calibration: calibration,
        })
    }

    /// Update metrics for ML model performance tracking
    pub async fn update_daily_metrics(&self, tenant_id: &str) -> Result<()> {
        let today = Utc::now().date_naive();

        sqlx::query(
            r#"
            INSERT INTO categorization_ml_metrics (
                tenant_id,
                metric_date,
                total_suggestions,
                accepted_suggestions,
                corrected_suggestions,
                rejected_suggestions,
                avg_confidence_accepted,
                avg_confidence_rejected
            )
            SELECT
                $1 as tenant_id,
                $2::date as metric_date,
                COUNT(*) as total_suggestions,
                SUM(CASE WHEN feedback_type = 'acceptance' THEN 1 ELSE 0 END) as accepted_suggestions,
                SUM(CASE WHEN feedback_type = 'correction' THEN 1 ELSE 0 END) as corrected_suggestions,
                SUM(CASE WHEN feedback_type = 'rejection' THEN 1 ELSE 0 END) as rejected_suggestions,
                AVG(CASE WHEN feedback_type = 'acceptance' THEN suggestion_confidence ELSE NULL END) as avg_confidence_accepted,
                AVG(CASE WHEN feedback_type IN ('correction', 'rejection') THEN suggestion_confidence ELSE NULL END) as avg_confidence_rejected
            FROM categorization_feedback
            WHERE tenant_id = $1
            AND DATE(created_at) = $2::date
            ON CONFLICT (tenant_id, metric_date)
            DO UPDATE SET
                total_suggestions = EXCLUDED.total_suggestions,
                accepted_suggestions = EXCLUDED.accepted_suggestions,
                corrected_suggestions = EXCLUDED.corrected_suggestions,
                rejected_suggestions = EXCLUDED.rejected_suggestions,
                avg_confidence_accepted = EXCLUDED.avg_confidence_accepted,
                avg_confidence_rejected = EXCLUDED.avg_confidence_rejected
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .execute(&self.pool)
        .await
        .context("Failed to update daily ML metrics")?;

        Ok(())
    }

    /// Get accuracy metrics for a tenant
    pub async fn get_accuracy_metrics(&self, tenant_id: &str, days: i32) -> Result<AccuracyMetrics> {
        let since = Utc::now() - Duration::days(days as i64);

        let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
            r#"
            SELECT
                COALESCE(SUM(total_suggestions), 0) as total,
                COALESCE(SUM(accepted_suggestions), 0) as accepted,
                COALESCE(SUM(corrected_suggestions), 0) as corrected,
                COALESCE(SUM(rejected_suggestions), 0) as rejected
            FROM categorization_ml_metrics
            WHERE tenant_id = $1
            AND metric_date >= $2::date
            "#,
        )
        .bind(tenant_id)
        .bind(since.date_naive())
        .fetch_one(&self.pool)
        .await
        .context("Failed to get accuracy metrics")?;

        Ok(AccuracyMetrics {
            total_suggestions: row.0,
            accepted_suggestions: row.1,
            corrected_suggestions: row.2,
            rejected_suggestions: row.3,
        })
    }

    // ------------------------------------------------------------------
    // Methods that APPLY feedback insights back into the ML model
    // ------------------------------------------------------------------

    /// Upsert correction rules from analyzed category adjustments.
    ///
    /// Only adjustments with frequency >= `min_frequency` are stored as active
    /// rules. Rules that no longer appear in the current training window (or
    /// drop below `min_frequency`) are deactivated so they stop influencing
    /// future suggestions.  Returns the number of active rules written.
    pub async fn apply_category_corrections(
        &self,
        tenant_id: &str,
        adjustments: &[CategoryAdjustment],
        min_frequency: i32,
    ) -> Result<usize> {
        // 1. Deactivate all existing rules for this tenant. Rules that are
        //    still valid will be re-activated in the upsert loop below.
        sqlx::query(
            r#"
            UPDATE category_correction_rules
            SET active = false, updated_at = NOW()
            WHERE tenant_id = $1 AND active = true
            "#,
        )
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .context("Failed to deactivate stale correction rules")?;

        // 2. Upsert rules that meet the frequency threshold
        let mut applied = 0usize;

        for adj in adjustments {
            if adj.frequency < min_frequency {
                continue;
            }

            let category_type_str = match adj.category_type {
                CategoryType::GlCode => "gl_code",
                CategoryType::Department => "department",
                CategoryType::CostCenter => "cost_center",
            };

            sqlx::query(
                r#"
                INSERT INTO category_correction_rules
                    (tenant_id, category_type, suggested_value, correct_value, frequency, active)
                VALUES ($1, $2, $3, $4, $5, true)
                ON CONFLICT (tenant_id, category_type, suggested_value, correct_value)
                DO UPDATE SET
                    frequency = EXCLUDED.frequency,
                    active = true,
                    updated_at = NOW()
                "#,
            )
            .bind(tenant_id)
            .bind(category_type_str)
            .bind(&adj.suggested_value)
            .bind(&adj.correct_value)
            .bind(adj.frequency)
            .execute(&self.pool)
            .await
            .context("Failed to upsert correction rule")?;

            applied += 1;
        }

        Ok(applied)
    }

    /// Persist confidence calibration so the ML model can adjust scores.
    ///
    /// The `calibration_offset` is computed as:
    /// `avg_confidence_when_correct - avg_confidence_when_wrong`.
    /// A large positive offset means the model's confidence is well-separated;
    /// a near-zero offset means it can't distinguish correct from incorrect,
    /// and raw scores should be damped.
    pub async fn apply_confidence_calibration(
        &self,
        tenant_id: &str,
        calibration: &ConfidenceCalibration,
    ) -> Result<()> {
        if calibration.total_samples == 0 {
            // No recent feedback: reset the stored calibration so stale
            // offsets from a previous window don't keep damping scores.
            sqlx::query(
                "DELETE FROM categorization_confidence_calibration WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .context("Failed to clear stale calibration")?;
            return Ok(());
        }

        let offset =
            calibration.avg_confidence_when_correct - calibration.avg_confidence_when_wrong;

        sqlx::query(
            r#"
            INSERT INTO categorization_confidence_calibration
                (tenant_id, avg_confidence_when_correct, avg_confidence_when_wrong,
                 total_samples, calibration_offset)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id)
            DO UPDATE SET
                avg_confidence_when_correct = EXCLUDED.avg_confidence_when_correct,
                avg_confidence_when_wrong = EXCLUDED.avg_confidence_when_wrong,
                total_samples = EXCLUDED.total_samples,
                calibration_offset = EXCLUDED.calibration_offset,
                updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(calibration.avg_confidence_when_correct)
        .bind(calibration.avg_confidence_when_wrong)
        .bind(calibration.total_samples)
        .bind(offset)
        .execute(&self.pool)
        .await
        .context("Failed to persist confidence calibration")?;

        Ok(())
    }

    /// Boost `usage_count` in `category_embeddings` for values that users
    /// actually choose, making them rank higher in similarity searches.
    pub async fn boost_category_usage(
        &self,
        tenant_id: &str,
        adjustments: &[CategoryAdjustment],
    ) -> Result<usize> {
        let mut boosted = 0usize;

        for adj in adjustments {
            let category_type_str = match adj.category_type {
                CategoryType::GlCode => "gl_code",
                CategoryType::Department => "department",
                CategoryType::CostCenter => "cost_center",
            };

            let rows_affected = sqlx::query(
                r#"
                UPDATE category_embeddings
                SET usage_count = usage_count + $1,
                    updated_at = NOW()
                WHERE tenant_id = $2
                AND category_type = $3
                AND category_value = $4
                "#,
            )
            .bind(adj.frequency)
            .bind(tenant_id)
            .bind(category_type_str)
            .bind(&adj.correct_value)
            .execute(&self.pool)
            .await
            .context("Failed to boost category usage count")?
            .rows_affected();

            if rows_affected > 0 {
                boosted += 1;
            }
        }

        Ok(boosted)
    }

    /// Retrieve the best active correction rule for a given suggestion.
    ///
    /// Returns the `correct_value` from the highest-frequency active rule,
    /// or `None` if no rule applies.
    pub async fn get_correction_for(
        &self,
        tenant_id: &str,
        category_type: &str,
        suggested_value: &str,
    ) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT correct_value
            FROM category_correction_rules
            WHERE tenant_id = $1
            AND category_type = $2
            AND suggested_value = $3
            AND active = true
            ORDER BY frequency DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(category_type)
        .bind(suggested_value)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to look up correction rule")?;

        Ok(row.map(|(v,)| v))
    }

    /// Load all active correction rules for a tenant (used by the
    /// suggestion pipeline to batch-check in memory).
    pub async fn get_active_correction_rules(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<CorrectionRule>> {
        let rows = sqlx::query_as::<_, (String, String, String, i32)>(
            r#"
            SELECT category_type, suggested_value, correct_value, frequency
            FROM category_correction_rules
            WHERE tenant_id = $1
            AND active = true
            ORDER BY frequency DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load active correction rules")?;

        Ok(rows
            .into_iter()
            .map(|(cat_type, suggested, correct, freq)| {
                let category_type = match cat_type.as_str() {
                    "gl_code" => CategoryType::GlCode,
                    "department" => CategoryType::Department,
                    "cost_center" => CategoryType::CostCenter,
                    _ => CategoryType::GlCode,
                };
                CorrectionRule {
                    category_type,
                    suggested_value: suggested,
                    correct_value: correct,
                    frequency: freq,
                }
            })
            .collect())
    }

    /// Load the stored confidence calibration offset for a tenant.
    /// Returns 0.0 if no calibration data exists.
    pub async fn get_calibration_offset(&self, tenant_id: &str) -> Result<f32> {
        let row = sqlx::query_as::<_, (f32,)>(
            r#"
            SELECT calibration_offset
            FROM categorization_confidence_calibration
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load calibration offset")?;

        Ok(row.map(|(v,)| v).unwrap_or(0.0))
    }
}

/// Accuracy metrics for categorization
#[derive(Debug, Clone, Serialize)]
pub struct AccuracyMetrics {
    pub total_suggestions: i64,
    pub accepted_suggestions: i64,
    pub corrected_suggestions: i64,
    pub rejected_suggestions: i64,
}

impl AccuracyMetrics {
    pub fn accuracy_rate(&self) -> f32 {
        if self.total_suggestions == 0 {
            return 0.0;
        }
        self.accepted_suggestions as f32 / self.total_suggestions as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accuracy_rate() {
        let metrics = AccuracyMetrics {
            total_suggestions: 100,
            accepted_suggestions: 85,
            corrected_suggestions: 10,
            rejected_suggestions: 5,
        };

        assert_eq!(metrics.accuracy_rate(), 0.85);
    }

    #[test]
    fn test_accuracy_rate_zero_division() {
        let metrics = AccuracyMetrics {
            total_suggestions: 0,
            accepted_suggestions: 0,
            corrected_suggestions: 0,
            rejected_suggestions: 0,
        };

        assert_eq!(metrics.accuracy_rate(), 0.0);
    }
}
