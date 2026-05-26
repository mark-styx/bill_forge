//! Report digest generation and sending job

use crate::config::WorkerConfig;
use anyhow::Result;
use billforge_email::EmailService;
use billforge_reporting::{DigestType, ReportingService};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use tracing::{error, info, warn};
use uuid::Uuid;

pub async fn send_digests(tenant_id_str: &str, config: &WorkerConfig) -> Result<()> {
    info!("Processing report digests for tenant: {}", tenant_id_str);

    let tenant_id = tenant_id_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid tenant ID: {}", e))?;

    let pool = config
        .pg_manager
        .tenant(&tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get tenant database: {}", e))?;

    let reporting_service = ReportingService::new();

    let email_config = match billforge_email::EmailConfig::from_env() {
        Some(cfg) => cfg,
        None => {
            warn!("Email service not configured, skipping digests");
            return Ok(());
        }
    };

    let email_service = billforge_email::EmailServiceImpl::new(email_config)?;

    if !email_service.is_enabled() {
        info!("Email service disabled, skipping digests");
        return Ok(());
    }

    let due_digests = reporting_service.get_due_digests(&tenant_id, &pool).await?;

    if due_digests.is_empty() {
        info!("No due digests for tenant: {}", tenant_id_str);
        return Ok(());
    }

    info!(
        "Processing {} due digests for tenant: {}",
        due_digests.len(),
        tenant_id_str
    );

    let mut sent_count = 0;
    let mut failed_count = 0;

    for digest in due_digests {
        match process_digest(
            &reporting_service,
            &email_service,
            &pool,
            &digest,
            tenant_id_str,
        )
        .await
        {
            Ok(_) => {
                mark_digest_sent(&pool, digest.id).await?;
                sent_count += 1;
            }
            Err(e) => {
                error!("Failed to send digest {}: {}", digest.id, e);
                failed_count += 1;
            }
        }
    }

    info!(
        "Digest processing complete for tenant: {} - sent: {}, failed: {}",
        tenant_id_str, sent_count, failed_count
    );

    Ok(())
}

async fn process_digest(
    reporting_service: &ReportingService,
    email_service: &billforge_email::EmailServiceImpl,
    pool: &sqlx::PgPool,
    digest: &billforge_reporting::ReportDigest,
    tenant_id_str: &str,
) -> Result<()> {
    let (period_start, period_end) = calculate_period(&digest.digest_type);

    let tenant_id = tenant_id_str.parse::<billforge_core::types::TenantId>()?;
    let pool_arc = std::sync::Arc::new(pool.clone());
    let content = reporting_service
        .generate_digest_content(
            &tenant_id,
            &pool_arc,
            digest.user_id,
            digest.digest_type.clone(),
            period_start,
            period_end,
        )
        .await?;

    let user_email = get_user_email(pool, digest.user_id).await?;

    let (html_body, text_body) = generate_digest_email(&content, tenant_id_str);

    let subject = match digest.digest_type {
        DigestType::DailySummary => format!("Daily Summary - {}", period_end),
        DigestType::WeeklySummary => format!("Weekly Summary - {} to {}", period_start, period_end),
        DigestType::MonthlySummary => format!("Monthly Summary - {}", period_end.format("%B %Y")),
        DigestType::ApprovalReminder => "Pending Approvals Reminder".to_string(),
    };

    use billforge_email::EmailService;
    email_service
        .send(&user_email, &subject, &html_body, &text_body)
        .await?;

    info!(
        "Sent {:?} digest to {} for user {}",
        digest.digest_type, user_email, digest.user_id
    );

    Ok(())
}

fn calculate_period(digest_type: &DigestType) -> (NaiveDate, NaiveDate) {
    let today = Utc::now().naive_utc().date();

    match digest_type {
        DigestType::DailySummary => (today - Duration::days(1), today - Duration::days(1)),
        DigestType::WeeklySummary => {
            let end = today - Duration::days(1);
            let start = end - Duration::days(6);
            (start, end)
        }
        DigestType::MonthlySummary => {
            let end = today - Duration::days(1);
            let start = NaiveDate::from_ymd_opt(end.year(), end.month(), 1).unwrap_or(end);
            (start, end)
        }
        DigestType::ApprovalReminder => (today - Duration::days(7), today),
    }
}

async fn get_user_email(pool: &sqlx::PgPool, user_id: Uuid) -> Result<String> {
    #[derive(sqlx::FromRow)]
    struct UserRow {
        email: String,
    }

    let row = sqlx::query_as::<_, UserRow>(r#"SELECT email FROM users WHERE id = $1"#)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(row.email)
}

async fn mark_digest_sent(pool: &sqlx::PgPool, digest_id: Uuid) -> Result<()> {
    let now = Utc::now();

    sqlx::query(
        r#"
        UPDATE report_digests
        SET last_sent_at = $1,
            next_send_at = CASE frequency
                WHEN 'daily' THEN NOW() + INTERVAL '1 day'
                WHEN 'weekly' THEN NOW() + INTERVAL '7 days'
                WHEN 'monthly' THEN NOW() + INTERVAL '30 days'
            END,
            updated_at = $1
        WHERE id = $2
        "#,
    )
    .bind(now)
    .bind(digest_id)
    .execute(pool)
    .await?;

    Ok(())
}

fn generate_digest_email(
    content: &billforge_reporting::DigestContent,
    _tenant_id: &str,
) -> (String, String) {
    let period_str = match content.digest_type {
        DigestType::DailySummary => format!("Daily Report for {}", content.period_end),
        DigestType::WeeklySummary => format!(
            "Weekly Report ({} to {})",
            content.period_start, content.period_end
        ),
        DigestType::MonthlySummary => {
            format!("Monthly Report for {}", content.period_end.format("%B %Y"))
        }
        DigestType::ApprovalReminder => "Pending Approvals".to_string(),
    };

    let mut highlights_html = String::new();
    for highlight in &content.highlights {
        let value_str = highlight
            .value
            .map(|v| format!("${:.2}", v))
            .unwrap_or_default();
        let change_str = highlight
            .change_percentage
            .map(|p| {
                if p > 0.0 {
                    format!("<span style=\"color: green;\">↑ {:.1}%</span>", p.abs())
                } else {
                    format!("<span style=\"color: red;\">↓ {:.1}%</span>", p.abs())
                }
            })
            .unwrap_or_default();

        highlights_html.push_str(&format!(
            r#"<div class="info-row">
                <span class="info-label">{}</span>
                <span class="info-value">{} {}</span>
            </div>"#,
            highlight.title, value_str, change_str
        ));
    }

    let mut items_html = String::new();
    for item in &content.actionable_items {
        let priority_color = match item.priority.as_str() {
            "high" => "#ef4444",
            "normal" => "#2563eb",
            _ => "#6b7280",
        };

        items_html.push_str(&format!(
            r#"<div class="info-box">
                <div style="margin-bottom: 10px;">
                    <span style="color: {}; font-weight: 500;">[{}]</span>
                    <strong>{}</strong>
                </div>
                <p style="margin: 0; color: #6b7280;">{}</p>
            </div>"#,
            priority_color,
            item.priority.to_uppercase(),
            item.title,
            item.description
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 600px;
            margin: 0 auto;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        .container {{
            background-color: #ffffff;
            border-radius: 8px;
            padding: 30px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
        }}
        .logo {{
            font-size: 24px;
            font-weight: bold;
            color: #2563eb;
        }}
        h1 {{
            color: #1f2937;
            font-size: 20px;
            margin-bottom: 20px;
        }}
        .info-box {{
            background-color: #f3f4f6;
            border-radius: 6px;
            padding: 15px;
            margin: 15px 0;
        }}
        .info-row {{
            display: flex;
            justify-content: space-between;
            padding: 8px 0;
        }}
        .info-label {{
            color: #6b7280;
        }}
        .info-value {{
            font-weight: 500;
        }}
        .footer {{
            margin-top: 30px;
            padding-top: 20px;
            border-top: 1px solid #e5e7eb;
            text-align: center;
            font-size: 12px;
            color: #6b7280;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div class="logo">BillForge</div>
        </div>
        <h1>{}</h1>
        <div class="info-box">
            <h3 style="margin-top: 0;">Summary</h3>
            <div class="info-row">
                <span class="info-label">Total Invoices:</span>
                <span class="info-value">{}</span>
            </div>
            <div class="info-row">
                <span class="info-label">Total Amount:</span>
                <span class="info-value">${:.2}</span>
            </div>
            <div class="info-row">
                <span class="info-label">Pending Approvals:</span>
                <span class="info-value">{}</span>
            </div>
            <div class="info-row">
                <span class="info-label">Avg Processing Time:</span>
                <span class="info-value">{:.1} hours</span>
            </div>
        </div>
        {}
        {}
        <div class="footer">
            <p>This email was sent by BillForge.</p>
        </div>
    </div>
</body>
</html>"#,
        period_str,
        period_str,
        content.summary.total_invoices,
        content.summary.total_amount,
        content.summary.pending_approvals,
        content.summary.avg_processing_time_hours,
        if !content.highlights.is_empty() {
            format!(
                "<div class=\"info-box\"><h3 style=\"margin-top: 0;\">Highlights</h3>{}</div>",
                highlights_html
            )
        } else {
            String::new()
        },
        if !content.actionable_items.is_empty() {
            format!(
                "<h3>Action Required</h3><p>You have {} items pending your review:</p>{}",
                content.actionable_items.len(),
                items_html
            )
        } else {
            String::new()
        },
    );

    let text = format!(
        r#"{} - BillForge

Summary:
- Total Invoices: {}
- Total Amount: ${:.2}
- Pending Approvals: {}
- Approved: {}
- Rejected: {}
- Avg Processing Time: {:.1} hours

{}

{}

---
This email was sent by BillForge."#,
        period_str,
        content.summary.total_invoices,
        content.summary.total_amount,
        content.summary.pending_approvals,
        content.summary.approved_count,
        content.summary.rejected_count,
        content.summary.avg_processing_time_hours,
        if !content.highlights.is_empty() {
            format!(
                "Highlights:\n{}",
                content
                    .highlights
                    .iter()
                    .map(|h| { format!("- {}: {}", h.title, h.description) })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        },
        if !content.actionable_items.is_empty() {
            format!(
                "Action Required:\nYou have {} items pending your review",
                content.actionable_items.len()
            )
        } else {
            String::new()
        },
    );

    (html, text)
}
