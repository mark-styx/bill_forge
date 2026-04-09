//! Per-tenant usage metering for billing

use billforge_core::{Error, Result, TenantId};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::subscription::SubscriptionUsage;

/// Query invoice and vendor counts for a tenant within a billing period window.
pub async fn get_tenant_usage(
    pool: &PgPool,
    tenant_id: &TenantId,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<SubscriptionUsage> {
    let invoices_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3",
    )
    .bind(*tenant_id.as_uuid())
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("usage: count invoices: {e}")))?;

    let vendor_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vendors WHERE tenant_id = $1",
    )
    .bind(*tenant_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("usage: count vendors: {e}")))?;

    // TODO(#133-followup): populate user_count from users table
    // TODO(#133-followup): populate storage_bytes from documents table
    Ok(SubscriptionUsage {
        tenant_id: tenant_id.clone(),
        period_start,
        period_end,
        user_count: 0,
        invoices_count: invoices_count as u32,
        vendor_count: vendor_count as u32,
        storage_bytes: 0,
    })
}
