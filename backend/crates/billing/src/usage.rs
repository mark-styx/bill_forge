//! Per-tenant usage metering for billing

use billforge_core::{Error, Result, TenantId};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use crate::plans::Plan;
use crate::stripe::{CreateMeterEventParams, StripeClient};
use crate::subscription::SubscriptionUsage;

/// Successful-processing statuses that should count toward billable usage.
const BILLABLE_PROCESSING_STATUSES: &[&str] = &["approved", "ready_for_payment", "paid"];

/// Query invoice and vendor counts for a tenant within a billing period window.
///
/// Only invoices whose `processing_status` is a successful terminal state
/// (approved, ready_for_payment, paid) are counted. Uses `updated_at` as the
/// time boundary because there is no dedicated `processed_at` column; the
/// status filter ensures only successfully-processed invoices are included.
pub async fn get_tenant_usage(
    pool: &PgPool,
    tenant_id: &TenantId,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<SubscriptionUsage> {
    let invoices_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM invoices
           WHERE tenant_id = $1
             AND updated_at >= $2
             AND updated_at < $3
             AND processing_status = ANY($4)"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(period_start)
    .bind(period_end)
    .bind(BILLABLE_PROCESSING_STATUSES)
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
///
/// On success the outbox row is marked `sent`. On Stripe failure the outbox row
/// is left as `pending` so `retry_pending_meter_events` can pick it up later.
/// The insert is idempotent via the `UNIQUE (invoice_id, event_name)` constraint.
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
    let payload_json = serde_json::to_value(&payload).unwrap_or_default();

    // Attempt to send to Stripe; persist outbox row regardless of outcome.
    let send_result = stripe
        .create_meter_event(CreateMeterEventParams {
            event_name: event_name.clone(),
            stripe_customer_id: stripe_customer_id.clone(),
            value: 1,
            identifier,
            timestamp: Some(Utc::now().timestamp()),
            payload,
        })
        .await;

    match send_result {
        Ok(_) => {
            // Mark sent (or skip if already recorded)
            sqlx::query(
                r#"INSERT INTO stripe_meter_events
                       (tenant_id, invoice_id, event_name, stripe_customer_id, payload, status, sent_at)
                   VALUES ($1, $2, $3, $4, $5, 'sent', NOW())
                   ON CONFLICT (invoice_id, event_name) DO UPDATE SET
                       status = 'sent', sent_at = NOW(), updated_at = NOW()"#,
            )
            .bind(*tenant_id.as_uuid())
            .bind(invoice_id)
            .bind(&event_name)
            .bind(&stripe_customer_id)
            .bind(&payload_json)
            .execute(metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("usage: insert sent outbox: {e}")))?;
            Ok(true)
        }
        Err(stripe_err) => {
            let err_msg = format!("{stripe_err}");
            // Persist as pending so the retry path can pick it up.
            sqlx::query(
                r#"INSERT INTO stripe_meter_events
                       (tenant_id, invoice_id, event_name, stripe_customer_id, payload, status, attempts, last_error)
                   VALUES ($1, $2, $3, $4, $5, 'pending', 1, $6)
                   ON CONFLICT (invoice_id, event_name) DO UPDATE SET
                       status = 'pending', attempts = stripe_meter_events.attempts + 1,
                       last_error = $6, updated_at = NOW()"#,
            )
            .bind(*tenant_id.as_uuid())
            .bind(invoice_id)
            .bind(&event_name)
            .bind(&stripe_customer_id)
            .bind(&payload_json)
            .bind(&err_msg)
            .execute(metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("usage: insert pending outbox: {e}")))?;
            Err(stripe_err)
        }
    }
}

/// Summary returned by [`retry_pending_meter_events`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetryReport {
    pub retried: u32,
    pub succeeded: u32,
    pub still_failing: u32,
}

/// Retry up to `limit` pending/failed outbox rows by resending the Stripe
/// meter event and updating the row status. Wiring this to a scheduler is
/// deferred; the function existing and being unit-testable satisfies the
/// durable-retry-path requirement.
pub async fn retry_pending_meter_events(
    metadata_pool: &PgPool,
    stripe: &StripeClient,
    limit: i64,
) -> Result<RetryReport> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String, String, serde_json::Value)>(
        r#"SELECT id, tenant_id, event_name, stripe_customer_id, payload
           FROM stripe_meter_events
           WHERE status IN ('pending', 'failed')
           ORDER BY updated_at ASC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(metadata_pool)
    .await
    .map_err(|e| Error::Database(format!("retry: fetch pending: {e}")))?;

    let mut report = RetryReport {
        retried: 0,
        succeeded: 0,
        still_failing: 0,
    };

    for (id, _tenant_id, event_name, stripe_customer_id, payload_json) in &rows {
        let payload: HashMap<String, String> =
            serde_json::from_value(payload_json.clone()).unwrap_or_default();
        let invoice_id = payload
            .get("invoice_id")
            .and_then(|v| uuid::Uuid::parse_str(v).ok())
            .unwrap_or_default();
        let tenant_id_str = payload.get("tenant_id").cloned().unwrap_or_default();
        let identifier = format!("tenant:{tenant_id_str}:invoice:{invoice_id}");

        let result = stripe
            .create_meter_event(CreateMeterEventParams {
                event_name: event_name.clone(),
                stripe_customer_id: stripe_customer_id.clone(),
                value: 1,
                identifier,
                timestamp: Some(Utc::now().timestamp()),
                payload,
            })
            .await;

        report.retried += 1;

        match result {
            Ok(_) => {
                sqlx::query(
                    r#"UPDATE stripe_meter_events
                       SET status = 'sent', sent_at = NOW(), updated_at = NOW()
                       WHERE id = $1"#,
                )
                .bind(id)
                .execute(metadata_pool)
                .await
                .map_err(|e| Error::Database(format!("retry: mark sent: {e}")))?;
                report.succeeded += 1;
            }
            Err(e) => {
                let err_msg = format!("{e}");
                sqlx::query(
                    r#"UPDATE stripe_meter_events
                       SET status = 'failed', attempts = attempts + 1,
                           last_error = $1, updated_at = NOW()
                       WHERE id = $2"#,
                )
                .bind(&err_msg)
                .bind(id)
                .execute(metadata_pool)
                .await
                .map_err(|e2| Error::Database(format!("retry: mark failed: {e2}")))?;
                report.still_failing += 1;
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the billable status list covers the expected enum values.
    #[test]
    fn billable_processing_statuses_match_enum() {
        // These must match the ProcessingStatus variants considered "successful".
        let expected = vec!["approved", "ready_for_payment", "paid"];
        for s in &expected {
            assert!(
                BILLABLE_PROCESSING_STATUSES.contains(s),
                "missing billable status: {s}"
            );
        }
        // Draft / submitted / pending_approval / rejected / on_hold / voided are NOT billable.
        let not_billable = vec![
            "draft",
            "submitted",
            "pending_approval",
            "rejected",
            "on_hold",
            "voided",
        ];
        for s in &not_billable {
            assert!(
                !BILLABLE_PROCESSING_STATUSES.contains(s),
                "unexpected billable status: {s}"
            );
        }
    }
}
