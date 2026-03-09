//! Email batch sending job

use crate::config::WorkerConfig;
use anyhow::Result;
use billforge_email::EmailService;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{info, warn, error};
use uuid::Uuid;
use chrono::Utc;

pub async fn send_batch(tenant_id: &str, _payload: &Value, config: &WorkerConfig) -> Result<()> {
    info!("Sending email batch for tenant: {}", tenant_id);

    // Create email service from environment
    let email_config = match billforge_email::EmailConfig::from_env() {
        Some(cfg) => cfg,
        None => {
            warn!("Email service not configured, skipping batch");
            return Ok(());
        }
    };

    let email_service = billforge_email::EmailServiceImpl::new(email_config)?;

    if !email_service.is_enabled() {
        info!("Email service disabled, skipping batch");
        return Ok(());
    }

    // Connect to database
    let pool = PgPool::connect(&config.database_url).await?;

    // Load pending email notifications
    let pending_emails = load_pending_emails(&pool, tenant_id).await?;

    if pending_emails.is_empty() {
        info!("No pending emails for tenant: {}", tenant_id);
        return Ok(());
    }

    info!("Loaded {} pending emails for tenant: {}", pending_emails.len(), tenant_id);

    // Group by recipient for batching
    let grouped = group_by_recipient(pending_emails);

    // Send each batch
    let mut sent_count = 0;
    let mut failed_count = 0;

    for (recipient_email, emails) in grouped {
        // Extract IDs before sending (for error handling)
        let email_ids = extract_ids(&emails);

        match send_email_batch(&email_service, recipient_email.clone(), emails).await {
            Ok(sent_ids) => {
                // Mark as sent
                mark_as_sent(&pool, &sent_ids).await?;
                sent_count += sent_ids.len();
            }
            Err(e) => {
                error!("Failed to send batch to {}: {}", recipient_email, e);
                // Mark as failed
                mark_as_failed(&pool, &email_ids, &e.to_string()).await?;
                failed_count += email_ids.len();
            }
        }
    }

    pool.close().await;

    info!(
        "Email batch complete for tenant: {} - sent: {}, failed: {}",
        tenant_id, sent_count, failed_count
    );

    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingEmail {
    id: Uuid,
    recipient_email: String,
    recipient_name: Option<String>,
    subject: String,
    html_body: String,
    text_body: String,
}

async fn load_pending_emails(pool: &PgPool, tenant_id: &str) -> Result<Vec<PendingEmail>> {
    let rows = sqlx::query_as::<_, PendingEmail>(
        r#"
        SELECT
            id,
            recipient_email,
            recipient_name,
            subject,
            html_body,
            text_body
        FROM email_notifications
        WHERE tenant_id = $1
          AND status = 'pending'
          AND (expires_at IS NULL OR expires_at > NOW())
        ORDER BY priority DESC, created_at ASC
        LIMIT 100
        "#
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

fn group_by_recipient(emails: Vec<PendingEmail>) -> HashMap<String, Vec<PendingEmail>> {
    let mut grouped: HashMap<String, Vec<PendingEmail>> = HashMap::new();

    for email in emails {
        grouped
            .entry(email.recipient_email.clone())
            .or_insert_with(Vec::new)
            .push(email);
    }

    grouped
}

async fn send_email_batch(
    email_service: &billforge_email::EmailServiceImpl,
    _recipient_email: String,
    emails: Vec<PendingEmail>,
) -> Result<Vec<Uuid>> {
    use billforge_email::EmailService;
    // For now, send each email individually
    // In the future, we could combine multiple notifications into a digest
    let mut sent_ids = Vec::new();
    let total_count = emails.len();

    for email in emails {
        match email_service.send(&email.recipient_email, &email.subject, &email.html_body, &email.text_body).await {
            Ok(_) => {
                sent_ids.push(email.id);
            }
            Err(e) => {
                // Log error but continue with remaining emails
                error!("Failed to send email {}: {}", email.id, e);
            }
        }
    }

    if sent_ids.is_empty() && total_count > 0 {
        // All failed
        anyhow::bail!("All emails failed to send");
    }

    Ok(sent_ids)
}

async fn mark_as_sent(pool: &PgPool, ids: &[Uuid]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE email_notifications
        SET status = 'sent',
            sent_at = $1,
            updated_at = $1
        WHERE id = ANY($2)
        "#
    )
    .bind(now)
    .bind(ids)
    .execute(pool)
    .await?;

    Ok(())
}

async fn mark_as_failed(pool: &PgPool, ids: &[Uuid], error_message: &str) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE email_notifications
        SET status = 'failed',
            error_message = $1,
            updated_at = $2
        WHERE id = ANY($3)
        "#
    )
    .bind(error_message)
    .bind(now)
    .bind(ids)
    .execute(pool)
    .await?;

    Ok(())
}

fn extract_ids(emails: &[PendingEmail]) -> Vec<Uuid> {
    emails.iter().map(|e| e.id).collect()
}
