//! Background job for refreshing embeddings
//!
//! Periodically updates vendor and category embeddings to keep them fresh.
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use anyhow::{Context, Result};
use billforge_core::TenantId;
use billforge_db::PgManager;
use std::env;
use std::sync::Arc;
use tracing::{info, warn};

use billforge_invoice_processing::{
    categorization_ml::MLCategorizer, embedding_cache::EmbeddingCache,
};

/// Refresh embeddings for all tenants
pub async fn refresh_all_embeddings(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting embedding refresh job");

    let openai_api_key =
        env::var("OPENAI_API_KEY").context("OPENAI_API_KEY environment variable not set")?;

    // Get all active tenants from metadata database
    let metadata_pool = pg_manager.metadata();
    let tenants =
        sqlx::query_as::<_, (String,)>("SELECT id::text FROM tenants WHERE is_active = true")
            .fetch_all(metadata_pool)
            .await
            .context("Failed to fetch tenants")?;

    info!("Refreshing embeddings for {} tenants", tenants.len());

    for (tenant_id,) in tenants {
        let tenant_id = match tenant_id.parse::<TenantId>() {
            Ok(tenant_id) => tenant_id,
            Err(e) => {
                warn!(tenant_id = %tenant_id, error = %e, "Skipping invalid tenant id");
                continue;
            }
        };
        match refresh_tenant_embeddings_with_api_key(&pg_manager, &tenant_id, &openai_api_key).await
        {
            Ok(stats) => {
                info!(
                    tenant_id = %tenant_id,
                    categories = stats.category_embeddings,
                    vendors = stats.vendor_embeddings,
                    "Embedding refresh completed"
                );
            }
            Err(e) => {
                warn!(tenant_id = %tenant_id, error = %e, "Failed to refresh embeddings for tenant");
            }
        }
    }

    info!("Embedding refresh job completed");
    Ok(())
}

pub async fn refresh_tenant_embeddings(
    pg_manager: Arc<PgManager>,
    tenant_id: &TenantId,
) -> Result<billforge_invoice_processing::embedding_cache::CacheStats> {
    let openai_api_key =
        env::var("OPENAI_API_KEY").context("OPENAI_API_KEY environment variable not set")?;
    refresh_tenant_embeddings_with_api_key(&pg_manager, tenant_id, &openai_api_key).await
}

/// Refresh embeddings for a single tenant.
async fn refresh_tenant_embeddings_with_api_key(
    pg_manager: &PgManager,
    tenant_id: &TenantId,
    openai_api_key: &str,
) -> Result<billforge_invoice_processing::embedding_cache::CacheStats> {
    let pool = pg_manager.tenant(tenant_id).await?;

    let categorizer = MLCategorizer::new((*pool).clone(), openai_api_key.to_string());
    let cache = EmbeddingCache::new((*pool).clone());

    let tenant_id_str = tenant_id.as_str();

    // Refresh category embeddings (GL codes, departments, cost centers)
    let categories_refreshed = cache
        .refresh_category_embeddings(&tenant_id_str, &categorizer)
        .await
        .context("Failed to refresh category embeddings")?;

    info!(
        tenant_id = %tenant_id,
        count = categories_refreshed,
        "Refreshed category embeddings"
    );

    // Refresh vendor embeddings from last 30 days
    let vendors_refreshed = cache
        .refresh_vendor_embeddings(&tenant_id_str, &categorizer, 30)
        .await
        .context("Failed to refresh vendor embeddings")?;

    info!(
        tenant_id = %tenant_id,
        count = vendors_refreshed,
        "Refreshed vendor embeddings"
    );

    // Return cache stats
    cache.get_cache_stats(&tenant_id_str).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_refresh_structure() {
        // This would require a database and OpenAI API key
        // In production, use testcontainers or mock the pool
    }
}
