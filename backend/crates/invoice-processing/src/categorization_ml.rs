//! ML-Based Invoice Categorization using OpenAI Embeddings
//!
//! Replaces keyword matching with semantic similarity using embeddings.
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use anyhow::{Context, Result};
use async_openai::{Client, types::{CreateEmbeddingRequest, EmbeddingInput}};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use std::collections::HashMap;

use super::categorization::{CategorySuggestion, CategoryType, SuggestionSource, LineItemInput};

/// ML-based categorizer using OpenAI embeddings
pub struct MLCategorizer {
    pool: PgPool,
    openai_client: Client<async_openai::config::OpenAIConfig>,
}

/// Cached embedding vector
#[derive(Debug, Clone)]
pub struct EmbeddingVector {
    pub vector: Vec<f32>,
    pub metadata: HashMap<String, String>,
}

/// Result of embedding-based similarity search
#[derive(Debug, Clone)]
pub struct SimilarityMatch {
    pub category_type: CategoryType,
    pub value: String,
    pub similarity: f32,
    pub description: Option<String>,
}

impl MLCategorizer {
    pub fn new(pool: PgPool, openai_api_key: String) -> Self {
        let config = async_openai::config::OpenAIConfig::new().with_api_key(openai_api_key);
        let openai_client = Client::with_config(config);

        Self { pool, openai_client }
    }

    /// Generate embedding for text using OpenAI text-embedding-3-small
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = CreateEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: EmbeddingInput::String(text.to_string()),
            ..Default::default()
        };

        let response = self.openai_client
            .embeddings()
            .create(request)
            .await
            .context("Failed to generate OpenAI embedding")?;

        let embedding = response
            .data
            .into_iter()
            .next()
            .context("No embedding returned from OpenAI")?;

        Ok(embedding.embedding)
    }

    /// Generate embedding for invoice (vendor + line items)
    pub async fn generate_invoice_embedding(
        &self,
        vendor_name: &str,
        line_items: &[LineItemInput],
        total_amount: f64,
    ) -> Result<Vec<f32>> {
        // Combine vendor name and line items into semantic text
        let mut text_parts = vec![format!("Vendor: {}", vendor_name)];

        for item in line_items {
            text_parts.push(format!(
                "{} x{} @ ${:.2}",
                item.description,
                item.quantity.unwrap_or(1.0),
                item.amount
            ));
        }

        text_parts.push(format!("Total: ${:.2}", total_amount));

        let combined_text = text_parts.join("\n");
        self.generate_embedding(&combined_text).await
    }

    /// Find similar categories using cosine similarity
    pub async fn find_similar_categories(
        &self,
        tenant_id: &str,
        embedding: &[f32],
        category_type: CategoryType,
        limit: usize,
    ) -> Result<Vec<SimilarityMatch>> {
        // Use pgvector's cosine similarity search
        let category_type_str = match category_type {
            CategoryType::GlCode => "gl_code",
            CategoryType::Department => "department",
            CategoryType::CostCenter => "cost_center",
        };

        let rows = sqlx::query_as::<_, (String, f32, Option<String>, i32)>(
            r#"
            SELECT
                category_value,
                1 - (embedding_vector <=> $1::vector) as similarity,
                description,
                usage_count
            FROM category_embeddings
            WHERE tenant_id = $2
            AND category_type = $3
            ORDER BY embedding_vector <=> $1::vector
            LIMIT $4
            "#,
        )
        .bind(embedding.to_vec())
        .bind(tenant_id)
        .bind(category_type_str)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .context("Failed to search similar categories")?;

        Ok(rows
            .into_iter()
            .map(|(value, similarity, description, usage_count)| SimilarityMatch {
                category_type: category_type.clone(),
                value,
                similarity,
                description,
            })
            .collect())
    }

    /// Find similar vendor embeddings
    pub async fn find_similar_vendors(
        &self,
        tenant_id: &str,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(Uuid, f32, String)>> {
        let rows = sqlx::query_as::<_, (Uuid, f32, String)>(
            r#"
            SELECT
                vendor_id,
                1 - (embedding_vector <=> $1::vector) as similarity,
                vendor_name
            FROM vendor_embeddings
            WHERE tenant_id = $2
            ORDER BY embedding_vector <=> $1::vector
            LIMIT $3
            "#,
        )
        .bind(embedding.to_vec())
        .bind(tenant_id)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .context("Failed to search similar vendors")?;

        Ok(rows)
    }

    /// Suggest categories using embedding similarity (hybrid approach)
    pub async fn suggest_categories_ml(
        &self,
        tenant_id: &str,
        vendor_id: Option<Uuid>,
        vendor_name: &str,
        line_items: &[LineItemInput],
        total_amount: f64,
    ) -> Result<super::categorization::InvoiceCategorization> {
        use super::categorization::InvoiceCategorization;

        let mut suggestions = Vec::new();

        // Generate embedding for invoice
        let invoice_embedding = self
            .generate_invoice_embedding(vendor_name, line_items, total_amount)
            .await?;

        // 1. Check if vendor already has cached embedding
        if let Some(vid) = vendor_id {
            if let Some(cached) = self.get_vendor_embedding(tenant_id, vid).await? {
                // Use cached vendor embedding for faster lookup
                let similar_categories = self
                    .find_similar_categories(tenant_id, &cached, CategoryType::GlCode, 3)
                    .await?;

                for match_item in similar_categories {
                    suggestions.push(CategorySuggestion {
                        category_type: CategoryType::GlCode,
                        value: match_item.value,
                        confidence: (match_item.similarity * 0.95).min(0.95), // Cap at 95%
                        source: SuggestionSource::VendorHistory,
                        reasoning: Some(format!(
                            "Based on similar invoices from this vendor (similarity: {:.2})",
                            match_item.similarity
                        )),
                    });
                }
            }
        }

        // 2. Use invoice embedding to find similar categories across all vendors
        let similar_gl_codes = self
            .find_similar_categories(tenant_id, &invoice_embedding, CategoryType::GlCode, 3)
            .await?;

        for match_item in similar_gl_codes {
            // Adjust confidence based on similarity score and usage
            let confidence = self.calculate_embedding_confidence(
                match_item.similarity,
                match_item.description.as_deref(),
            );

            suggestions.push(CategorySuggestion {
                category_type: CategoryType::GlCode,
                value: match_item.value,
                confidence,
                source: SuggestionSource::LineItemAnalysis, // Actually embedding-based
                reasoning: Some(format!(
                    "Semantic match (similarity: {:.2})",
                    match_item.similarity
                )),
            });
        }

        // 3. Suggest departments and cost centers
        let similar_departments = self
            .find_similar_categories(tenant_id, &invoice_embedding, CategoryType::Department, 2)
            .await?;

        for match_item in similar_departments {
            suggestions.push(CategorySuggestion {
                category_type: CategoryType::Department,
                value: match_item.value,
                confidence: match_item.similarity * 0.85,
                source: SuggestionSource::LineItemAnalysis,
                reasoning: Some(format!(
                    "Department match (similarity: {:.2})",
                    match_item.similarity
                )),
            });
        }

        let similar_cost_centers = self
            .find_similar_categories(tenant_id, &invoice_embedding, CategoryType::CostCenter, 2)
            .await?;

        for match_item in similar_cost_centers {
            suggestions.push(CategorySuggestion {
                category_type: CategoryType::CostCenter,
                value: match_item.value,
                confidence: match_item.similarity * 0.80,
                source: SuggestionSource::LineItemAnalysis,
                reasoning: Some(format!(
                    "Cost center match (similarity: {:.2})",
                    match_item.similarity
                )),
            });
        }

        // 4. Aggregate suggestions - pick best for each category type
        let gl_code = self.pick_best_suggestion(&suggestions, CategoryType::GlCode);
        let department = self.pick_best_suggestion(&suggestions, CategoryType::Department);
        let cost_center = self.pick_best_suggestion(&suggestions, CategoryType::CostCenter);

        // 5. Calculate overall confidence
        let overall_confidence = self.calculate_overall_confidence(&gl_code, &department, &cost_center);

        Ok(InvoiceCategorization {
            invoice_id: Uuid::nil(), // Will be set by caller
            gl_code,
            department,
            cost_center,
            overall_confidence,
        })
    }

    /// Pick the best suggestion for a category type
    fn pick_best_suggestion(
        &self,
        suggestions: &[CategorySuggestion],
        category_type: CategoryType,
    ) -> Option<CategorySuggestion> {
        suggestions
            .iter()
            .filter(|s| s.category_type == category_type)
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Calculate overall confidence score.
    ///
    /// Uses a fixed denominator of 3 so missing fields contribute 0.0 to the
    /// average. This ensures incomplete categorizations (e.g. 1-of-3 fields
    /// at 0.98 confidence) produce a low overall score (0.327) that cannot
    /// reach the 0.95 auto-approval threshold.
    fn calculate_overall_confidence(
        &self,
        gl_code: &Option<CategorySuggestion>,
        department: &Option<CategorySuggestion>,
        cost_center: &Option<CategorySuggestion>,
    ) -> f32 {
        let gl_score = gl_code.as_ref().map_or(0.0, |s| s.confidence);
        let dept_score = department.as_ref().map_or(0.0, |s| s.confidence);
        let cc_score = cost_center.as_ref().map_or(0.0, |s| s.confidence);

        (gl_score + dept_score + cc_score) / 3.0
    }

    /// Get cached vendor embedding
    async fn get_vendor_embedding(&self, tenant_id: &str, vendor_id: Uuid) -> Result<Option<Vec<f32>>> {
        let row = sqlx::query(
            r#"
            SELECT embedding_vector
            FROM vendor_embeddings
            WHERE tenant_id = $1 AND vendor_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch vendor embedding")?;

        Ok(row.map(|r| r.try_get::<Vec<f32>, _>("embedding_vector").unwrap()))
    }

    /// Cache vendor embedding for future lookups
    pub async fn cache_vendor_embedding(
        &self,
        tenant_id: &str,
        vendor_id: Uuid,
        vendor_name: &str,
        embedding: Vec<f32>,
        invoice_summary: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO vendor_embeddings (tenant_id, vendor_id, vendor_name, embedding_vector, last_invoice_summary)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, vendor_id)
            DO UPDATE SET
                embedding_vector = EXCLUDED.embedding_vector,
                last_invoice_summary = EXCLUDED.last_invoice_summary,
                updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(vendor_name)
        .bind(embedding)
        .bind(invoice_summary)
        .execute(&self.pool)
        .await
        .context("Failed to cache vendor embedding")?;

        Ok(())
    }

    /// Calculate confidence score based on similarity and context
    fn calculate_embedding_confidence(&self, similarity: f32, description: Option<&str>) -> f32 {
        // Base confidence from similarity
        let mut confidence = similarity;

        // Boost confidence if category has a description (well-defined)
        if description.is_some() {
            confidence *= 1.1;
        }

        // Apply sigmoid function to normalize between 0.4 and 0.95
        let normalized = 0.4 + (0.55 / (1.0 + (-10.0 * (confidence - 0.5)).exp()));

        normalized.min(0.95)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_calculate_embedding_confidence() {
        let categorizer = MLCategorizer::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            "test-key".to_string(),
        );

        // High similarity with description
        let conf1 = categorizer.calculate_embedding_confidence(0.92, Some("Software subscriptions"));
        assert!(conf1 > 0.85 && conf1 <= 0.95);

        // Medium similarity without description
        let conf2 = categorizer.calculate_embedding_confidence(0.65, None);
        assert!(conf2 > 0.5 && conf2 < 0.9);

        // Low similarity
        let conf3 = categorizer.calculate_embedding_confidence(0.30, None);
        assert!(conf3 < 0.5);
    }

    // ========================================================================
    // Confidence calculation tests: missing fields penalize overall score
    // ========================================================================

    fn make_suggestion(confidence: f32) -> CategorySuggestion {
        CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "6000-Software".to_string(),
            confidence,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }
    }

    #[test]
    fn test_overall_confidence_all_fields_present() {
        let categorizer = MLCategorizer::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            "test-key".to_string(),
        );

        let gl = Some(make_suggestion(0.97));
        let dept = Some(CategorySuggestion {
            category_type: CategoryType::Department,
            value: "Engineering".to_string(),
            confidence: 0.96,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        });
        let cc = Some(CategorySuggestion {
            category_type: CategoryType::CostCenter,
            value: "CC-100".to_string(),
            confidence: 0.95,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        });

        let overall = categorizer.calculate_overall_confidence(&gl, &dept, &cc);
        assert!(
            overall >= 0.95,
            "All fields present at 0.95+ should yield overall >= 0.95, got {overall}"
        );
    }

    #[test]
    fn test_overall_confidence_one_field_missing() {
        let categorizer = MLCategorizer::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            "test-key".to_string(),
        );

        let gl = Some(make_suggestion(0.98));
        let dept = Some(CategorySuggestion {
            category_type: CategoryType::Department,
            value: "Engineering".to_string(),
            confidence: 0.98,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        });
        let cc: Option<CategorySuggestion> = None;

        let overall = categorizer.calculate_overall_confidence(&gl, &dept, &cc);
        // (0.98 + 0.98 + 0.0) / 3.0 ≈ 0.653
        assert!(
            overall < 0.70,
            "2 of 3 fields at 0.98 should yield ~0.653, got {overall}"
        );
        assert!(
            overall < 0.95,
            "Incomplete categorization must not reach 0.95 threshold, got {overall}"
        );
    }

    #[test]
    fn test_overall_confidence_two_fields_missing() {
        let categorizer = MLCategorizer::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            "test-key".to_string(),
        );

        let gl = Some(make_suggestion(0.98));
        let dept: Option<CategorySuggestion> = None;
        let cc: Option<CategorySuggestion> = None;

        let overall = categorizer.calculate_overall_confidence(&gl, &dept, &cc);
        // (0.98 + 0.0 + 0.0) / 3.0 ≈ 0.327
        let expected = 0.98_f32 / 3.0;
        assert!(
            (overall - expected).abs() < 0.01,
            "1 of 3 fields at 0.98 should yield ~0.327, got {overall}"
        );
        assert!(
            overall < 0.95,
            "Incomplete categorization must not reach 0.95 threshold, got {overall}"
        );
    }

    #[test]
    fn test_overall_confidence_no_fields() {
        let categorizer = MLCategorizer::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            "test-key".to_string(),
        );

        let overall = categorizer.calculate_overall_confidence(&None, &None, &None);
        assert!(
            overall == 0.0,
            "No fields should yield 0.0, got {overall}"
        );
    }
}
