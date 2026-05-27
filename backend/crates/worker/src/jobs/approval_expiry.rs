//! Approval expiry and reminder job
//!
//! Handles two responsibilities:
//! 1. Expire stale approval requests that have passed their `expires_at` deadline
//! 2. Send reminder emails for approvals expiring within the next 24 hours
//! 3. Send one-time SLA near-breach and breached alerts

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

    // 3. Send SLA threshold alerts
    let near_breach_count = send_sla_near_breach_alerts(&pool, tenant_id_str).await?;
    let breached_count = send_sla_breach_alerts(&pool, tenant_id_str).await?;
    if near_breach_count > 0 || breached_count > 0 {
        info!(
            "Queued {} near-breach and {} breached SLA alerts for tenant {}",
            near_breach_count, breached_count, tenant_id_str
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

async fn send_sla_near_breach_alerts(pool: &sqlx::PgPool, tenant_id: &str) -> Result<u64> {
    let requests: Vec<(uuid::Uuid, uuid::Uuid, String, String, i32)> = sqlx::query_as(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            COALESCE(ar.requested_from->>'User', '') AS approver_id,
            COALESCE(i.invoice_number, 'N/A') AS invoice_number,
            COALESCE(ar.sla_hours, 24) AS sla_hours
        FROM approval_requests ar
        JOIN invoices i ON i.id = ar.invoice_id
        WHERE ar.tenant_id = $1
          AND ar.status = 'pending'
          AND ar.near_breach_notified_at IS NULL
          AND ar.breached_notified_at IS NULL
          AND EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0
              >= COALESCE(ar.sla_hours, 24) * 0.8
          AND EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0
              < COALESCE(ar.sla_hours, 24)
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    queue_sla_alerts(pool, tenant_id, requests, "sla_near_breach").await
}

async fn send_sla_breach_alerts(pool: &sqlx::PgPool, tenant_id: &str) -> Result<u64> {
    let requests: Vec<(uuid::Uuid, uuid::Uuid, String, String, i32)> = sqlx::query_as(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            COALESCE(ar.requested_from->>'User', '') AS approver_id,
            COALESCE(i.invoice_number, 'N/A') AS invoice_number,
            COALESCE(ar.sla_hours, 24) AS sla_hours
        FROM approval_requests ar
        JOIN invoices i ON i.id = ar.invoice_id
        WHERE ar.tenant_id = $1
          AND ar.status = 'pending'
          AND ar.breached_notified_at IS NULL
          AND EXTRACT(EPOCH FROM (NOW() - COALESCE(ar.sla_started_at, ar.created_at))) / 3600.0
              >= COALESCE(ar.sla_hours, 24)
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    queue_sla_alerts(pool, tenant_id, requests, "sla_breached").await
}

async fn queue_sla_alerts(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    requests: Vec<(uuid::Uuid, uuid::Uuid, String, String, i32)>,
    alert_type: &str,
) -> Result<u64> {
    if requests.is_empty() {
        return Ok(0);
    }

    let mut queued = 0u64;

    for (request_id, invoice_id, approver_id, invoice_number, sla_hours) in requests {
        if approver_id.is_empty() {
            continue;
        }

        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM users WHERE id = $1 AND tenant_id = $2")
                .bind(uuid::Uuid::parse_str(&approver_id).unwrap_or_default())
                .bind(tenant_id)
                .fetch_optional(pool)
                .await?;

        let Some(email) = email else {
            warn!(
                "No email found for SLA alert approver {} on request {}",
                approver_id, request_id
            );
            continue;
        };

        let (subject, text_body, html_body) = if alert_type == "sla_breached" {
            (
                format!(
                    "SLA breached: Invoice {} approval is overdue",
                    invoice_number
                ),
                format!(
                    "Invoice {} has breached its {}h approval SLA. Please review immediately.",
                    invoice_number, sla_hours
                ),
                format!(
                    r#"<p><strong>Invoice {}</strong> has breached its {}h approval SLA.</p>
                    <p>Please review and take action immediately.</p>
                    <p><a href="{}/invoices/{}">View Invoice</a></p>"#,
                    invoice_number,
                    sla_hours,
                    std::env::var("APP_URL")
                        .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                    invoice_id
                ),
            )
        } else {
            (
                format!(
                    "SLA warning: Invoice {} approval is near breach",
                    invoice_number
                ),
                format!(
                    "Invoice {} is nearing its {}h approval SLA. Please review soon.",
                    invoice_number, sla_hours
                ),
                format!(
                    r#"<p><strong>Invoice {}</strong> is nearing its {}h approval SLA.</p>
                    <p>Please review before the deadline.</p>
                    <p><a href="{}/invoices/{}">View Invoice</a></p>"#,
                    invoice_number,
                    sla_hours,
                    std::env::var("APP_URL")
                        .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                    invoice_id
                ),
            )
        };

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO email_notifications (
                id, tenant_id, recipient_email, subject, html_body, text_body,
                status, priority, metadata, created_at
            ) VALUES (
                gen_random_uuid(), $1, $2, $3, $4, $5,
                'pending', 7, $6, NOW()
            )
            "#,
        )
        .bind(tenant_id)
        .bind(&email)
        .bind(&subject)
        .bind(&html_body)
        .bind(&text_body)
        .bind(serde_json::json!({
            "type": alert_type,
            "approval_request_id": request_id.to_string(),
            "invoice_id": invoice_id.to_string(),
        }))
        .execute(pool)
        .await
        {
            error!(
                "Failed to queue {} alert for request {}: {}",
                alert_type, request_id, e
            );
            continue;
        }

        mark_sla_alert_sent(pool, request_id, alert_type).await?;
        queued += 1;
    }

    Ok(queued)
}

async fn mark_sla_alert_sent(
    pool: &sqlx::PgPool,
    request_id: uuid::Uuid,
    alert_type: &str,
) -> Result<()> {
    if alert_type == "sla_breached" {
        sqlx::query(
            r#"
            UPDATE approval_requests
            SET breached_notified_at = NOW(), escalated_at = COALESCE(escalated_at, NOW()), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(request_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE approval_requests
            SET near_breach_notified_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(request_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}
