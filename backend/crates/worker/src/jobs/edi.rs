//! EDI background job handlers (issue #402).
//!
//! Wires the four `JobType::Edi*` variants to the existing `billforge_edi`
//! library code. Each handler verifies the tenant still has the EDI module
//! entitlement before any side effect — a tenant whose entitlement was removed
//! between enqueue and processing must not be able to drive outbound traffic.
//!
//! The EDI crate is parked; this module adds NO new EDI domain logic. It only
//! connects the existing handlers (`OutboundEdiService::send_remittance`,
//! `send_ack`, `check_ack_timeouts`, `process_inbound_ack`) to the JobType
//! wire contract that already shipped in `JobType::Edi*`.
//!
//! Only `EdiCheckAckStatus` is scheduled in this slice; the other three are
//! triggered by payload-bearing producers that will be added later (vendor
//! portal submit, payment-completion, inbound webhook follow-up).

use anyhow::{anyhow, Context, Result};
use billforge_core::TenantId;
use billforge_edi::{
    check_ack_timeouts, config::EdiProvider, process_inbound_ack, EdiClient, EdiConfig,
    EdiFunctionalAck, OutboundEdiService,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::WorkerConfig;

/// SQL the scheduler uses to filter active tenants down to those that have
/// the EDI module entitlement on their metadata row. The `enabled_modules`
/// column is a JSON array of snake_case module strings (see
/// `billforge_core::Module`). Filtering server-side keeps the scheduler from
/// pulling every tenant into the worker process just to drop most of them.
pub const EDI_TENANT_DISCOVERY_SQL: &str = "SELECT id::text FROM tenants \
    WHERE is_active = true \
    AND enabled_modules @> '[\"edi\"]'::jsonb";

/// Verifies the tenant still has the EDI module on their metadata row.
///
/// Public to the crate so the scheduler test can exercise the same logic.
///
/// A tenant whose subscription was downgraded between job enqueue and job
/// processing must not be able to drive outbound traffic through the EDI
/// middleware. This check fires before any side effect in every handler.
pub async fn tenant_has_edi(metadata_pool: &PgPool, tenant_uuid: Uuid) -> Result<bool> {
    let row: Option<Value> =
        sqlx::query_scalar("SELECT enabled_modules FROM tenants WHERE id = $1 AND is_active = true")
            .bind(tenant_uuid)
            .fetch_optional(metadata_pool)
            .await
            .context("Failed to load tenant enabled_modules")?;

    let modules = match row {
        Some(v) => v,
        None => return Ok(false),
    };

    let arr = match modules.as_array() {
        Some(a) => a,
        None => return Ok(false),
    };

    Ok(arr.iter().any(|m| m.as_str() == Some("edi")))
}

/// Loaded EDI connection settings for a tenant. Mirrors the row layout
/// `routes/edi.rs::send_remittance` reads — kept as a struct to share between
/// the remittance and ack handlers.
struct EdiConnectionRow {
    api_key: String,
    webhook_secret: String,
    provider: String,
    isa_qualifier: Option<String>,
    isa_id: Option<String>,
    api_base_url: Option<String>,
    ack_timeout_hours: i32,
}

async fn load_edi_connection(
    tenant_pool: &PgPool,
    tenant_uuid: Uuid,
) -> Result<Option<EdiConnectionRow>> {
    let row: Option<(String, String, String, Option<String>, Option<String>, Option<String>, i32)> =
        sqlx::query_as(
            r#"SELECT api_key_encrypted, webhook_secret, provider,
                      our_isa_qualifier, our_isa_id, api_base_url, ack_timeout_hours
               FROM edi_connections
               WHERE tenant_id = $1 AND is_active = true"#,
        )
        .bind(tenant_uuid)
        .fetch_optional(tenant_pool)
        .await
        .context("Failed to load EDI connection for tenant")?;

    Ok(row.map(
        |(api_key, webhook_secret, provider, isa_qualifier, isa_id, api_base_url, ack_timeout_hours)| {
            EdiConnectionRow {
                api_key,
                webhook_secret,
                provider,
                isa_qualifier,
                isa_id,
                api_base_url,
                ack_timeout_hours,
            }
        },
    ))
}

fn build_outbound_service(conn: &EdiConnectionRow) -> OutboundEdiService {
    let provider = match conn.provider.as_str() {
        "stedi" => EdiProvider::Stedi,
        "orderful" => EdiProvider::Orderful,
        "sps_commerce" => EdiProvider::SpsCommerce,
        _ => EdiProvider::Custom,
    };
    let config = EdiConfig {
        api_key: conn.api_key.clone(),
        webhook_secret: conn.webhook_secret.clone(),
        provider,
        api_base_url: conn
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://core.us.stedi.com/2023-08-01".to_string()),
        our_isa_qualifier: conn
            .isa_qualifier
            .clone()
            .unwrap_or_else(|| "ZZ".to_string()),
        our_isa_id: conn.isa_id.clone().unwrap_or_default(),
    };
    OutboundEdiService::new(EdiClient::new(config))
}

/// Audit envelope used by all four EDI handlers. Matches the
/// `workflow_audit_log` pattern other tenant-scoped jobs in this crate use
/// (see `forecast_tuning::write_tuning_audit`).
async fn write_audit(
    tenant_pool: &PgPool,
    tenant_uuid: Uuid,
    action: &str,
    metadata: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO workflow_audit_log (
            id, tenant_id, entity_type, entity_id, action,
            actor_type, old_values, new_values, metadata, created_at
        ) VALUES (
            gen_random_uuid(), $1, 'EdiJob', $2, $3,
            'system:edi_worker', NULL, NULL, $4, NOW()
        )
        "#,
    )
    .bind(tenant_uuid)
    .bind(tenant_uuid)
    .bind(action)
    .bind(&metadata)
    .execute(tenant_pool)
    .await
    .context("Failed to write EDI job audit entry")?;
    Ok(())
}

/// Process an inbound EDI document async.
///
/// In this slice the supported async-retry path is the 997 functional ack:
/// the payload carries the ack body that the webhook handler already parses,
/// and we reuse the existing `process_inbound_ack` library function. Other
/// inbound document types are still handled inline by the webhook handler;
/// this handler short-circuits for them rather than re-implementing webhook
/// orchestration in the worker.
#[derive(Debug, Deserialize)]
struct InboundPayload {
    /// EDI document id this job retries (already stored in `edi_documents`).
    document_id: Uuid,
    /// Functional ack body if this is a 997 retry. Optional so producers can
    /// enqueue document-only retries for other types without breaking.
    #[serde(default)]
    ack: Option<EdiFunctionalAck>,
}

pub async fn process_inbound(
    tenant_id: &TenantId,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let tenant_uuid = *tenant_id.as_uuid();
    let metadata_pool = config.pg_manager.metadata();

    if !tenant_has_edi(metadata_pool, tenant_uuid).await? {
        warn!(tenant_id = %tenant_id, "EDI entitlement missing for EdiProcessInbound, skipping");
        return Err(anyhow!("entitlement_denied: edi module not enabled for tenant"));
    }

    let parsed: InboundPayload = serde_json::from_value(payload.clone())
        .context("Invalid EdiProcessInbound payload")?;

    let tenant_pool = config.pg_manager.tenant(tenant_id).await?;

    match parsed.ack {
        Some(ack) => {
            let matched = process_inbound_ack(&tenant_pool, tenant_uuid, &ack)
                .await
                .context("process_inbound_ack failed")?;
            info!(
                tenant_id = %tenant_id,
                document_id = %parsed.document_id,
                matched_doc_id = ?matched,
                "EdiProcessInbound 997 processed",
            );
            write_audit(
                &tenant_pool,
                tenant_uuid,
                "edi.process_inbound.ack",
                json!({
                    "document_id": parsed.document_id,
                    "matched_doc_id": matched,
                    "group_control": ack.group_control,
                }),
            )
            .await?;
        }
        None => {
            // Document-only retries: confirm the row exists for the tenant
            // and re-mark it for reprocessing by leaving status=processing.
            // The webhook handler is the source of truth for non-997 inbound
            // mapping; this is a guarded no-side-effect retry to avoid
            // duplicating that logic in the worker (parked crate).
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM edi_documents WHERE id = $1 AND tenant_id = $2)",
            )
            .bind(parsed.document_id)
            .bind(tenant_uuid)
            .fetch_one(&*tenant_pool)
            .await
            .context("Failed to verify edi_document for retry")?;
            if !exists {
                return Err(anyhow!(
                    "edi_document {} not found for tenant {}",
                    parsed.document_id,
                    tenant_id
                ));
            }
            info!(
                tenant_id = %tenant_id,
                document_id = %parsed.document_id,
                "EdiProcessInbound document-only retry acknowledged",
            );
            write_audit(
                &tenant_pool,
                tenant_uuid,
                "edi.process_inbound.acknowledged",
                json!({ "document_id": parsed.document_id }),
            )
            .await?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RemittancePayload {
    invoice_id: Uuid,
    payment_reference: String,
    #[serde(default)]
    payment_method: Option<String>,
    #[serde(default)]
    payer_name: Option<String>,
}

pub async fn send_remittance(
    tenant_id: &TenantId,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let tenant_uuid = *tenant_id.as_uuid();
    let metadata_pool = config.pg_manager.metadata();

    if !tenant_has_edi(metadata_pool, tenant_uuid).await? {
        warn!(tenant_id = %tenant_id, "EDI entitlement missing for EdiSendRemittance, skipping");
        return Err(anyhow!("entitlement_denied: edi module not enabled for tenant"));
    }

    let parsed: RemittancePayload = serde_json::from_value(payload.clone())
        .context("Invalid EdiSendRemittance payload")?;

    let tenant_pool = config.pg_manager.tenant(tenant_id).await?;

    use billforge_core::domain::InvoiceId;
    use billforge_core::traits::InvoiceRepository;
    use billforge_db::repositories::InvoiceRepositoryImpl;

    let invoice_repo = InvoiceRepositoryImpl::new(Arc::clone(&tenant_pool));
    let invoice = invoice_repo
        .get_by_id(tenant_id, &InvoiceId(parsed.invoice_id))
        .await
        .context("Failed to load invoice for remittance")?
        .ok_or_else(|| anyhow!("invoice {} not found", parsed.invoice_id))?;

    let vendor_id = invoice
        .vendor_id
        .ok_or_else(|| anyhow!("invoice has no vendor_id; cannot route remittance"))?;

    let partner_edi_id: Option<String> = sqlx::query_scalar(
        "SELECT edi_id FROM edi_trading_partners \
         WHERE tenant_id = $1 AND vendor_id = $2 AND is_active = true LIMIT 1",
    )
    .bind(tenant_uuid)
    .bind(vendor_id)
    .fetch_optional(&*tenant_pool)
    .await
    .context("Failed to look up EDI trading partner")?;

    let receiver_id = partner_edi_id
        .ok_or_else(|| anyhow!("no EDI trading partner mapped for vendor {}", vendor_id))?;

    let conn = load_edi_connection(&tenant_pool, tenant_uuid)
        .await?
        .ok_or_else(|| anyhow!("no active EDI connection for tenant"))?;

    let sender_id = conn.isa_id.clone().unwrap_or_default();
    let ack_timeout_hours = conn.ack_timeout_hours;
    let service = build_outbound_service(&conn);

    let payment_method = parsed.payment_method.as_deref().unwrap_or("ACH");
    let payer_name = parsed.payer_name.as_deref().unwrap_or("BillForge");

    let doc_id = service
        .send_remittance(
            &tenant_pool,
            tenant_uuid,
            &invoice,
            &sender_id,
            &receiver_id,
            &parsed.payment_reference,
            payment_method,
            payer_name,
            ack_timeout_hours,
        )
        .await
        .context("OutboundEdiService::send_remittance failed")?;

    info!(
        tenant_id = %tenant_id,
        invoice_id = %parsed.invoice_id,
        doc_id = %doc_id,
        "EdiSendRemittance dispatched 820",
    );
    write_audit(
        &tenant_pool,
        tenant_uuid,
        "edi.send_remittance.sent",
        json!({
            "invoice_id": parsed.invoice_id,
            "edi_document_id": doc_id,
            "receiver_id": receiver_id,
        }),
    )
    .await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct SendAckPayload {
    /// Inbound EDI document this 997 acknowledges.
    original_doc_id: Uuid,
    /// Set to false to send a rejection ack (defaults to accept).
    #[serde(default = "default_true")]
    accepted: bool,
}

fn default_true() -> bool {
    true
}

pub async fn send_ack(
    tenant_id: &TenantId,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let tenant_uuid = *tenant_id.as_uuid();
    let metadata_pool = config.pg_manager.metadata();

    if !tenant_has_edi(metadata_pool, tenant_uuid).await? {
        warn!(tenant_id = %tenant_id, "EDI entitlement missing for EdiSendAck, skipping");
        return Err(anyhow!("entitlement_denied: edi module not enabled for tenant"));
    }

    let parsed: SendAckPayload =
        serde_json::from_value(payload.clone()).context("Invalid EdiSendAck payload")?;

    let tenant_pool = config.pg_manager.tenant(tenant_id).await?;

    // Pull the source document's envelope fields so the 997 can echo the right
    // group_control and swap sender/receiver back at the partner.
    let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT group_control, sender_id, receiver_id FROM edi_documents \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(parsed.original_doc_id)
    .bind(tenant_uuid)
    .fetch_optional(&*tenant_pool)
    .await
    .context("Failed to load source edi_document for ack")?;

    let (group_control, partner_sender_id, partner_receiver_id) = row
        .ok_or_else(|| anyhow!("edi_document {} not found", parsed.original_doc_id))?;
    let group_control = group_control
        .ok_or_else(|| anyhow!("edi_document {} has no group_control", parsed.original_doc_id))?;

    let conn = load_edi_connection(&tenant_pool, tenant_uuid)
        .await?
        .ok_or_else(|| anyhow!("no active EDI connection for tenant"))?;

    // The 997 we send out reverses the inbound envelope: their sender becomes
    // our receiver and vice versa.
    let sender_id = conn.isa_id.clone().unwrap_or_default();
    let receiver_id = partner_sender_id
        .or(partner_receiver_id)
        .unwrap_or_default();
    let ack_timeout_hours = conn.ack_timeout_hours;
    let service = build_outbound_service(&conn);

    let doc_id = service
        .send_ack(
            &tenant_pool,
            tenant_uuid,
            parsed.original_doc_id,
            &group_control,
            &sender_id,
            &receiver_id,
            parsed.accepted,
            ack_timeout_hours,
        )
        .await
        .context("OutboundEdiService::send_ack failed")?;

    info!(
        tenant_id = %tenant_id,
        original_doc_id = %parsed.original_doc_id,
        doc_id = %doc_id,
        accepted = parsed.accepted,
        "EdiSendAck dispatched 997",
    );
    write_audit(
        &tenant_pool,
        tenant_uuid,
        "edi.send_ack.sent",
        json!({
            "original_doc_id": parsed.original_doc_id,
            "edi_document_id": doc_id,
            "accepted": parsed.accepted,
        }),
    )
    .await?;

    Ok(())
}

pub async fn check_ack_status(tenant_id: &TenantId, config: &WorkerConfig) -> Result<()> {
    let tenant_uuid = *tenant_id.as_uuid();
    let metadata_pool = config.pg_manager.metadata();

    if !tenant_has_edi(metadata_pool, tenant_uuid).await? {
        // Producer filters by entitlement, but the entitlement may have been
        // revoked between enqueue and processing. Treat as a no-op so a
        // downgraded tenant doesn't fill the retry queue.
        info!(tenant_id = %tenant_id, "EdiCheckAckStatus: tenant lacks EDI entitlement, skipping");
        return Ok(());
    }

    let tenant_pool = config.pg_manager.tenant(tenant_id).await?;
    let timed_out = check_ack_timeouts(&tenant_pool, tenant_uuid)
        .await
        .context("check_ack_timeouts failed")?;

    info!(
        tenant_id = %tenant_id,
        timed_out_count = timed_out.len(),
        "EdiCheckAckStatus completed",
    );
    if !timed_out.is_empty() {
        write_audit(
            &tenant_pool,
            tenant_uuid,
            "edi.check_ack_status.timeouts_detected",
            json!({
                "timed_out_count": timed_out.len(),
                "document_ids": timed_out,
            }),
        )
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_sql_filters_active_and_entitled() {
        // Sanity-check the SQL the scheduler relies on. The exact text matters
        // because Postgres' jsonb @> operator is strict about the literal.
        assert!(EDI_TENANT_DISCOVERY_SQL.contains("is_active = true"));
        assert!(EDI_TENANT_DISCOVERY_SQL.contains("enabled_modules @>"));
        assert!(EDI_TENANT_DISCOVERY_SQL.contains("[\"edi\"]"));
    }

    #[test]
    fn inbound_payload_accepts_ack_form() {
        use billforge_edi::AckStatus;
        let raw = json!({
            "document_id": "00000000-0000-0000-0000-000000000001",
            "ack": {
                "sender_id": "PARTNER",
                "receiver_id": "US",
                "group_control": "1234",
                "transaction_control": null,
                "status": "accepted",
                "errors": [],
            }
        });
        let parsed: InboundPayload = serde_json::from_value(raw).unwrap();
        let ack = parsed.ack.expect("ack");
        assert_eq!(ack.group_control, "1234");
        assert_eq!(ack.status, AckStatus::Accepted);
    }

    #[test]
    fn inbound_payload_accepts_document_only_form() {
        let raw = json!({ "document_id": "00000000-0000-0000-0000-000000000002" });
        let parsed: InboundPayload = serde_json::from_value(raw).unwrap();
        assert!(parsed.ack.is_none());
    }

    #[test]
    fn remittance_payload_parses_minimum_fields() {
        let raw = json!({
            "invoice_id": "00000000-0000-0000-0000-000000000003",
            "payment_reference": "PR-2026-001",
        });
        let parsed: RemittancePayload = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.payment_reference, "PR-2026-001");
        assert!(parsed.payment_method.is_none());
    }

    #[test]
    fn send_ack_payload_defaults_to_accepted() {
        let raw = json!({ "original_doc_id": "00000000-0000-0000-0000-000000000004" });
        let parsed: SendAckPayload = serde_json::from_value(raw).unwrap();
        assert!(parsed.accepted);
    }
}
