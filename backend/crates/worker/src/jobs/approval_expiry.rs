//! Approval expiry and reminder job
//!
//! Handles two responsibilities:
//! 1. Expire stale approval requests that have passed their `expires_at` deadline
//! 2. Send reminder emails for approvals expiring within the next 24 hours

use crate::config::WorkerConfig;
use anyhow::Result;
use tracing::{error, info, warn};

/// Run approval expiry checks and send reminders for a tenant.
pub async fn process_approval_expiry(tenant_id_str: &str, config: &WorkerConfig) -> Result<()> {
    info!("Processing approval expiry for tenant: {}", tenant_id_str);

    let tenant_id = tenant_id_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid tenant ID: {}", e))?;

    let pool = config
        .pg_manager
        .tenant(&tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get tenant database: {}", e))?;

    // 1. Expire stale approval requests
    let expired_count = expire_stale_requests(&pool, tenant_id_str).await?;
    if expired_count > 0 {
        info!(
            "Expired {} stale approval requests for tenant {}",
            expired_count, tenant_id_str
        );
    }

    // 2. Send reminders for approvals expiring within 24 hours
    let reminder_count = send_expiry_reminders(&pool, tenant_id_str).await?;
    if reminder_count > 0 {
        info!(
            "Queued {} expiry reminder emails for tenant {}",
            reminder_count, tenant_id_str
        );
    }

    Ok(())
}

/// Mark approval requests as expired when they have passed their `expires_at` deadline.
async fn expire_stale_requests(pool: &sqlx::PgPool, tenant_id: &str) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE approval_requests
        SET status = 'expired', updated_at = NOW()
        WHERE tenant_id = $1
          AND status = 'pending'
          AND expires_at IS NOT NULL
          AND expires_at < NOW()
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await?;

    let count = result.rows_affected();

    // For each expired request, check if the invoice's approval status should be resolved.
    // Expired requests are treated like rejections for aggregation purposes.
    if count > 0 {
        // Get distinct invoice IDs from the just-expired requests
        let invoice_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT invoice_id
            FROM approval_requests
            WHERE tenant_id = $1
              AND status = 'expired'
              AND updated_at > NOW() - INTERVAL '1 minute'
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        for invoice_id in invoice_ids {
            // Log the expiry for audit
            if let Err(e) = sqlx::query(
                r#"
                INSERT INTO workflow_audit_log (
                    id, tenant_id, entity_type, entity_id, action,
                    actor_type, metadata, created_at
                ) VALUES (
                    gen_random_uuid(), $1, 'ApprovalRequest', $2, 'expired',
                    'system', '{"reason": "approval_request_expired"}'::jsonb, NOW()
                )
                "#,
            )
            .bind(tenant_id)
            .bind(invoice_id)
            .execute(pool)
            .await
            {
                error!(
                    "Failed to log approval expiry audit for invoice {}: {}",
                    invoice_id, e
                );
            }
        }
    }

    Ok(count)
}

/// Queue reminder emails for approval requests expiring within 24 hours
/// that haven't already been reminded.
async fn send_expiry_reminders(pool: &sqlx::PgPool, tenant_id: &str) -> Result<u64> {
    // Find approval requests expiring in the next 24 hours that are still pending.
    // We use metadata to track whether a reminder was already sent to avoid duplicates.
    let expiring_requests: Vec<(uuid::Uuid, uuid::Uuid, String)> = sqlx::query_as(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            COALESCE(ar.requested_from->>'User', '') as approver_id
        FROM approval_requests ar
        WHERE ar.tenant_id = $1
          AND ar.status = 'pending'
          AND ar.expires_at IS NOT NULL
          AND ar.expires_at > NOW()
          AND ar.expires_at < NOW() + INTERVAL '24 hours'
          AND NOT EXISTS (
            SELECT 1 FROM email_notifications en
            WHERE en.tenant_id = $1
              AND en.metadata->>'approval_request_id' = ar.id::text
              AND en.metadata->>'type' = 'expiry_reminder'
          )
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    if expiring_requests.is_empty() {
        return Ok(0);
    }

    let mut queued = 0u64;

    for (request_id, invoice_id, approver_id) in &expiring_requests {
        if approver_id.is_empty() {
            continue;
        }

        // Look up the approver's email
        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM users WHERE id = $1 AND tenant_id = $2")
                .bind(uuid::Uuid::parse_str(approver_id).unwrap_or_default())
                .bind(tenant_id)
                .fetch_optional(pool)
                .await?;

        let Some(email) = email else {
            warn!(
                "No email found for approver {} on request {}",
                approver_id, request_id
            );
            continue;
        };

        // Look up invoice details for the reminder
        let invoice_number: Option<String> = sqlx::query_scalar(
            "SELECT invoice_number FROM invoices WHERE id = $1 AND tenant_id = $2",
        )
        .bind(invoice_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;

        let invoice_number = invoice_number.unwrap_or_else(|| "N/A".to_string());

        let subject = format!(
            "Reminder: Approval expiring soon for Invoice {}",
            invoice_number
        );
        let html_body = format!(
            r#"<p>Your approval for <strong>Invoice {}</strong> is expiring soon.</p>
            <p>Please review and take action before the deadline.</p>
            <p><a href="{}/invoices/{}">View Invoice</a></p>"#,
            invoice_number,
            std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            invoice_id
        );
        let text_body = format!(
            "Reminder: Your approval for Invoice {} is expiring soon. Please take action.",
            invoice_number
        );

        // Queue the reminder email
        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO email_notifications (
                id, tenant_id, recipient_email, subject, html_body, text_body,
                status, priority, metadata, created_at
            ) VALUES (
                gen_random_uuid(), $1, $2, $3, $4, $5,
                'pending', 5, $6, NOW()
            )
            "#,
        )
        .bind(tenant_id)
        .bind(&email)
        .bind(&subject)
        .bind(&html_body)
        .bind(&text_body)
        .bind(serde_json::json!({
            "type": "expiry_reminder",
            "approval_request_id": request_id.to_string(),
            "invoice_id": invoice_id.to_string(),
        }))
        .execute(pool)
        .await
        {
            error!(
                "Failed to queue expiry reminder for request {}: {}",
                request_id, e
            );
        } else {
            queued += 1;
        }
    }

    Ok(queued)
}
