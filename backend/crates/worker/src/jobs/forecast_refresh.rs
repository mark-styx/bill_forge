//! Forecast Refresh Job
//!
//! Weekly job to regenerate forecasts for all active vendors and departments.

use anyhow::{Context, Result};
use tracing::{info, warn};
use uuid::Uuid;

use billforge_analytics::PredictiveService;
use billforge_core::TenantId;
use billforge_db::PgManager;
use std::sync::Arc;

/// Refresh forecasts for all active vendors
pub async fn refresh_forecasts(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting forecast refresh job");

    // Get all active tenants
    let metadata_pool = pg_manager.metadata();
    let tenants: Vec<(String,)> =
        sqlx::query_as("SELECT id::text FROM tenants WHERE active = true")
            .fetch_all(metadata_pool)
            .await
            .context("Failed to fetch active tenants")?;

    info!("Processing {} active tenants", tenants.len());

    for (tenant_id_str,) in tenants {
        let tenant_id = match tenant_id_str.parse::<TenantId>() {
            Ok(tenant_id) => tenant_id,
            Err(e) => {
                warn!(tenant_id = %tenant_id_str, error = %e, "Skipping invalid tenant id");
                continue;
            }
        };
        if let Err(e) = process_tenant_forecasts(pg_manager.clone(), &tenant_id).await {
            warn!(
                "Failed to process forecasts for tenant {}: {}",
                tenant_id_str, e
            );
        }
    }

    info!("Forecast refresh job completed");
    Ok(())
}

/// Refresh forecasts for a single validated tenant.
pub async fn refresh_tenant_forecasts(
    pg_manager: Arc<PgManager>,
    tenant_id: &TenantId,
) -> Result<()> {
    process_tenant_forecasts(pg_manager, tenant_id).await
}

/// Process forecasts for a single tenant
async fn process_tenant_forecasts(pg_manager: Arc<PgManager>, tenant_id: &TenantId) -> Result<()> {
    let tenant_id_str = tenant_id.as_str();
    info!("Processing forecasts for tenant {}", tenant_id_str);

    let pool = pg_manager.tenant(tenant_id).await?;

    let uuid_tenant_id: Uuid = tenant_id_str
        .parse()
        .context("Invalid tenant UUID format")?;
    let service = PredictiveService::new((*pool).clone());

    // Get active vendors (vendors with invoices in last 90 days)
    let vendors: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT vendor_id::text
        FROM invoices
        WHERE tenant_id = $1
            AND created_at > NOW() - INTERVAL '90 days'
            AND status != 'rejected'
        ORDER BY vendor_id
        "#,
    )
    .bind(uuid_tenant_id)
    .fetch_all(&*pool)
    .await
    .context("Failed to fetch active vendors")?;

    info!(
        "Found {} active vendors for tenant {}",
        vendors.len(),
        tenant_id_str
    );

    // Generate vendor forecasts
    let vendor_forecasts = service
        .generate_vendor_forecasts(uuid_tenant_id, &vendors)
        .await
        .context("Failed to generate vendor forecasts")?;

    info!(
        "Generated {} vendor forecasts for tenant {}",
        vendor_forecasts.len(),
        tenant_id_str
    );

    // Get active departments
    let departments: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT department
        FROM invoices
        WHERE tenant_id = $1
            AND created_at > NOW() - INTERVAL '90 days'
            AND status != 'rejected'
            AND department IS NOT NULL
        ORDER BY department
        "#,
    )
    .bind(uuid_tenant_id)
    .fetch_all(&*pool)
    .await
    .context("Failed to fetch active departments")?;

    if !departments.is_empty() {
        info!(
            "Found {} active departments for tenant {}",
            departments.len(),
            tenant_id_str
        );

        // Generate department forecasts
        let dept_forecasts = service
            .generate_department_forecasts(uuid_tenant_id, &departments)
            .await
            .context("Failed to generate department forecasts")?;

        info!(
            "Generated {} department forecasts for tenant {}",
            dept_forecasts.len(),
            tenant_id_str
        );
    }

    Ok(())
}
