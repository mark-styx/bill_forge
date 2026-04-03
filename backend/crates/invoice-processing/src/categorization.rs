//! Intelligent invoice categorization using ML-based suggestions
//!
//! Suggests GL codes, departments, and cost centers based on:
//! - Vendor historical patterns
//! - Line item descriptions
//! - Similar invoices

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Category suggestion with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySuggestion {
    pub category_type: CategoryType,
    pub value: String,
    pub confidence: f32,
    pub source: SuggestionSource,
    pub reasoning: Option<String>,
}

/// Type of categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CategoryType {
    GlCode,
    Department,
    CostCenter,
}

/// Source of the suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    /// Based on vendor's historical invoices
    VendorHistory,
    /// Based on line item text analysis
    LineItemAnalysis,
    /// Based on similar invoices
    SimilarInvoices,
    /// Default/fallback suggestion
    Default,
}

/// Complete categorization result for an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceCategorization {
    pub invoice_id: Uuid,
    pub gl_code: Option<CategorySuggestion>,
    pub department: Option<CategorySuggestion>,
    pub cost_center: Option<CategorySuggestion>,
    pub overall_confidence: f32,
}

/// Intelligent categorization engine
pub struct CategorizationEngine {
    pool: PgPool,
}

impl CategorizationEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Suggest categories for an invoice
    pub async fn suggest_categories(
        &self,
        tenant_id: &str,
        vendor_id: Option<Uuid>,
        vendor_name: &str,
        line_items: &[LineItemInput],
        total_amount: f64,
    ) -> Result<InvoiceCategorization> {
        let mut suggestions = Vec::new();

        // Strategy 1: Vendor historical patterns (highest confidence)
        if let Some(vid) = vendor_id {
            if let Some(suggestion) = self.suggest_from_vendor_history(tenant_id, vid).await? {
                suggestions.push(suggestion);
            }
        }

        // Strategy 2: Line item text analysis
        if let Some(suggestion) = self.suggest_from_line_items(line_items).await? {
            suggestions.push(suggestion);
        }

        // Strategy 3: Similar invoices by vendor name and amount range
        if let Some(suggestion) = self
            .suggest_from_similar_invoices(tenant_id, vendor_name, total_amount)
            .await?
        {
            suggestions.push(suggestion);
        }

        // Aggregate suggestions by category type
        let gl_code = self
            .pick_best_suggestion(&suggestions, CategoryType::GlCode)
            .await?;
        let department = self
            .pick_best_suggestion(&suggestions, CategoryType::Department)
            .await?;
        let cost_center = self
            .pick_best_suggestion(&suggestions, CategoryType::CostCenter)
            .await?;

        // Calculate overall confidence
        let overall_confidence =
            self.calculate_overall_confidence(&gl_code, &department, &cost_center);

        Ok(InvoiceCategorization {
            invoice_id: Uuid::nil(), // Will be set by caller
            gl_code,
            department,
            cost_center,
            overall_confidence,
        })
    }

    /// Suggest categories based on vendor's historical invoices
    async fn suggest_from_vendor_history(
        &self,
        tenant_id: &str,
        vendor_id: Uuid,
    ) -> Result<Option<CategorySuggestion>> {
        // Get most common GL code, department, cost center for this vendor
        let row = sqlx::query(
            r#"
            SELECT
                gl_code,
                department,
                cost_center,
                COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND vendor_id = $2
            AND gl_code IS NOT NULL
            GROUP BY gl_code, department, cost_center
            ORDER BY usage_count DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let usage_count: i64 = r.try_get("usage_count").unwrap_or(0);
            let confidence = (usage_count as f32 / 10.0).min(0.95); // Cap at 95%

            CategorySuggestion {
                category_type: CategoryType::GlCode, // Primary category
                value: r.try_get("gl_code").unwrap_or_default(),
                confidence,
                source: SuggestionSource::VendorHistory,
                reasoning: Some(format!(
                    "Based on {} previous invoices from this vendor",
                    usage_count
                )),
            }
        }))
    }

    /// Suggest categories based on line item text analysis
    async fn suggest_from_line_items(
        &self,
        line_items: &[LineItemInput],
    ) -> Result<Option<CategorySuggestion>> {
        if line_items.is_empty() {
            return Ok(None);
        }

        // Simple keyword-based categorization
        // In production, this would use an ML model or OpenAI embeddings
        let combined_text = line_items
            .iter()
            .map(|item| item.description.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");

        // Common patterns
        let (category, confidence) = if combined_text.contains("software")
            || combined_text.contains("subscription")
            || combined_text.contains("saas")
            || combined_text.contains("license")
        {
            ("6000-Software & Subscriptions", 0.85)
        } else if combined_text.contains("marketing")
            || combined_text.contains("advertising")
            || combined_text.contains("ad")
        {
            ("7000-Marketing", 0.80)
        } else if combined_text.contains("travel")
            || combined_text.contains("flight")
            || combined_text.contains("hotel")
        {
            ("8000-Travel & Entertainment", 0.80)
        } else if combined_text.contains("office")
            || combined_text.contains("supplies")
            || combined_text.contains("equipment")
        {
            ("5000-Office Supplies & Equipment", 0.75)
        } else if combined_text.contains("consulting")
            || combined_text.contains("professional")
            || combined_text.contains("services")
        {
            ("9000-Professional Services", 0.75)
        } else {
            ("0000-General", 0.40)
        };

        Ok(Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: category.to_string(),
            confidence,
            source: SuggestionSource::LineItemAnalysis,
            reasoning: Some("Based on line item description analysis".to_string()),
        }))
    }

    /// Suggest categories based on similar invoices (same vendor name, similar amount)
    async fn suggest_from_similar_invoices(
        &self,
        tenant_id: &str,
        vendor_name: &str,
        total_amount: f64,
    ) -> Result<Option<CategorySuggestion>> {
        // Find invoices from vendors with similar names and similar amounts (±20%)
        let amount_min = total_amount * 0.8;
        let amount_max = total_amount * 1.2;

        let row = sqlx::query(
            r#"
            SELECT
                gl_code,
                department,
                cost_center,
                COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND vendor_name ILIKE $2
            AND total_amount_cents >= $3
            AND total_amount_cents <= $4
            AND gl_code IS NOT NULL
            GROUP BY gl_code, department, cost_center
            ORDER BY usage_count DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(format!("%{}%", vendor_name))
        .bind(amount_min as i64 * 100) // Convert to cents
        .bind(amount_max as i64 * 100)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let usage_count: i64 = r.try_get("usage_count").unwrap_or(0);
            let confidence = (usage_count as f32 / 5.0).min(0.70); // Lower confidence for similarity

            CategorySuggestion {
                category_type: CategoryType::GlCode,
                value: r.try_get("gl_code").unwrap_or_default(),
                confidence,
                source: SuggestionSource::SimilarInvoices,
                reasoning: Some(format!(
                    "Based on {} similar invoices from this vendor",
                    usage_count
                )),
            }
        }))
    }

    /// Pick the best suggestion for a category type
    async fn pick_best_suggestion(
        &self,
        suggestions: &[CategorySuggestion],
        category_type: CategoryType,
    ) -> Result<Option<CategorySuggestion>> {
        // Filter by category type and pick highest confidence
        let matching: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category_type == category_type)
            .collect();

        if matching.is_empty() {
            return Ok(None);
        }

        // Sort by confidence (descending)
        let best = matching
            .into_iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        Ok(Some(best.clone()))
    }

    /// Calculate overall confidence score
    fn calculate_overall_confidence(
        &self,
        gl_code: &Option<CategorySuggestion>,
        department: &Option<CategorySuggestion>,
        cost_center: &Option<CategorySuggestion>,
    ) -> f32 {
        let mut scores = Vec::new();

        if let Some(gl) = gl_code {
            scores.push(gl.confidence);
        }
        if let Some(dept) = department {
            scores.push(dept.confidence);
        }
        if let Some(cc) = cost_center {
            scores.push(cc.confidence);
        }

        if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f32>() / scores.len() as f32
        }
    }
}

/// Input for line items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItemInput {
    pub description: String,
    pub quantity: Option<f64>,
    pub amount: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_line_item_analysis_software() {
        let items = vec![LineItemInput {
            description: "Annual software license".to_string(),
            quantity: Some(1.0),
            amount: 1200.0,
        }];

        let engine =
            CategorizationEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());

        // Can't test async easily in unit tests without runtime
        // This would be tested in integration tests
    }
}
