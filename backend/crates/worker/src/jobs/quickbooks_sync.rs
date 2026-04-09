//! QuickBooks synchronization jobs

use crate::config::WorkerConfig;
use anyhow::{Context, Result};
use billforge_core::TenantId;
use billforge_quickbooks::{QuickBooksClient, QuickBooksEnvironment, QuickBooksOAuth, QuickBooksOAuthConfig};
use chrono::{Duration, Utc};
use serde_json::Value;
use tracing::{error, info, warn};
use uuid::Uuid;

pub async fn sync_vendors(tenant_id: &str, _payload: &Value, config: &WorkerConfig) -> Result<()> {
    info!("Syncing QuickBooks vendors for tenant: {}", tenant_id);

    let tenant_id: TenantId = tenant_id
        .parse()
        .context("Invalid tenant_id for QuickBooks vendor sync")?;
    let pool = config.pg_manager.tenant(&tenant_id).await?;

    // Fetch QuickBooks connection for this tenant
    let connection: Option<(String, String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, refresh_token, access_token_expires_at \
         FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .context("Failed to fetch QuickBooks connection")?;

    let (company_id, mut access_token, refresh_token_val, token_expires_at) = match connection {
        Some(conn) => conn,
        None => {
            info!(
                "No QuickBooks connection found for tenant {} — skipping vendor sync",
                tenant_id.as_str()
            );
            return Ok(());
        }
    };

    // Create sync log entry immediately so failures are recorded
    let sync_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO quickbooks_sync_log (id, tenant_id, sync_type, status, started_at) \
         VALUES ($1, $2, 'vendors', 'running', NOW())",
    )
    .bind(sync_id)
    .bind(tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .context("Failed to create vendor sync log")?;

    // Run the rest in a helper so we can mark the sync log as 'failed' on any error
    let result = run_vendor_sync(
        config,
        &pool,
        &tenant_id,
        company_id,
        &mut access_token,
        &refresh_token_val,
        token_expires_at,
        sync_id,
    )
    .await;

    if let Err(ref e) = result {
        error!(error = %e, "QuickBooks vendor sync failed for tenant {}", tenant_id.as_str());
        // Best-effort: mark sync log as failed
        let _ = sqlx::query(
            "UPDATE quickbooks_sync_log \
             SET status = 'failed', completed_at = NOW() \
             WHERE id = $1",
        )
        .bind(sync_id)
        .execute(&*pool)
        .await;
    }

    result
}

async fn run_vendor_sync(
    config: &WorkerConfig,
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    company_id: String,
    access_token: &mut String,
    refresh_token_val: &str,
    token_expires_at: chrono::DateTime<Utc>,
    sync_id: Uuid,
) -> Result<()> {
    // Refresh access token if expired or expiring within 5 minutes
    if token_expires_at <= Utc::now() + Duration::minutes(5) {
        let qb_client_id = config
            .qb_client_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("QuickBooks OAuth config missing (QUICKBOOKS_CLIENT_ID) — cannot refresh token"))?;
        let qb_client_secret = config
            .qb_client_secret
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("QuickBooks OAuth config missing (QUICKBOOKS_CLIENT_SECRET) — cannot refresh token"))?;

        let env = match config.qb_environment.as_str() {
            "sandbox" => QuickBooksEnvironment::Sandbox,
            _ => QuickBooksEnvironment::Production,
        };

        let oauth = QuickBooksOAuth::new(QuickBooksOAuthConfig {
            client_id: qb_client_id.to_string(),
            client_secret: qb_client_secret.to_string(),
            redirect_uri: String::new(), // not needed for refresh
            environment: env,
        });

        let new_tokens = oauth.refresh_token(refresh_token_val).await?;

        let now = Utc::now();
        sqlx::query(
            "UPDATE quickbooks_connections \
             SET access_token = $2, refresh_token = $3, \
                 access_token_expires_at = $4, refresh_token_expires_at = $5, updated_at = NOW() \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(&new_tokens.refresh_token)
        .bind(now + Duration::seconds(new_tokens.expires_in))
        .bind(now + Duration::seconds(new_tokens.x_refresh_token_expires_in))
        .execute(pool)
        .await
        .context("Failed to persist refreshed QuickBooks tokens")?;

        *access_token = new_tokens.access_token;
    }

    // Build QuickBooks client
    let env = match config.qb_environment.as_str() {
        "sandbox" => QuickBooksEnvironment::Sandbox,
        _ => QuickBooksEnvironment::Production,
    };
    let client = QuickBooksClient::new(access_token.clone(), company_id, env);

    // Paginate through all vendors
    let mut all_vendors = Vec::new();
    let mut start_position = 1;
    let max_results = 100;

    loop {
        let vendors = client.query_vendors(start_position, max_results).await?;
        if vendors.is_empty() {
            break;
        }
        all_vendors.extend(vendors);
        start_position += max_results;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;
    let mut errors = 0u64;

    // Sync each vendor
    for qb_vendor in &all_vendors {
        // Check if vendor already exists
        let existing: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
            "SELECT v.id FROM vendors v \
             INNER JOIN quickbooks_vendor_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.quickbooks_vendor_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&qb_vendor.Id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to look up vendor mapping");
            e
        })?;

        if let Some((vendor_id,)) = existing {
            // Update existing vendor
            let email = qb_vendor.PrimaryEmailAddr.as_ref().map(|e| e.Address.as_str()).unwrap_or("");
            let phone = qb_vendor.PrimaryPhone.as_ref().map(|p| p.FreeFormNumber.as_str()).unwrap_or("");

            if let Err(e) = sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&qb_vendor.DisplayName)
            .bind(email)
            .bind(phone)
            .execute(pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to update vendor");
                errors += 1;
                continue;
            }

            // Update mapping
            if let Err(e) = sqlx::query(
                "UPDATE quickbooks_vendor_mappings \
                 SET quickbooks_vendor_name = $3, sync_token = $4, last_synced_at = NOW(), updated_at = NOW() \
                 WHERE tenant_id = $1 AND quickbooks_vendor_id = $2",
            )
            .bind(tenant_id.as_uuid())
            .bind(&qb_vendor.Id)
            .bind(&qb_vendor.DisplayName)
            .bind(&qb_vendor.SyncToken)
            .execute(pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to update vendor mapping");
                errors += 1;
                continue;
            }

            updated += 1;
        } else {
            // Create new vendor
            let new_vendor_id = Uuid::new_v4();
            let email = qb_vendor.PrimaryEmailAddr.as_ref().map(|e| e.Address.as_str()).unwrap_or("");
            let phone = qb_vendor.PrimaryPhone.as_ref().map(|p| p.FreeFormNumber.as_str()).unwrap_or("");
            let vendor_type = if qb_vendor.CompanyName.is_some() { "business" } else { "contractor" };

            if let Err(e) = sqlx::query(
                "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
            )
            .bind(new_vendor_id)
            .bind(tenant_id.as_uuid())
            .bind(&qb_vendor.DisplayName)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if qb_vendor.Active { "active" } else { "inactive" })
            .execute(pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to insert vendor");
                errors += 1;
                continue;
            }

            // Create mapping
            if let Err(e) = sqlx::query(
                "INSERT INTO quickbooks_vendor_mappings \
                 (tenant_id, quickbooks_vendor_id, billforge_vendor_id, quickbooks_vendor_name, sync_token, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(&qb_vendor.Id)
            .bind(new_vendor_id)
            .bind(&qb_vendor.DisplayName)
            .bind(&qb_vendor.SyncToken)
            .execute(pool)
            .await
            {
                error!(error = %e, vendor_id = %qb_vendor.Id, "Failed to insert vendor mapping");
                errors += 1;
                continue;
            }

            imported += 1;
        }
    }

    // Update sync log to completed
    sqlx::query(
        "UPDATE quickbooks_sync_log \
         SET status = 'completed', completed_at = NOW(), records_processed = $2, records_created = $3, records_updated = $4 \
         WHERE id = $1",
    )
    .bind(sync_id)
    .bind((imported + updated) as i32)
    .bind(imported as i32)
    .bind(updated as i32)
    .execute(pool)
    .await
    .context("Failed to update vendor sync log to completed")?;

    // Update last sync time on connection
    sqlx::query(
        "UPDATE quickbooks_connections SET last_sync_at = NOW() WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .execute(pool)
    .await
    .context("Failed to update last sync time on QuickBooks connection")?;

    info!(
        "QuickBooks vendor sync completed for tenant {}: imported={}, updated={}, errors={}",
        tenant_id.as_str(),
        imported,
        updated,
        errors
    );

    Ok(())
}

pub async fn sync_accounts(tenant_id: &str, _payload: &Value, _config: &WorkerConfig) -> Result<()> {
    // TODO(#138 follow-up): Implement real QuickBooks account sync background job.
    // The sync logic lives in backend/crates/api/src/routes/quickbooks.rs sync_accounts handler.
    // This stub returns Ok(()) so queued jobs don't accumulate in the DLQ.
    warn!(
        "QuickBooks account sync for tenant {} is not yet implemented as a background job — skipping",
        tenant_id
    );
    Ok(())
}

pub async fn export_invoice(tenant_id: &str, _payload: &Value, _config: &WorkerConfig) -> Result<()> {
    // TODO(#138 follow-up): Implement real QuickBooks invoice export background job.
    // The export logic lives in backend/crates/api/src/routes/quickbooks.rs export_invoice_to_quickbooks handler.
    // This stub returns Ok(()) so queued jobs don't accumulate in the DLQ.
    warn!(
        "QuickBooks invoice export for tenant {} is not yet implemented as a background job — skipping",
        tenant_id
    );
    Ok(())
}
