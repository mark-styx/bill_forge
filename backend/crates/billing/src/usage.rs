//! Per-tenant usage metering for billing

use billforge_core::{Error, Result, TenantId};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use crate::plans::Plan;
use crate::stripe::{CreateMeterEventParams, StripeClient};
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

    let vendor_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendors WHERE tenant_id = $1")
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

/// Report one processed invoice to Stripe metered billing when the tenant is on
/// a paid metered plan and has a Stripe customer attached to its subscription.
pub async fn record_invoice_meter_event(
    metadata_pool: &PgPool,
    stripe: Option<&StripeClient>,
    tenant_id: &TenantId,
    invoice_id: &uuid::Uuid,
) -> Result<bool> {
    let stripe = match stripe {
        Some(stripe) => stripe,
        None => return Ok(false),
    };

    let row = sqlx::query(
        "SELECT plan_id, stripe_customer_id FROM tenant_subscriptions WHERE tenant_id = $1",
    )
    .bind(*tenant_id.as_uuid())
    .fetch_optional(metadata_pool)
    .await
    .map_err(|e| Error::Database(format!("usage: load subscription for meter event: {e}")))?;

    let Some(row) = row else {
        return Ok(false);
    };

    let plan_id: String = row
        .try_get("plan_id")
        .map_err(|e| Error::Database(format!("usage: read plan_id: {e}")))?;
    let stripe_customer_id: Option<String> = row
        .try_get("stripe_customer_id")
        .map_err(|e| Error::Database(format!("usage: read stripe_customer_id: {e}")))?;

    let plan = plan_id
        .parse::<crate::plans::PlanId>()
        .map(Plan::by_id)
        .map_err(|e| Error::Database(format!("usage: invalid plan_id: {e}")))?;

    let Some(stripe_customer_id) = stripe_customer_id.filter(|id| !id.is_empty()) else {
        return Ok(false);
    };

    if !plan.stripe_invoice_metering_enabled {
        return Ok(false);
    }

    let event_name = std::env::var("STRIPE_INVOICE_METER_EVENT_NAME")
        .unwrap_or_else(|_| "billforge_invoice_processed".to_string());
    let identifier = format!("tenant:{}:invoice:{}", tenant_id.as_str(), invoice_id);
    let mut payload = HashMap::new();
    payload.insert("tenant_id".to_string(), tenant_id.as_str().to_string());
    payload.insert("invoice_id".to_string(), invoice_id.to_string());
    payload.insert("plan_id".to_string(), plan_id);
    payload.insert(
        "unit_price_cents".to_string(),
        plan.metered_invoice_unit_price_cents.to_string(),
    );

    stripe
        .create_meter_event(CreateMeterEventParams {
            event_name,
            stripe_customer_id,
            value: 1,
            identifier,
            timestamp: Some(Utc::now().timestamp()),
            payload,
        })
        .await?;

    Ok(true)
}
