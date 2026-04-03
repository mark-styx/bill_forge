//! Email action routes - handle secure email-based actions
//!
//! These routes allow users to perform actions (approve/reject) via email links
//! using secure time-limited tokens.

use crate::error::ApiResult;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use billforge_core::{
    services::{EmailAction, EmailActionTokenService},
    traits::InvoiceRepository,
    UserId,
};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approve", get(handle_approve))
        .route("/reject", get(handle_reject))
        .route("/hold", get(handle_hold))
        .route("/view", get(handle_view))
}

#[derive(Debug, Deserialize)]
pub struct ActionQuery {
    /// The email action token
    t: String,
}

/// Handle approve action from email
async fn handle_approve(
    State(state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &query.t, EmailAction::ApproveInvoice).await
}

/// Handle reject action from email
async fn handle_reject(
    State(state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &query.t, EmailAction::RejectInvoice).await
}

/// Handle hold action from email
async fn handle_hold(
    State(state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &query.t, EmailAction::HoldInvoice).await
}

/// Handle view action from email (redirects to invoice detail page)
async fn handle_view(
    State(state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.metadata();
    let token_service = EmailActionTokenService::new(
        pool,
        std::env::var("TOKEN_SECRET_KEY").unwrap_or_else(|_| "secret".to_string()),
    );

    // Validate token
    let token_data = token_service.validate_token(&query.t).await?;

    // Mark as used
    token_service.mark_used(&query.t).await?;

    // Redirect to invoice detail page
    let redirect_url = format!(
        "{}/invoices/{}",
        std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
        token_data.resource_id
    );

    Ok(Redirect::temporary(&redirect_url))
}

/// Generic handler for email actions
async fn handle_email_action(
    state: &AppState,
    token_str: &str,
    expected_action: EmailAction,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.metadata();
    let token_service = EmailActionTokenService::new(
        pool,
        std::env::var("TOKEN_SECRET_KEY").unwrap_or_else(|_| "secret".to_string()),
    );

    // Validate token
    let token_data = token_service.validate_token(token_str).await?;

    // Verify action matches
    if std::mem::discriminant(&token_data.action) != std::mem::discriminant(&expected_action) {
        return Err(billforge_core::Error::Validation("Token action mismatch".to_string()).into());
    }

    // Get tenant database pool
    let tenant_uuid = uuid::Uuid::parse_str(&token_data.tenant_id)
        .map_err(|e| billforge_core::Error::Validation(format!("Invalid tenant ID: {}", e)))?;
    let tenant_id = billforge_core::TenantId(tenant_uuid);
    let tenant_pool = state.db.tenant(&tenant_id).await?;

    // Perform the action based on type
    match token_data.action {
        EmailAction::ApproveInvoice => {
            perform_approval(
                &tenant_pool,
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?
        }
        EmailAction::RejectInvoice => {
            perform_rejection(
                &tenant_pool,
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?
        }
        EmailAction::HoldInvoice => {
            perform_hold(
                &tenant_pool,
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?
        }
        _ => {
            return Err(
                billforge_core::Error::Validation("Unsupported action type".to_string()).into(),
            )
        }
    };

    // Mark token as used
    token_service.mark_used(token_str).await?;

    // Return success HTML page
    let html = generate_success_page(&token_data.action, token_data.resource_id);

    Ok(Html(html))
}

/// Perform approval action
async fn perform_approval(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
) -> billforge_core::Result<()> {
    // Update approval request status
    sqlx::query(
        r#"UPDATE approval_requests
           SET status = 'approved', responded_by = $1, responded_at = NOW()
           WHERE invoice_id = $2 AND status = 'pending'"#,
    )
    .bind(user_id.as_uuid())
    .bind(invoice_id)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice status
    let invoice_id_typed = billforge_core::InvoiceId(invoice_id);
    let invoice_repo =
        billforge_db::repositories::InvoiceRepositoryImpl::new(std::sync::Arc::new(pool.clone()));
    invoice_repo
        .update_processing_status(
            tenant_id,
            &invoice_id_typed,
            billforge_core::domain::ProcessingStatus::Approved,
        )
        .await?;

    Ok(())
}

/// Perform rejection action
async fn perform_rejection(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
) -> billforge_core::Result<()> {
    // Update approval request status
    sqlx::query(
        r#"UPDATE approval_requests
           SET status = 'rejected', responded_by = $1, responded_at = NOW()
           WHERE invoice_id = $2 AND status = 'pending'"#,
    )
    .bind(user_id.as_uuid())
    .bind(invoice_id)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice status
    let invoice_id_typed = billforge_core::InvoiceId(invoice_id);
    let invoice_repo =
        billforge_db::repositories::InvoiceRepositoryImpl::new(std::sync::Arc::new(pool.clone()));
    invoice_repo
        .update_processing_status(
            tenant_id,
            &invoice_id_typed,
            billforge_core::domain::ProcessingStatus::Rejected,
        )
        .await?;

    Ok(())
}

/// Perform hold action
async fn perform_hold(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    _user_id: &UserId,
) -> billforge_core::Result<()> {
    // Update invoice status to on-hold
    let invoice_id_typed = billforge_core::InvoiceId(invoice_id);
    let invoice_repo =
        billforge_db::repositories::InvoiceRepositoryImpl::new(std::sync::Arc::new(pool.clone()));
    invoice_repo
        .update_processing_status(
            tenant_id,
            &invoice_id_typed,
            billforge_core::domain::ProcessingStatus::OnHold,
        )
        .await?;

    Ok(())
}

/// Generate a success HTML page
fn generate_success_page(action: &EmailAction, invoice_id: uuid::Uuid) -> String {
    let action_text = match action {
        EmailAction::ApproveInvoice => "approved",
        EmailAction::RejectInvoice => "rejected",
        EmailAction::HoldInvoice => "placed on hold",
        _ => "processed",
    };

    let action_color = match action {
        EmailAction::ApproveInvoice => "#10b981",
        EmailAction::RejectInvoice => "#ef4444",
        EmailAction::HoldInvoice => "#f59e0b",
        _ => "#6b7280",
    };

    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Invoice {action_text}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
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
            padding: 40px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            text-align: center;
        }}
        .icon {{
            width: 60px;
            height: 60px;
            border-radius: 50%;
            background-color: {action_color};
            display: flex;
            align-items: center;
            justify-content: center;
            margin: 0 auto 20px;
            color: white;
            font-size: 30px;
        }}
        h1 {{
            color: #1f2937;
            font-size: 24px;
            margin-bottom: 10px;
        }}
        p {{
            color: #6b7280;
            margin-bottom: 30px;
        }}
        .button {{
            display: inline-block;
            background-color: #2563eb;
            color: #ffffff;
            padding: 12px 24px;
            text-decoration: none;
            border-radius: 6px;
            font-weight: 500;
        }}
        .footer {{
            margin-top: 30px;
            padding-top: 20px;
            border-top: 1px solid #e5e7eb;
            font-size: 12px;
            color: #9ca3af;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="icon">✓</div>
        <h1>Invoice {action_text}</h1>
        <p>The invoice has been successfully {action_text}.</p>
        <a href="{app_url}/invoices/{invoice_id}" class="button">View Invoice</a>
        <div class="footer">
            <p>This action was performed via email.</p>
        </div>
    </div>
</body>
</html>"#,
        action_text = action_text,
        action_color = action_color,
        app_url = app_url,
        invoice_id = invoice_id
    )
}
