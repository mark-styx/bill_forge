//! Anomaly Detection Job
//!
//! Daily job to detect anomalies in invoices, vendor behavior, and budgets.

use anyhow::{Context, Result};
use tracing::{info, warn};
use uuid::Uuid;

use billforge_analytics::PredictiveService;
use billforge_core::TenantId;
use billforge_db::PgManager;
use std::sync::Arc;

/// Run anomaly detection for all active tenants
pub async fn detect_anomalies(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting anomaly detection job");

    // Get all active tenants
    let metadata_pool = pg_manager.metadata();
    let tenants: Vec<(String,)> = sqlx::query_as(
        "SELECT id::text FROM tenants WHERE active = true",
    )
    .fetch_all(metadata_pool)
    .await
    .context("Failed to fetch active tenants")?;

    info!("Processing {} active tenants", tenants.len());

    for (tenant_id_str,) in tenants {
        if let Err(e) = process_tenant_anomalies(pg_manager.clone(), &tenant_id_str).await {
            warn!("Failed to process anomalies for tenant {}: {}", tenant_id_str, e);
        }
    }

    info!("Anomaly detection job completed");
    Ok(())
}

/// Process anomaly detection for a single tenant
async fn process_tenant_anomalies(
    pg_manager: Arc<PgManager>,
    tenant_id_str: &str,
) -> Result<()> {
    info!("Processing anomalies for tenant {}", tenant_id_str);

    let tenant_id: TenantId = tenant_id_str.parse()
        .context("Invalid tenant ID format")?;
    let pool = pg_manager.tenant(&tenant_id).await?;

    let uuid_tenant_id: Uuid = tenant_id_str.parse()
        .context("Invalid tenant UUID format")?;
    let service = PredictiveService::new((*pool).clone());

    // 1. Detect invoice anomalies (last 30 days)
    let anomalies = service
        .detect_invoice_anomalies(&pool, uuid_tenant_id, 30)
        .await
        .context("Failed to detect invoice anomalies")?;

    info!(
        "Detected {} anomalies for tenant {}",
        anomalies.len(),
        tenant_id_str
    );

    // 2. Check budget thresholds
    let alerts = service
        .check_budget_thresholds(&pool, uuid_tenant_id)
        .await
        .context("Failed to check budget thresholds")?;

    info!(
        "Generated {} budget alerts for tenant {}",
        alerts.len(),
        tenant_id_str
    );

    // 3. Calculate forecast accuracy (for past forecasts)
    if let Err(e) = service.calculate_forecast_accuracy(&pool, uuid_tenant_id).await {
        warn!(
            "Failed to calculate forecast accuracy for tenant {}: {}",
            tenant_id_str, e
        );
    }

    // 4. Send notifications for critical anomalies (future: integrate with notification system)
    let critical_anomalies: Vec<_> = anomalies
        .iter()
        .filter(|a| a.severity == billforge_analytics::AnomalySeverity::Critical)
        .collect();

    if !critical_anomalies.is_empty() {
        warn!(
            "Found {} critical anomalies for tenant {} - requires immediate attention",
            critical_anomalies.len(),
            tenant_id_str
        );

        // TODO: Send notifications via notification system
        // For now, just log them
        for anomaly in critical_anomalies {
            warn!(
                "Critical anomaly: type={:?}, entity={}, value={}",
                anomaly.anomaly_type, anomaly.entity_id, anomaly.detected_value
            );
        }
    }

    Ok(())
}
