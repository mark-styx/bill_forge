//! Helper for enqueueing non-QuickBooks ERP sync/export jobs to the worker.
//!
//! The HTTP route handlers in xero.rs / sage_intacct.rs / salesforce.rs /
//! workday.rs / bill_com.rs / netsuite.rs used to paginate ERP APIs and write
//! to Postgres inline, blocking the request handler for the entire sync. This
//! module provides the small bit of plumbing they need to hand the work off
//! to the worker and return 202 Accepted within the sub-200ms API budget.
//!
//! The wire format here must match `billforge_worker::jobs::Job`. We do not
//! depend on the worker crate (which would invert the dependency graph), so
//! the discriminator strings below — `xero_contact_sync`, `xero_account_sync`,
//! etc. — must stay in sync with the snake_case serde rename on the worker's
//! `JobType` enum. The worker has a unit test that pins those values; bend
//! either side and tests there will catch the drift.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use billforge_core::TenantId;
use redis::AsyncCommands;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// Discriminators for the worker JobType enum (rename_all = snake_case).
pub(crate) mod job_type {
    pub const XERO_CONTACT_SYNC: &str = "xero_contact_sync";
    pub const XERO_ACCOUNT_SYNC: &str = "xero_account_sync";
    pub const XERO_INVOICE_EXPORT: &str = "xero_invoice_export";
    pub const SAGE_INTACCT_VENDOR_SYNC: &str = "sage_intacct_vendor_sync";
    pub const SAGE_INTACCT_ACCOUNT_SYNC: &str = "sage_intacct_account_sync";
    pub const SAGE_INTACCT_INVOICE_EXPORT: &str = "sage_intacct_invoice_export";
    pub const SALESFORCE_ACCOUNT_SYNC: &str = "salesforce_account_sync";
    pub const SALESFORCE_CONTACT_SYNC: &str = "salesforce_contact_sync";
    pub const WORKDAY_SUPPLIER_SYNC: &str = "workday_supplier_sync";
    pub const WORKDAY_ACCOUNT_SYNC: &str = "workday_account_sync";
    pub const WORKDAY_INVOICE_EXPORT: &str = "workday_invoice_export";
    pub const BILL_COM_VENDOR_SYNC: &str = "bill_com_vendor_sync";
    pub const NETSUITE_VENDOR_SYNC: &str = "netsuite_vendor_sync";
}

/// Body returned by every ERP sync/export endpoint after the refactor.
#[derive(Debug, Serialize)]
pub struct EnqueuedResponse {
    pub job_id: String,
    pub status: &'static str,
}

/// Build the wire JSON for a single job. Lifted out so tests can pin the
/// shape without going through Redis.
pub(crate) fn build_job_payload(
    job_type: &str,
    tenant_id: &TenantId,
    payload: Value,
) -> (Uuid, Value) {
    let job_id = Uuid::new_v4();
    let job = serde_json::json!({
        "id": job_id.to_string(),
        "job_type": job_type,
        "tenant_id": tenant_id.to_string(),
        "payload": payload,
        "created_at": chrono::Utc::now(),
        "retry_count": 0,
    });
    (job_id, job)
}

/// Enqueue an ERP job to the shared worker queue. Returns 202 with the
/// generated job_id. If Redis is unavailable the endpoint surfaces 503 so
/// the caller can retry — running the sync inline as a fallback is exactly
/// what this whole refactor is removing.
pub async fn enqueue_erp_job(
    redis_client: Option<&redis::Client>,
    job_type: &str,
    tenant_id: &TenantId,
    payload: Value,
) -> Result<impl IntoResponse, crate::error::ApiError> {
    let client = redis_client.ok_or_else(|| {
        crate::error::ApiError(billforge_core::Error::Internal(
            "Background job queue not configured (REDIS_URL missing)".to_string(),
        ))
    })?;

    let (job_id, job) = build_job_payload(job_type, tenant_id, payload);
    let job_json = serde_json::to_string(&job).map_err(|e| {
        crate::error::ApiError(billforge_core::Error::Internal(format!(
            "Failed to serialize ERP job: {}",
            e
        )))
    })?;

    let mut conn = client.get_async_connection().await.map_err(|e| {
        crate::error::ApiError(billforge_core::Error::Internal(format!(
            "Redis connection failed: {}",
            e
        )))
    })?;

    conn.lpush::<_, _, ()>("billforge:jobs:queue", job_json)
        .await
        .map_err(|e| {
            crate::error::ApiError(billforge_core::Error::Internal(format!(
                "Failed to enqueue ERP job: {}",
                e
            )))
        })?;

    tracing::info!(
        tenant_id = %tenant_id,
        job_id = %job_id,
        job_type = %job_type,
        "ERP sync/export job enqueued"
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(EnqueuedResponse {
            job_id: job_id.to_string(),
            status: "queued",
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_payload_carries_tenant_and_type() {
        let tenant: TenantId = "11111111-1111-1111-1111-111111111111".parse().unwrap();
        let payload = serde_json::json!({"full_sync": true});
        let (job_id, body) = build_job_payload(job_type::XERO_CONTACT_SYNC, &tenant, payload);

        assert_eq!(body["job_type"], "xero_contact_sync");
        assert_eq!(body["tenant_id"], tenant.to_string());
        assert_eq!(body["payload"]["full_sync"], true);
        assert_eq!(body["retry_count"], 0);
        assert_eq!(body["id"], job_id.to_string());
    }

    #[test]
    fn job_type_discriminators_are_stable() {
        // Pin the wire-format constants. Changes here MUST be matched by the
        // serde rename_all = "snake_case" variants on the worker's JobType.
        assert_eq!(job_type::XERO_CONTACT_SYNC, "xero_contact_sync");
        assert_eq!(job_type::XERO_ACCOUNT_SYNC, "xero_account_sync");
        assert_eq!(job_type::XERO_INVOICE_EXPORT, "xero_invoice_export");
        assert_eq!(
            job_type::SAGE_INTACCT_VENDOR_SYNC,
            "sage_intacct_vendor_sync"
        );
        assert_eq!(
            job_type::SAGE_INTACCT_ACCOUNT_SYNC,
            "sage_intacct_account_sync"
        );
        assert_eq!(
            job_type::SAGE_INTACCT_INVOICE_EXPORT,
            "sage_intacct_invoice_export"
        );
        assert_eq!(job_type::SALESFORCE_ACCOUNT_SYNC, "salesforce_account_sync");
        assert_eq!(job_type::SALESFORCE_CONTACT_SYNC, "salesforce_contact_sync");
        assert_eq!(job_type::WORKDAY_SUPPLIER_SYNC, "workday_supplier_sync");
        assert_eq!(job_type::WORKDAY_ACCOUNT_SYNC, "workday_account_sync");
        assert_eq!(job_type::WORKDAY_INVOICE_EXPORT, "workday_invoice_export");
        assert_eq!(job_type::BILL_COM_VENDOR_SYNC, "bill_com_vendor_sync");
        assert_eq!(job_type::NETSUITE_VENDOR_SYNC, "netsuite_vendor_sync");
    }
}
