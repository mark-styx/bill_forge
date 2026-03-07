//! QuickBooks synchronization jobs

use crate::config::WorkerConfig;
use anyhow::Result;
use serde_json::Value;
use tracing::info;

pub async fn sync_vendors(tenant_id: &str, _payload: &Value, _config: &WorkerConfig) -> Result<()> {
    info!("Syncing QuickBooks vendors for tenant: {}", tenant_id);

    // This job is now handled by the API endpoint directly
    // The worker job can be used for scheduled background syncs
    info!("QuickBooks vendor sync is handled via API endpoint /api/v1/quickbooks/sync/vendors");

    Ok(())
}

pub async fn sync_accounts(tenant_id: &str, _payload: &Value, _config: &WorkerConfig) -> Result<()> {
    info!("Syncing QuickBooks accounts for tenant: {}", tenant_id);

    // This job is now handled by the API endpoint directly
    // The worker job can be used for scheduled background syncs
    info!("QuickBooks account sync is handled via API endpoint /api/v1/quickbooks/sync/accounts");

    Ok(())
}

pub async fn export_invoice(tenant_id: &str, _payload: &Value, _config: &WorkerConfig) -> Result<()> {
    info!("Exporting invoice to QuickBooks for tenant: {}", tenant_id);

    // This job is now handled by the API endpoint directly
    // The worker job can be used for scheduled batch exports
    info!("QuickBooks invoice export is handled via API endpoint");

    Ok(())
}
