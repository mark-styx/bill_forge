//! Email action routes - handle secure email-based actions
//!
//! These routes allow users to perform actions (approve/reject) via email links
//! using secure time-limited tokens. Supports delegation fallback: if the
//! token's user has an active delegation, the delegate's approval request
//! is also checked.

use crate::error::ApiResult;
use crate::state::AppState;
use axum::{
    extract::{Form, Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    services::{EmailAction, EmailActionTokenService},
    traits::InvoiceRepository,
    UserId,
};
use serde::Deserialize;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approve", get(handle_approve).post(handle_approve_confirm))
        .route("/reject", get(handle_reject).post(handle_reject_confirm))
        .route("/hold", get(handle_hold).post(handle_hold_confirm))
        .route(
            "/request_info",
            get(handle_request_info).post(handle_request_info_confirm),
        )
        .route("/view", get(handle_view))
}

#[derive(Debug, Deserialize)]
pub struct ActionQuery {
    /// The email action token
    t: String,
}

/// Form body for POST confirm actions
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ActionForm {
    /// The email action token
    t: String,
}

/// Form body for POST confirm actions that carry a reason (e.g. request info)
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ActionFormWithReason {
    /// The email action token
    t: String,
    /// The reason / question posed back to the submitter
    #[serde(default)]
    reason: String,
}

/// Handle approve action from email - renders confirmation interstitial (GET)
#[utoipa::path(get, path = "/api/v1/actions/approve", tag = "Email Actions",
    params(("t" = String, Query, description = "Action token")),
    responses((status = 200, description = "Renders confirmation page"), (status = 400, description = "Invalid token")))]
async fn handle_approve(
    State(_state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Html(render_confirmation_page(
        &query.t,
        EmailAction::ApproveInvoice,
    )))
}

/// Handle reject action from email - renders confirmation interstitial (GET)
#[utoipa::path(get, path = "/api/v1/actions/reject", tag = "Email Actions",
    params(("t" = String, Query, description = "Action token")),
    responses((status = 200, description = "Renders confirmation page"), (status = 400, description = "Invalid token")))]
async fn handle_reject(
    State(_state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Html(render_confirmation_page(
        &query.t,
        EmailAction::RejectInvoice,
    )))
}

/// Handle hold action from email - renders confirmation interstitial (GET)
#[utoipa::path(get, path = "/api/v1/actions/hold", tag = "Email Actions",
    params(("t" = String, Query, description = "Action token")),
    responses((status = 200, description = "Renders confirmation page"), (status = 400, description = "Invalid token")))]
async fn handle_hold(
    State(_state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Html(render_confirmation_page(
        &query.t,
        EmailAction::HoldInvoice,
    )))
}

/// POST confirm: approve invoice (performs the actual mutation)
#[utoipa::path(post, path = "/api/v1/actions/approve", tag = "Email Actions",
    responses((status = 200, description = "Invoice approved"), (status = 400, description = "Invalid token")))]
async fn handle_approve_confirm(
    State(state): State<AppState>,
    Form(form): Form<ActionForm>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &form.t, EmailAction::ApproveInvoice).await
}

/// POST confirm: reject invoice (performs the actual mutation)
#[utoipa::path(post, path = "/api/v1/actions/reject", tag = "Email Actions",
    responses((status = 200, description = "Invoice rejected"), (status = 400, description = "Invalid token")))]
async fn handle_reject_confirm(
    State(state): State<AppState>,
    Form(form): Form<ActionForm>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &form.t, EmailAction::RejectInvoice).await
}

/// POST confirm: hold invoice (performs the actual mutation)
#[utoipa::path(post, path = "/api/v1/actions/hold", tag = "Email Actions",
    responses((status = 200, description = "Invoice placed on hold"), (status = 400, description = "Invalid token")))]
async fn handle_hold_confirm(
    State(state): State<AppState>,
    Form(form): Form<ActionForm>,
) -> ApiResult<impl IntoResponse> {
    handle_email_action(&state, &form.t, EmailAction::HoldInvoice).await
}

/// Handle request-info action from email - renders confirmation interstitial
/// with a textarea for the approver's question (GET)
#[utoipa::path(get, path = "/api/v1/actions/request_info", tag = "Email Actions",
    params(("t" = String, Query, description = "Action token")),
    responses((status = 200, description = "Renders confirmation page"), (status = 400, description = "Invalid token")))]
async fn handle_request_info(
    State(_state): State<AppState>,
    Query(query): Query<ActionQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Html(render_confirmation_page(
        &query.t,
        EmailAction::RequestInfoInvoice,
    )))
}

/// POST confirm: request info from submitter (pauses approval, posts a question)
#[utoipa::path(post, path = "/api/v1/actions/request_info", tag = "Email Actions",
    responses((status = 200, description = "Request for info recorded"), (status = 400, description = "Invalid token")))]
async fn handle_request_info_confirm(
    State(state): State<AppState>,
    Form(form): Form<ActionFormWithReason>,
) -> ApiResult<impl IntoResponse> {
    handle_request_info_action(&state, &form.t, &form.reason).await
}

/// Handle view action from email (redirects to invoice detail page)
#[utoipa::path(get, path = "/api/v1/actions/view", tag = "Email Actions",
    params(("t" = String, Query, description = "Action token")),
    responses((status = 302, description = "Redirect to invoice")))]
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
    let action_label = match token_data.action {
        EmailAction::ApproveInvoice => {
            perform_approval(
                &tenant_pool,
                Some(&state.db.metadata()),
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?;
            "approved"
        }
        EmailAction::RejectInvoice => {
            perform_rejection(
                &tenant_pool,
                Some(&state.db.metadata()),
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?;
            "rejected"
        }
        EmailAction::HoldInvoice => {
            perform_hold(
                &tenant_pool,
                &tenant_id,
                token_data.resource_id,
                &UserId(token_data.user_id),
            )
            .await?;
            "placed on hold"
        }
        _ => {
            return Err(
                billforge_core::Error::Validation("Unsupported action type".to_string()).into(),
            )
        }
    };

    // Mark token as used
    token_service.mark_used(token_str).await?;

    // Log audit entry (SOX compliance)
    let audit_action = match token_data.action {
        EmailAction::ApproveInvoice => AuditAction::InvoiceApproved,
        EmailAction::RejectInvoice => AuditAction::InvoiceRejected,
        EmailAction::HoldInvoice => AuditAction::InvoicePutOnHold,
        _ => AuditAction::Update,
    };

    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(UserId(token_data.user_id)),
        audit_action,
        ResourceType::Invoice,
        token_data.resource_id.to_string(),
        format!("Invoice {} via email action", action_label),
    )
    .with_metadata(serde_json::json!({
        "channel": "email",
        "token_nonce": token_data.nonce.to_string(),
    }));

    super::workflows::log_audit_or_record_gap(&state.db.metadata(), audit_entry).await;

    // Return success HTML page
    let html = generate_success_page(&token_data.action, token_data.resource_id);

    Ok(Html(html))
}

/// Update an approval request for the given user, handling the actual JSONB
/// structure of `requested_from`. Supports:
/// - `{"User":"<uuid>"}` - direct user target
/// - `{"AnyOf":["<uuid>",...]}`  - any-of group target
/// - Delegation fallback: if the acting user is a delegate for the original
///   approver, the original approver's request is updated with the delegate
///   recorded in `responded_by`.
pub(crate) async fn update_approval_request(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
    new_status: &str,
) -> billforge_core::Result<()> {
    // First, try to match a direct User target or AnyOf member
    let rows_affected = sqlx::query(
        r#"UPDATE approval_requests
           SET status = $1, responded_by = $2, responded_at = NOW(), updated_at = NOW()
           WHERE tenant_id = $3 AND invoice_id = $4 AND status = 'pending'
             AND (
               requested_from->>'User' = $5
               OR (requested_from ? 'AnyOf' AND requested_from->'AnyOf' @> to_jsonb($5::text))
             )"#,
    )
    .bind(new_status)
    .bind(user_id.as_uuid())
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(user_id.as_uuid().to_string())
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?
    .rows_affected();

    if rows_affected > 0 {
        return Ok(());
    }

    // No direct match - check if this user is a delegate for someone with a
    // pending approval request on this invoice.
    let delegate_rows = sqlx::query(
        r#"UPDATE approval_requests ar
           SET status = $1, responded_by = $2, responded_at = NOW(), updated_at = NOW()
           FROM approval_delegations ad
           WHERE ar.tenant_id = $3 AND ar.invoice_id = $4 AND ar.status = 'pending'
             AND ad.tenant_id = ar.tenant_id
             AND ad.delegate_id = $2
             AND ad.is_active = true
             AND ad.start_date <= NOW()
             AND ad.end_date > NOW()
             AND (
               ar.requested_from->>'User' = ad.delegator_id::text
               OR (ar.requested_from ? 'AnyOf' AND ar.requested_from->'AnyOf' @> to_jsonb(ad.delegator_id::text))
             )"#
    )
    .bind(new_status)
    .bind(user_id.as_uuid())
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?
    .rows_affected();

    if delegate_rows > 0 {
        tracing::info!(
            user_id = %user_id,
            invoice_id = %invoice_id,
            "Approval performed via delegation fallback"
        );
    } else {
        tracing::warn!(
            user_id = %user_id,
            invoice_id = %invoice_id,
            "Email action found no matching pending approval request (direct or delegated)"
        );
    }

    Ok(())
}

/// Perform approval action
async fn perform_approval(
    pool: &sqlx::PgPool,
    metadata_pool: Option<&std::sync::Arc<sqlx::PgPool>>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
) -> billforge_core::Result<()> {
    // Period lock guard: reject approval if invoice is in a locked period
    let inv_date: Option<(Option<String>,)> =
        sqlx::query_as("SELECT invoice_date::text FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(invoice_id)
            .bind(tenant_id.as_uuid())
            .fetch_optional(pool)
            .await
            .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if let Some((Some(ref dt),)) = inv_date {
        if super::close_periods::find_locked_period_for_date(pool, tenant_id, dt)
            .await?
            .is_some()
        {
            return Err(billforge_core::Error::Conflict(
                "period_locked: Invoice belongs to a locked period and cannot be approved".into(),
            ));
        }
    }

    // Budget guardrail check: block approval if budget is exceeded
    let budget_check =
        super::budgets::check_invoice_against_budgets(pool, *tenant_id.as_uuid(), invoice_id)
            .await?;

    if budget_check.blocked {
        return Err(billforge_core::Error::Conflict(format!(
            "BUDGET_EXCEEDED: {}",
            serde_json::to_string(&budget_check.violations)
                .unwrap_or_else(|_| "budget exceeded".to_string())
        )));
    }

    // Log budget check audit entry for warnings
    if !budget_check.warnings.is_empty() {
        let _ = sqlx::query(
            "INSERT INTO invoice_audit_log \
             (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata) \
             VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'budget_check_performed', $5)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(*tenant_id.as_uuid())
        .bind(invoice_id)
        .bind(*user_id.as_uuid())
        .bind(serde_json::json!({
            "warnings": budget_check.warnings,
            "violations": budget_check.violations,
            "channel": "email",
        }))
        .execute(pool)
        .await;
    }

    update_approval_request(pool, tenant_id, invoice_id, user_id, "approved").await?;

    // Resolve invoice approval status (only transitions if ALL requests resolved)
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;
    super::workflows::resolve_invoice_approval_status(
        &mut conn,
        metadata_pool,
        tenant_id,
        invoice_id,
    )
    .await?;

    Ok(())
}

/// Perform rejection action
async fn perform_rejection(
    pool: &sqlx::PgPool,
    metadata_pool: Option<&std::sync::Arc<sqlx::PgPool>>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
) -> billforge_core::Result<()> {
    update_approval_request(pool, tenant_id, invoice_id, user_id, "rejected").await?;

    // Resolve invoice approval status (only transitions if ALL requests resolved)
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;
    super::workflows::resolve_invoice_approval_status(
        &mut conn,
        metadata_pool,
        tenant_id,
        invoice_id,
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

/// Parallel handler for request-info action. Unlike approve/reject/hold this
/// path carries a `reason` body field, so it does not share the generic
/// `ActionForm { t }` dispatcher.
async fn handle_request_info_action(
    state: &AppState,
    token_str: &str,
    reason: &str,
) -> ApiResult<impl IntoResponse> {
    let pool = state.db.metadata();
    let token_service = EmailActionTokenService::new(
        pool,
        std::env::var("TOKEN_SECRET_KEY").unwrap_or_else(|_| "secret".to_string()),
    );

    let token_data = token_service.validate_token(token_str).await?;

    if std::mem::discriminant(&token_data.action)
        != std::mem::discriminant(&EmailAction::RequestInfoInvoice)
    {
        return Err(billforge_core::Error::Validation("Token action mismatch".to_string()).into());
    }

    let tenant_uuid = uuid::Uuid::parse_str(&token_data.tenant_id)
        .map_err(|e| billforge_core::Error::Validation(format!("Invalid tenant ID: {}", e)))?;
    let tenant_id = billforge_core::TenantId(tenant_uuid);
    let tenant_pool = state.db.tenant(&tenant_id).await?;

    perform_request_info(
        &tenant_pool,
        &tenant_id,
        token_data.resource_id,
        &UserId(token_data.user_id),
        reason,
    )
    .await?;

    token_service.mark_used(token_str).await?;

    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(UserId(token_data.user_id)),
        AuditAction::Update,
        ResourceType::Invoice,
        token_data.resource_id.to_string(),
        "Requested additional information from submitter via email action".to_string(),
    )
    .with_metadata(serde_json::json!({
        "channel": "email",
        "event_type": "info_requested",
        "reason": reason,
        "token_nonce": token_data.nonce.to_string(),
    }));

    super::workflows::log_audit_or_record_gap(&state.db.metadata(), audit_entry).await;

    let html = generate_success_page(&EmailAction::RequestInfoInvoice, token_data.resource_id);
    Ok(Html(html))
}

/// Perform request-info action.
///
/// Writes an `info_requested` row to invoice_audit_log and transitions the
/// matching approval_requests row to `awaiting_info` so the approval clock
/// pauses without resolving as approve/reject. The invoice's
/// processing_status is intentionally left unchanged - the approval remains
/// open until the submitter replies (via the existing inbound-email
/// pipeline) or the approver issues a fresh approve/reject token.
pub async fn perform_request_info(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
    user_id: &UserId,
    reason: &str,
) -> billforge_core::Result<()> {
    sqlx::query(
        "INSERT INTO invoice_audit_log \
         (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata) \
         VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_info', 'info_requested', $5)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id)
    .bind(*user_id.as_uuid())
    .bind(serde_json::json!({
        "reason": reason,
        "channel": "email",
        "requested_by": user_id.as_uuid().to_string(),
    }))
    .execute(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    update_approval_request(pool, tenant_id, invoice_id, user_id, "awaiting_info").await?;

    Ok(())
}

/// Generate a success HTML page
fn generate_success_page(action: &EmailAction, invoice_id: uuid::Uuid) -> String {
    let action_text = match action {
        EmailAction::ApproveInvoice => "approved",
        EmailAction::RejectInvoice => "rejected",
        EmailAction::HoldInvoice => "placed on hold",
        EmailAction::RequestInfoInvoice => "info requested",
        _ => "processed",
    };

    let action_color = match action {
        EmailAction::ApproveInvoice => "#10b981",
        EmailAction::RejectInvoice => "#ef4444",
        EmailAction::HoldInvoice => "#f59e0b",
        EmailAction::RequestInfoInvoice => "#f59e0b",
        _ => "#6b7280",
    };

    let description = match action {
        EmailAction::RequestInfoInvoice => {
            "Request for information sent to submitter. The approval will resume once the submitter replies.".to_string()
        }
        _ => format!("The invoice has been successfully {}.", action_text),
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
        <p>{description}</p>
        <a href="{app_url}/invoices/{invoice_id}" class="button">View Invoice</a>
        <div class="footer">
            <p>This action was performed via email.</p>
        </div>
    </div>
</body>
</html>"#,
        action_text = action_text,
        action_color = action_color,
        description = description,
        app_url = app_url,
        invoice_id = invoice_id
    )
}

/// HTML-escape the minimal set of characters that could break an HTML attribute value.
fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render a confirmation interstitial page with a POST form.
/// The GET handler returns this page; the user must click the confirm button
/// to submit a POST that performs the actual mutation.
pub fn render_confirmation_page(token: &str, action: EmailAction) -> String {
    let action_path = match action {
        EmailAction::ApproveInvoice => "approve",
        EmailAction::RejectInvoice => "reject",
        EmailAction::HoldInvoice => "hold",
        EmailAction::RequestInfoInvoice => "request_info",
        _ => "approve",
    };

    let action_label = match action {
        EmailAction::ApproveInvoice => "Approve",
        EmailAction::RejectInvoice => "Reject",
        EmailAction::HoldInvoice => "Place on Hold",
        EmailAction::RequestInfoInvoice => "Request Info",
        _ => "Confirm",
    };

    let action_color = match action {
        EmailAction::ApproveInvoice => "#10b981",
        EmailAction::RejectInvoice => "#ef4444",
        EmailAction::HoldInvoice => "#f59e0b",
        EmailAction::RequestInfoInvoice => "#f59e0b",
        _ => "#6b7280",
    };

    let prompt_text = match action {
        EmailAction::RequestInfoInvoice => {
            "Type your question for the submitter, then click confirm.".to_string()
        }
        _ => format!("Click confirm to {} this invoice.", action_label.to_lowercase()),
    };

    let reason_field = match action {
        EmailAction::RequestInfoInvoice => {
            r#"<p style="text-align: left;"><label for="reason" style="display:block; margin-bottom:6px; color:#1f2937; font-weight:500;">Question for the submitter</label><textarea id="reason" name="reason" rows="4" required style="width:100%; box-sizing:border-box; padding:8px; border:1px solid #d1d5db; border-radius:6px; font-family:inherit; font-size:14px;"></textarea></p>"#.to_string()
        }
        _ => String::new(),
    };

    let escaped_token = html_escape_attr(token);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Confirm {action_label}</title>
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
            background-color: {action_color};
            color: #ffffff;
            padding: 12px 24px;
            border: none;
            border-radius: 6px;
            font-weight: 500;
            font-size: 16px;
            cursor: pointer;
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
        <div class="icon">?</div>
        <h1>Confirm {action_label}</h1>
        <p>{prompt_text}</p>
        <form method="post" action="/api/v1/actions/{action_path}">
            <input type="hidden" name="t" value="{escaped_token}" />
            {reason_field}
            <button type="submit" class="button">Confirm {action_label}</button>
        </form>
        <div class="footer">
            <p>This action was requested via email. If you did not expect this, close this page.</p>
        </div>
    </div>
</body>
</html>"#,
        action_label = action_label,
        prompt_text = prompt_text,
        reason_field = reason_field,
        action_path = action_path,
        action_color = action_color,
        escaped_token = escaped_token,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_confirmation_page(html: &str, expected_action_path: &str, token: &str) {
        let lower = html.to_lowercase();

        // Must contain a POST form
        assert!(
            lower.contains("method=\"post\""),
            "confirmation page must contain method=\"post\""
        );

        // Form must POST to the correct action path
        assert!(
            html.contains(&format!(
                "action=\"/api/v1/actions/{}\"",
                expected_action_path
            )),
            "confirmation page must contain action=\"/api/v1/actions/{}\"",
            expected_action_path
        );

        // Must contain the token in a hidden input
        assert!(
            html.contains("name=\"t\""),
            "confirmation page must contain hidden input with name=\"t\""
        );
        assert!(
            html.contains(&format!("value=\"{}\"", token)),
            "confirmation page must contain hidden input with value=\"{}\"",
            token
        );

        // Must NOT contain auto-submitting JavaScript
        assert!(
            !lower.contains("onload"),
            "confirmation page must not contain onload"
        );
        assert!(
            !lower.contains("document.forms"),
            "confirmation page must not contain document.forms"
        );
        assert!(
            !lower.contains("submit()"),
            "confirmation page must not contain submit()"
        );
    }

    #[test]
    fn test_render_confirmation_page_approve() {
        let html = render_confirmation_page("sample-token", EmailAction::ApproveInvoice);
        assert_confirmation_page(&html, "approve", "sample-token");
    }

    #[test]
    fn test_render_confirmation_page_reject() {
        let html = render_confirmation_page("sample-token", EmailAction::RejectInvoice);
        assert_confirmation_page(&html, "reject", "sample-token");
    }

    #[test]
    fn test_render_confirmation_page_hold() {
        let html = render_confirmation_page("sample-token", EmailAction::HoldInvoice);
        assert_confirmation_page(&html, "hold", "sample-token");
    }

    #[test]
    fn test_render_confirmation_page_request_info_has_reason_textarea() {
        let html = render_confirmation_page("sample-token", EmailAction::RequestInfoInvoice);
        assert_confirmation_page(&html, "request_info", "sample-token");
        assert!(
            html.contains("name=\"reason\""),
            "request_info confirmation page must contain a reason textarea: {}",
            html
        );
        assert!(
            html.contains("<textarea"),
            "request_info confirmation page must contain a textarea element"
        );
    }

    #[test]
    fn test_render_confirmation_page_html_escapes_token() {
        let html = render_confirmation_page("tok&en\"<>val", EmailAction::ApproveInvoice);
        // The token should be escaped in the hidden input value
        assert!(
            html.contains("value=\"tok&amp;en&quot;&lt;&gt;val\""),
            "token must be HTML-escaped in hidden input: {}",
            html
        );
        // Raw unescaped characters must NOT appear in the value attribute
        assert!(
            !html.contains("value=\"tok&en"),
            "unescaped & must not appear in value attribute"
        );
    }
}
