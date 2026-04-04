//! Outbound EDI document service
//!
//! Orchestrates sending documents to trading partners via the EDI middleware:
//! - 820 Payment Remittance Advice (after invoice payment)
//! - 997 Functional Acknowledgments (for received inbound documents)
//!
//! Each outbound document is stored in `edi_documents` with direction='outbound'
//! and tracked through the ack lifecycle.

use crate::client::{EdiClient, EdiOutboundRequest};
use crate::mapper::EdiMapper;
use crate::types::{AckStatus, EdiDocumentType, EdiFunctionalAck};
use anyhow::{Context, Result};
use billforge_core::domain::Invoice;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Sends outbound EDI documents and tracks their ack status.
pub struct OutboundEdiService {
    client: EdiClient,
}

impl OutboundEdiService {
    pub fn new(client: EdiClient) -> Self {
        Self { client }
    }

    /// Send an 820 Payment Remittance Advice for a paid invoice.
    ///
    /// 1. Generates the 820 JSON via EdiMapper
    /// 2. Stores outbound edi_document record (ack_status = pending)
    /// 3. Submits to middleware via EdiClient
    /// 4. Records middleware_id and sets ack timeout
    pub async fn send_remittance(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        invoice: &Invoice,
        sender_id: &str,
        receiver_id: &str,
        payment_reference: &str,
        payment_method: &str,
        payer_name: &str,
        ack_timeout_hours: i32,
    ) -> Result<Uuid> {
        let remittance = EdiMapper::remittance_from_invoice(
            invoice,
            sender_id,
            receiver_id,
            payment_reference,
            payment_method,
            payer_name,
        );

        let payload = serde_json::to_value(&remittance)
            .context("Failed to serialize remittance advice")?;

        let doc_id = Uuid::new_v4();
        let ack_timeout_at = Utc::now() + Duration::hours(ack_timeout_hours as i64);

        // Store outbound document
        sqlx::query(
            r#"INSERT INTO edi_documents
               (id, tenant_id, document_type, direction, interchange_control, group_control,
                sender_id, receiver_id, status, invoice_id, raw_payload, ack_status,
                ack_timeout_at, created_at)
               VALUES ($1, $2, 'remittance_820', 'outbound', $3, $4, $5, $6, 'processing',
                       $7, $8, 'pending', $9, NOW())"#,
        )
        .bind(doc_id)
        .bind(tenant_id)
        .bind(&remittance.interchange_control)
        .bind(&remittance.group_control)
        .bind(sender_id)
        .bind(receiver_id)
        .bind(invoice.id.0)
        .bind(&payload)
        .bind(ack_timeout_at)
        .execute(pool)
        .await
        .context("Failed to store outbound remittance document")?;

        // Submit to middleware
        let request = EdiOutboundRequest {
            document_type: EdiDocumentType::Remittance820,
            receiver_id: receiver_id.to_string(),
            payload,
        };

        match self.client.send_document(&request).await {
            Ok(result) => {
                // Record middleware_id and mark as sent
                sqlx::query(
                    r#"UPDATE edi_documents
                       SET status = 'sent', middleware_id = $1, processed_at = NOW()
                       WHERE id = $2"#,
                )
                .bind(&result.middleware_id)
                .bind(doc_id)
                .execute(pool)
                .await
                .context("Failed to update outbound document with middleware_id")?;

                tracing::info!(
                    doc_id = %doc_id,
                    middleware_id = %result.middleware_id,
                    invoice_id = %invoice.id,
                    "820 remittance advice sent to middleware"
                );
            }
            Err(e) => {
                // Mark as failed
                sqlx::query(
                    "UPDATE edi_documents SET status = 'failed', error_message = $1 WHERE id = $2",
                )
                .bind(e.to_string())
                .bind(doc_id)
                .execute(pool)
                .await
                .context("Failed to update outbound document error status")?;

                return Err(e.context("Failed to send remittance to middleware"));
            }
        }

        Ok(doc_id)
    }

    /// Send a 997 Functional Acknowledgment for a received inbound document.
    ///
    /// 1. Generates the 997 JSON
    /// 2. Stores outbound edi_document record linked to the original
    /// 3. Submits to middleware
    pub async fn send_ack(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        original_doc_id: Uuid,
        group_control: &str,
        sender_id: &str,
        receiver_id: &str,
        accepted: bool,
        ack_timeout_hours: i32,
    ) -> Result<Uuid> {
        let ack = EdiMapper::ack_for_document(group_control, None, sender_id, receiver_id, accepted);
        let payload =
            serde_json::to_value(&ack).context("Failed to serialize functional ack")?;

        let doc_id = Uuid::new_v4();
        let ack_timeout_at = Utc::now() + Duration::hours(ack_timeout_hours as i64);

        sqlx::query(
            r#"INSERT INTO edi_documents
               (id, tenant_id, document_type, direction, group_control,
                sender_id, receiver_id, status, raw_payload, related_document_id,
                ack_status, ack_timeout_at, created_at)
               VALUES ($1, $2, 'functional_ack_997', 'outbound', $3, $4, $5,
                       'processing', $6, $7, 'pending', $8, NOW())"#,
        )
        .bind(doc_id)
        .bind(tenant_id)
        .bind(group_control)
        .bind(sender_id)
        .bind(receiver_id)
        .bind(&payload)
        .bind(original_doc_id)
        .bind(ack_timeout_at)
        .execute(pool)
        .await
        .context("Failed to store outbound ack document")?;

        let request = EdiOutboundRequest {
            document_type: EdiDocumentType::FunctionalAck997,
            receiver_id: receiver_id.to_string(),
            payload,
        };

        match self.client.send_document(&request).await {
            Ok(result) => {
                sqlx::query(
                    r#"UPDATE edi_documents
                       SET status = 'sent', middleware_id = $1, processed_at = NOW()
                       WHERE id = $2"#,
                )
                .bind(&result.middleware_id)
                .bind(doc_id)
                .execute(pool)
                .await
                .context("Failed to update outbound ack with middleware_id")?;

                tracing::info!(
                    doc_id = %doc_id,
                    original_doc_id = %original_doc_id,
                    "997 functional ack sent to middleware"
                );
            }
            Err(e) => {
                sqlx::query(
                    "UPDATE edi_documents SET status = 'failed', error_message = $1 WHERE id = $2",
                )
                .bind(e.to_string())
                .bind(doc_id)
                .execute(pool)
                .await
                .context("Failed to update outbound ack error status")?;

                return Err(e.context("Failed to send ack to middleware"));
            }
        }

        Ok(doc_id)
    }
}

/// Process an inbound 997 functional acknowledgment.
///
/// Finds the outbound document that this ack references (by group_control),
/// updates its ack_status, and optionally enqueues a retry if rejected.
pub async fn process_inbound_ack(
    pool: &PgPool,
    tenant_id: Uuid,
    ack: &EdiFunctionalAck,
) -> Result<Option<Uuid>> {
    // Find the outbound document this ack references
    let matched_doc: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT id, document_type FROM edi_documents
           WHERE tenant_id = $1
             AND direction = 'outbound'
             AND group_control = $2
             AND ack_status = 'pending'
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(tenant_id)
    .bind(&ack.group_control)
    .fetch_optional(pool)
    .await
    .context("Failed to look up outbound document for ack")?;

    let (doc_id, doc_type) = match matched_doc {
        Some(row) => row,
        None => {
            tracing::warn!(
                group_control = %ack.group_control,
                "No pending outbound document found for inbound 997"
            );
            return Ok(None);
        }
    };

    let ack_status_str = match ack.status {
        AckStatus::Accepted => "accepted",
        AckStatus::AcceptedWithErrors => "accepted_with_errors",
        AckStatus::Rejected => "rejected",
        AckStatus::Pending => "pending",
    };

    let error_msg = if ack.errors.is_empty() {
        None
    } else {
        Some(
            ack.errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.description))
                .collect::<Vec<_>>()
                .join("; "),
        )
    };

    sqlx::query(
        r#"UPDATE edi_documents
           SET ack_status = $1, ack_received_at = NOW(), error_message = COALESCE($2, error_message)
           WHERE id = $3"#,
    )
    .bind(ack_status_str)
    .bind(&error_msg)
    .bind(doc_id)
    .execute(pool)
    .await
    .context("Failed to update outbound document ack status")?;

    tracing::info!(
        doc_id = %doc_id,
        doc_type = %doc_type,
        ack_status = %ack_status_str,
        group_control = %ack.group_control,
        "Processed inbound 997 acknowledgment"
    );

    // If rejected and auto-retry is enabled, check retry count
    if ack.status == AckStatus::Rejected {
        let (retry_count, max_retries): (i32, i32) = sqlx::query_as(
            "SELECT ack_retry_count, max_ack_retries FROM edi_documents WHERE id = $1",
        )
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .context("Failed to check retry count")?;

        if retry_count < max_retries {
            // Check if auto-retry is enabled for this tenant
            let auto_retry: bool = sqlx::query_scalar(
                "SELECT COALESCE(auto_retry_on_reject, true) FROM edi_connections WHERE tenant_id = $1 AND is_active = true",
            )
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .context("Failed to check auto-retry setting")?
            .unwrap_or(false);

            if auto_retry {
                // Increment retry count and reset ack_status to pending for re-send
                sqlx::query(
                    r#"UPDATE edi_documents
                       SET ack_status = 'pending', ack_retry_count = ack_retry_count + 1,
                           ack_received_at = NULL, error_message = NULL,
                           ack_timeout_at = NOW() + INTERVAL '24 hours'
                       WHERE id = $1"#,
                )
                .bind(doc_id)
                .execute(pool)
                .await
                .context("Failed to reset document for retry")?;

                tracing::info!(
                    doc_id = %doc_id,
                    retry = retry_count + 1,
                    max = max_retries,
                    "Document queued for retry after 997 rejection"
                );
            }
        } else {
            tracing::warn!(
                doc_id = %doc_id,
                retries = retry_count,
                "Document exceeded max retries after 997 rejection"
            );
        }
    }

    Ok(Some(doc_id))
}

/// Check for outbound documents that have exceeded their ack timeout.
///
/// Returns the IDs of documents that timed out so they can be alerted on.
pub async fn check_ack_timeouts(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Uuid>> {
    let timed_out: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM edi_documents
           WHERE tenant_id = $1
             AND direction = 'outbound'
             AND ack_status = 'pending'
             AND ack_timeout_at IS NOT NULL
             AND ack_timeout_at < NOW()
             AND (last_ack_check_at IS NULL OR last_ack_check_at < NOW() - INTERVAL '1 hour')"#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .context("Failed to query ack timeouts")?;

    if !timed_out.is_empty() {
        // Update last_ack_check_at to avoid spamming alerts
        sqlx::query(
            r#"UPDATE edi_documents
               SET last_ack_check_at = NOW()
               WHERE id = ANY($1)"#,
        )
        .bind(&timed_out)
        .execute(pool)
        .await
        .context("Failed to update last_ack_check_at")?;

        tracing::warn!(
            tenant_id = %tenant_id,
            count = timed_out.len(),
            "EDI documents with overdue ack responses"
        );
    }

    Ok(timed_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EdiAckError;

    #[test]
    fn test_ack_status_matching() {
        let ack = EdiFunctionalAck {
            sender_id: "ACME-001".to_string(),
            receiver_id: "BILLFORGE-001".to_string(),
            group_control: "1234".to_string(),
            transaction_control: Some("0001".to_string()),
            status: AckStatus::Accepted,
            errors: vec![],
        };
        assert_eq!(ack.status, AckStatus::Accepted);
        assert!(ack.errors.is_empty());
    }

    #[test]
    fn test_rejected_ack_with_errors() {
        let ack = EdiFunctionalAck {
            sender_id: "ACME-001".to_string(),
            receiver_id: "BILLFORGE-001".to_string(),
            group_control: "5678".to_string(),
            transaction_control: None,
            status: AckStatus::Rejected,
            errors: vec![
                EdiAckError {
                    code: "AK501".to_string(),
                    segment_position: Some(3),
                    description: "Transaction set not supported".to_string(),
                },
            ],
        };
        assert_eq!(ack.status, AckStatus::Rejected);
        assert_eq!(ack.errors.len(), 1);

        // Verify error formatting
        let msg: String = ack.errors
            .iter()
            .map(|e| format!("{}: {}", e.code, e.description))
            .collect::<Vec<_>>()
            .join("; ");
        assert_eq!(msg, "AK501: Transaction set not supported");
    }
}
