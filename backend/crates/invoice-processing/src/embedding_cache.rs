//! Embedding Cache for Category and Vendor Embeddings
//!
//! Pre-computed embeddings for fast similarity searches.
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Duration, Utc};

use super::categorization_ml::MLCategorizer;
use super::categorization::CategoryType;

/// Embedding cache manager
pub struct EmbeddingCache {
    pool: PgPool,
}

/// Cached category embedding
#[derive(Debug, Clone)]
pub struct CachedCategoryEmbedding {
    pub id: Uuid,
    pub tenant_id: String,
    pub category_type: CategoryType,
    pub category_value: String,
    pub description: Option<String>,
    pub usage_count: i32,
    pub updated_at: chrono::DateTime<Utc>,
}

impl EmbeddingCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Pre-compute embeddings for all GL codes, departments, and cost centers
    pub async fn refresh_category_embeddings(
        &self,
        tenant_id: &str,
        categorizer: &MLCategorizer,
    ) -> Result<usize> {
        let mut count = 0;

        // Get all distinct GL codes from historical invoices
        let gl_codes = sqlx::query_as::<_, (String, i32)>(
            r#"
            SELECT gl_code, COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND gl_code IS NOT NULL
            GROUP BY gl_code
            ORDER BY usage_count DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch GL codes")?;

        // Generate and cache embeddings for each GL code
        for (gl_code, usage_count) in gl_codes {
            let description = self.infer_gl_code_description(&gl_code);
            let embedding = categorizer.generate_embedding(&format!("{} {}", gl_code, description)).await?;

            self.upsert_category_embedding(
                tenant_id,
                CategoryType::GlCode,
                &gl_code,
                Some(&description),
                &embedding,
                usage_count,
            )
            .await?;

            count += 1;
        }

        // Get all distinct departments
        let departments = sqlx::query_as::<_, (String, i32)>(
            r#"
            SELECT department, COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND department IS NOT NULL
            GROUP BY department
            ORDER BY usage_count DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch departments")?;

        for (department, usage_count) in departments {
            let embedding = categorizer.generate_embedding(&department).await?;

            self.upsert_category_embedding(
                tenant_id,
                CategoryType::Department,
                &department,
                None,
                &embedding,
                usage_count,
            )
            .await?;

            count += 1;
        }

        // Get all distinct cost centers
        let cost_centers = sqlx::query_as::<_, (String, i32)>(
            r#"
            SELECT cost_center, COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND cost_center IS NOT NULL
            GROUP BY cost_center
            ORDER BY usage_count DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch cost centers")?;

        for (cost_center, usage_count) in cost_centers {
            let embedding = categorizer.generate_embedding(&cost_center).await?;

            self.upsert_category_embedding(
                tenant_id,
                CategoryType::CostCenter,
                &cost_center,
                None,
                &embedding,
                usage_count,
            )
            .await?;

            count += 1;
        }

        Ok(count)
    }

    /// Refresh vendor embeddings based on recent invoices
    pub async fn refresh_vendor_embeddings(
        &self,
        tenant_id: &str,
        categorizer: &MLCategorizer,
        days: i32,
    ) -> Result<usize> {
        let since = Utc::now() - Duration::days(days as i64);

        // Get vendors with recent invoices
        let vendors = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT DISTINCT v.id, v.name
            FROM vendors v
            INNER JOIN invoices i ON i.vendor_id = v.id
            WHERE i.tenant_id = $1
            AND i.created_at >= $2
            ORDER BY v.name
            "#,
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch vendors for embedding refresh")?;

        let mut count = 0;

        for (vendor_id, vendor_name) in vendors {
            // Get recent invoices for this vendor
            let invoices = sqlx::query_as::<_, (String, f64)>(
                r#"
                SELECT
                    COALESCE(line_items->>'description', 'Invoice') as description,
                    (total_amount_cents::float / 100.0) as amount
                FROM invoices
                WHERE tenant_id = $1
                AND vendor_id = $2
                AND created_at >= $3
                ORDER BY created_at DESC
                LIMIT 10
                "#,
            )
            .bind(tenant_id)
            .bind(vendor_id)
            .bind(since)
            .fetch_all(&self.pool)
            .await?;

            // Build summary text
            let mut summary_parts = Vec::new();
            for (description, amount) in invoices {
                summary_parts.push(format!("{} (${:.2})", description, amount));
            }
            let invoice_summary = summary_parts.join(", ");

            // Generate embedding for vendor
            let embedding = categorizer
                .generate_embedding(&format!("{}: {}", vendor_name, invoice_summary))
                .await?;

            // Cache it
            categorizer
                .cache_vendor_embedding(tenant_id, vendor_id, &vendor_name, embedding, &invoice_summary)
                .await?;

            count += 1;
        }

        Ok(count)
    }

    /// Infer description for GL code based on patterns
    fn infer_gl_code_description(&self, gl_code: &str) -> String {
        // Common GL code patterns
        if gl_code.starts_with("6") {
            "Software subscriptions and technology expenses".to_string()
        } else if gl_code.starts_with("7") {
            "Marketing and advertising expenses".to_string()
        } else if gl_code.starts_with("8") {
            "Travel and entertainment expenses".to_string()
        } else if gl_code.starts_with("5") {
            "Office supplies and equipment".to_string()
        } else if gl_code.starts_with("9") {
            "Professional services and consulting".to_string()
        } else if gl_code.starts_with("4") {
            "Facilities and operations".to_string()
        } else if gl_code.starts_with("3") {
            "Human resources and benefits".to_string()
        } else {
            "General business expenses".to_string()
        }
    }

    /// Upsert category embedding
    async fn upsert_category_embedding(
        &self,
        tenant_id: &str,
        category_type: CategoryType,
        category_value: &str,
        description: Option<&str>,
        embedding: &[f32],
        usage_count: i32,
    ) -> Result<()> {
        let category_type_str = match category_type {
            CategoryType::GlCode => "gl_code",
            CategoryType::Department => "department",
            CategoryType::CostCenter => "cost_center",
        };

        sqlx::query(
            r#"
            INSERT INTO category_embeddings (
                tenant_id, category_type, category_value, description,
                embedding_vector, usage_count
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, category_type, category_value)
            DO UPDATE SET
                description = COALESCE(EXCLUDED.description, category_embeddings.description),
                embedding_vector = EXCLUDED.embedding_vector,
                usage_count = EXCLUDED.usage_count,
                updated_at = NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(category_type_str)
        .bind(category_value)
        .bind(description)
        .bind(embedding.to_vec())
        .bind(usage_count)
        .execute(&self.pool)
        .await
        .context("Failed to upsert category embedding")?;

        Ok(())
    }

    /// Get stale embeddings that need refresh
    pub async fn get_stale_embeddings(&self, tenant_id: &str, days: i32) -> Result<Vec<Uuid>> {
        let threshold = Utc::now() - Duration::days(days as i64);

        let ids = sqlx::query_as::<_, (Uuid,)>(
            r#"
            SELECT id
            FROM vendor_embeddings
            WHERE tenant_id = $1
            AND updated_at < $2
            "#,
        )
        .bind(tenant_id)
        .bind(threshold)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch stale embeddings")?;

        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    /// Get embedding statistics for a tenant
    pub async fn get_cache_stats(&self, tenant_id: &str) -> Result<CacheStats> {
        let vendor_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vendor_embeddings WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let category_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM category_embeddings WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let gl_code_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM category_embeddings WHERE tenant_id = $1 AND category_type = 'gl_code'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let department_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM category_embeddings WHERE tenant_id = $1 AND category_type = 'department'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let cost_center_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM category_embeddings WHERE tenant_id = $1 AND category_type = 'cost_center'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        Ok(CacheStats {
            vendor_embeddings: vendor_count,
            category_embeddings: category_count,
            gl_codes: gl_code_count,
            departments: department_count,
            cost_centers: cost_center_count,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub vendor_embeddings: i64,
    pub category_embeddings: i64,
    pub gl_codes: i64,
    pub departments: i64,
    pub cost_centers: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_infer_gl_code_description() {
        let cache = EmbeddingCache::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
        );

        assert!(cache.infer_gl_code_description("6000").contains("Software"));
        assert!(cache.infer_gl_code_description("7000").contains("Marketing"));
        assert!(cache.infer_gl_code_description("8000").contains("Travel"));
    }
}
