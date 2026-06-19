//! Signed magic-link token endpoints for email-based approval flow.
//!
//! Approvers can approve, reject, or comment on invoices directly from an email
//! link without logging into the portal. Every action is recorded to the
//! `invoice_audit_log` table for a full audit trail.
//!
//! # Token format
//! Uses HS256 JWTs (via `jsonwebtoken`) with a 7-day TTL.  Each token carries a
//! unique `jti` (JWT ID) which is checked against a single-use store to prevent
//! replay.
//!
//! # TODO
//! - Secret rotation (currently uses a single `TOKEN_SECRET_KEY` env var).
//! - Rate limiting on these endpoints.

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::notifications::sms::{self, SmsChannel, SmsProvider};
use crate::state::AppState;
use crate::state_machine::{transition, InvoiceStatus};
use billforge_core::UserId;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default token time-to-live: 7 days (in seconds).
const TOKEN_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Environment variable holding the HMAC signing secret.
const SECRET_ENV: &str = "TOKEN_SECRET_KEY";

lazy_static::lazy_static! {
    /// Cached signing secret so sign/verify always use the same value within a process.
    static ref SIGNING_SECRET: String = std::env::var(SECRET_ENV)
        .unwrap_or_else(|_| "development-approval-secret".to_string());
}

// ---------------------------------------------------------------------------
// Delivery channel
// ---------------------------------------------------------------------------

/// Channel through which an approval magic-link token was delivered. Drives
/// biometric-confirmation requirements on the consume side (SMS / WhatsApp
/// require an explicit attestation; email does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryChannel {
    /// Email magic link (default; the original single channel).
    Email,
    /// SMS text message.
    Sms,
    /// WhatsApp Business message.
    WhatsApp,
}

impl DeliveryChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeliveryChannel::Email => "email",
            DeliveryChannel::Sms => "sms",
            DeliveryChannel::WhatsApp => "whatsapp",
        }
    }

    /// Whether this channel requires `biometric_attested: true` on consume.
    pub fn requires_biometric(&self) -> bool {
        matches!(self, DeliveryChannel::Sms | DeliveryChannel::WhatsApp)
    }

    /// Map a [`SmsChannel`] to the equivalent delivery channel.
    pub fn from_sms_channel(channel: SmsChannel) -> Self {
        match channel {
            SmsChannel::Sms => DeliveryChannel::Sms,
            SmsChannel::WhatsApp => DeliveryChannel::WhatsApp,
        }
    }
}

impl Default for DeliveryChannel {
    fn default() -> Self {
        DeliveryChannel::Email
    }
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

/// JWT claims embedded in an approval magic-link token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalTokenClaims {
    /// The invoice this token authorises an action on.
    pub invoice_id: Uuid,
    /// Email address of the approver.
    pub approver_email: String,
    /// Tenant ID that owns the invoice.
    pub tenant_id: Uuid,
    /// Which actions this token permits: "approve", "reject", "comment".
    pub action_scope: Vec<String>,
    /// Expiration timestamp (UNIX epoch).
    pub exp: i64,
    /// JWT ID - unique per token, used for single-use enforcement.
    pub jti: Uuid,
    /// Delivery channel for this token. Older tokens minted before this field
    /// existed deserialize to the default ([`DeliveryChannel::Email`]).
    #[serde(default)]
    pub delivery_channel: DeliveryChannel,
}

// ---------------------------------------------------------------------------
// Token sign / verify
// ---------------------------------------------------------------------------

fn signing_secret() -> &'static str {
    &SIGNING_SECRET
}

/// Create a signed JWT for the given claims.
pub fn sign_approval_token(claims: &ApprovalTokenClaims) -> Result<String, ApprovalError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(signing_secret().as_bytes()),
    )
    .map_err(|e| ApprovalError::TokenEncoding(e.to_string()))
}

/// Verify a signed JWT, checking signature and expiry.
pub async fn verify_approval_token(token: &str) -> Result<ApprovalTokenClaims, ApprovalError> {
    let token_data = decode::<ApprovalTokenClaims>(
        token,
        &DecodingKey::from_secret(signing_secret().as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => ApprovalError::TokenExpired,
        _ => ApprovalError::InvalidToken(e.to_string()),
    })?;

    Ok(token_data.claims)
}

/// Atomically consume a token so it cannot be replayed.
///
/// Performs an `INSERT ... ON CONFLICT DO NOTHING` against the
/// `used_approval_tokens` table.  Returns `TokenAlreadyUsed` when the
/// `jti` has already been recorded.
pub async fn consume_token(
    pool: &sqlx::PgPool,
    jti: Uuid,
    tenant_id: Uuid,
    invoice_id: Uuid,
    expires_at: i64,
) -> Result<(), ApprovalError> {
    let rows = sqlx::query(
        "INSERT INTO used_approval_tokens (jti, tenant_id, invoice_id, expires_at) \
         VALUES ($1, $2, $3, to_timestamp($4)) ON CONFLICT (jti) DO NOTHING",
    )
    .bind(jti)
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| ApprovalError::Core(billforge_core::Error::Database(e.to_string())))?;

    if rows.rows_affected() == 0 {
        return Err(ApprovalError::TokenAlreadyUsed);
    }
    Ok(())
}

/// Public wrapper for consuming a token (exposed for tests).
pub async fn mark_token_used_pub(
    pool: &sqlx::PgPool,
    jti: Uuid,
    tenant_id: Uuid,
    invoice_id: Uuid,
    exp: i64,
) -> Result<(), ApprovalError> {
    consume_token(pool, jti, tenant_id, invoice_id, exp).await
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("Token has expired")]
    TokenExpired,
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    #[error("Token has already been used")]
    TokenAlreadyUsed,
    #[error("Action not permitted by token scope")]
    ActionNotInScope,
    #[error("Token encoding error: {0}")]
    TokenEncoding(String),
    #[error("{0}")]
    Core(#[from] billforge_core::Error),
}

impl From<ApprovalError> for crate::error::ApiError {
    fn from(err: ApprovalError) -> Self {
        crate::error::ApiError(match err {
            ApprovalError::TokenExpired => billforge_core::Error::TokenExpired,
            ApprovalError::InvalidToken(msg) => billforge_core::Error::InvalidToken(msg),
            ApprovalError::TokenAlreadyUsed => {
                billforge_core::Error::InvalidToken("Token has already been used".to_string())
            }
            ApprovalError::ActionNotInScope => {
                billforge_core::Error::Validation("Action not permitted by token scope".to_string())
            }
            ApprovalError::TokenEncoding(msg) => {
                billforge_core::Error::Internal(format!("Token encoding failed: {}", msg))
            }
            ApprovalError::Core(e) => e,
        })
    }
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: String,
    /// Required for SMS / WhatsApp channels: the mobile / fallback page sets
    /// this after the platform biometric prompt succeeds. Ignored for email
    /// tokens (the confirmation interstitial is sufficient).
    biometric_attested: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RejectQuery {
    token: String,
    reason: Option<String>,
    /// See [`TokenQuery::biometric_attested`].
    biometric_attested: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CommentQuery {
    token: String,
    body: Option<String>,
}

/// Enforce the biometric-attestation requirement for SMS / WhatsApp channels.
///
/// Executives approving via SMS / WhatsApp must confirm via a platform
/// biometric prompt; the consume handler passes the result here. Email tokens
/// bypass this check (the confirmation interstitial + session is sufficient).
pub fn require_biometric_for_channel(
    channel: DeliveryChannel,
    biometric_attested: bool,
) -> Result<(), ApprovalError> {
    if channel.requires_biometric() && !biometric_attested {
        return Err(ApprovalError::Core(billforge_core::Error::Validation(
            "Biometric attestation is required for SMS / WhatsApp approvals"
                .to_string(),
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Approval-request resolution helper (bridges magic-link email to user-scoped
// approval_request update + invoice resolution)
// ---------------------------------------------------------------------------

/// Look up the user for `approver_email` scoped to the tenant, update the
/// matching `approval_requests` row, and run the all-approvers-resolved check.
///
/// If no user matches the email, logs a warning and returns `Ok(())` so that
/// existing behaviour (tokens whose email is not a real user) is preserved.
pub async fn resolve_approval_for_link(
    pool: &sqlx::PgPool,
    metadata_pool: Option<&std::sync::Arc<sqlx::PgPool>>,
    tenant_id: &billforge_core::TenantId,
    invoice_id: Uuid,
    approver_email: &str,
    new_status: &str,
) -> billforge_core::Result<()> {
    // 1. Resolve email -> user_id within the tenant
    let user_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE tenant_id = $1 AND email = $2")
            .bind(*tenant_id.as_uuid())
            .bind(approver_email)
            .fetch_optional(pool)
            .await
            .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    let Some((user_id,)) = user_id else {
        tracing::warn!(
            approver_email = %approver_email,
            invoice_id = %invoice_id,
            "No matching user for magic-link approver email"
        );
        return Ok(());
    };

    // 2. Update the approval request row
    super::email_actions::update_approval_request(
        pool,
        tenant_id,
        invoice_id,
        &UserId(user_id),
        new_status,
    )
    .await?;

    // 3. Resolve invoice approval status (only transitions if ALL requests resolved)
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

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approve", get(approve_via_link))
        .route("/reject", get(reject_via_link))
        .route("/comment", get(comment_via_link))
}

/// Mobile approval-link routes (mounted under `/api/v1/mobile/approval-links`).
/// Includes the SMS / WhatsApp send surface; consume still happens through the
/// shared `/approve` and `/reject` endpoints above.
pub fn mobile_routes() -> Router<AppState> {
    Router::new().route("/send-sms", post(send_sms_approval_link))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn approve_via_link(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> ApiResult<impl IntoResponse> {
    let claims = verify_approval_token(&query.token).await?;

    if !claims.action_scope.contains(&"approve".to_string()) {
        return Err(ApprovalError::ActionNotInScope.into());
    }

    // Biometric confirmation gate for SMS / WhatsApp channels. Email bypasses.
    let biometric_attested = query.biometric_attested.unwrap_or(false);
    require_biometric_for_channel(claims.delivery_channel, biometric_attested)?;

    let tenant_id = billforge_core::TenantId(claims.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

    // Budget guardrail check: block approval if budget is exceeded
    let budget_check = crate::routes::budgets::check_invoice_against_budgets(
        &pool,
        claims.tenant_id,
        claims.invoice_id,
    )
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Budget check failed: {}", e)))?;

    if budget_check.blocked {
        return Err(ApprovalError::Core(billforge_core::Error::Conflict(format!(
            "BUDGET_EXCEEDED: {}",
            serde_json::to_string(&budget_check.violations)
                .unwrap_or_else(|_| "budget exceeded".to_string())
        )))
        .into());
    }

    // Atomically consume the token *before* performing the action.
    consume_token(
        &pool,
        claims.jti,
        claims.tenant_id,
        claims.invoice_id,
        claims.exp,
    )
    .await?;

    // Use a sentinel actor_id for email-based actions (the approver_email is
    // recorded in the audit metadata instead).
    let actor_id = Uuid::nil();

    transition(
        &pool,
        &tenant_id,
        &claims.invoice_id,
        &actor_id,
        InvoiceStatus::Approved,
        "approve_via_email",
        serde_json::json!({
            "approver_email": claims.approver_email,
            "channel": claims.delivery_channel.as_str(),
            "delivery_channel": claims.delivery_channel.as_str(),
            "biometric_attested": biometric_attested,
            "jti": claims.jti.to_string(),
        }),
    )
    .await
    .map_err(ApprovalError::Core)?;

    // Resolve the matching approval_request row so multi-approver logic works.
    resolve_approval_for_link(
        &pool,
        Some(&state.db.metadata()),
        &tenant_id,
        claims.invoice_id,
        &claims.approver_email,
        "approved",
    )
    .await
    .map_err(ApprovalError::Core)?;

    Ok(Html(success_page_html(
        "Approved",
        &claims.invoice_id.to_string(),
        "#10b981",
        "✓",
    )))
}

async fn reject_via_link(
    State(state): State<AppState>,
    Query(query): Query<RejectQuery>,
) -> ApiResult<impl IntoResponse> {
    let claims = verify_approval_token(&query.token).await?;

    if !claims.action_scope.contains(&"reject".to_string()) {
        return Err(ApprovalError::ActionNotInScope.into());
    }

    // Biometric confirmation gate for SMS / WhatsApp channels. Email bypasses.
    let biometric_attested = query.biometric_attested.unwrap_or(false);
    require_biometric_for_channel(claims.delivery_channel, biometric_attested)?;

    let tenant_id = billforge_core::TenantId(claims.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

    // Atomically consume the token *before* performing the action.
    consume_token(
        &pool,
        claims.jti,
        claims.tenant_id,
        claims.invoice_id,
        claims.exp,
    )
    .await?;

    let actor_id = Uuid::nil();
    let reason = query.reason.unwrap_or_default();

    transition(
        &pool,
        &tenant_id,
        &claims.invoice_id,
        &actor_id,
        InvoiceStatus::Rejected,
        "reject_via_email",
        serde_json::json!({
            "approver_email": claims.approver_email,
            "channel": claims.delivery_channel.as_str(),
            "delivery_channel": claims.delivery_channel.as_str(),
            "biometric_attested": biometric_attested,
            "jti": claims.jti.to_string(),
            "reason": reason,
        }),
    )
    .await
    .map_err(ApprovalError::Core)?;

    // Resolve the matching approval_request row so multi-approver logic works.
    resolve_approval_for_link(
        &pool,
        Some(&state.db.metadata()),
        &tenant_id,
        claims.invoice_id,
        &claims.approver_email,
        "rejected",
    )
    .await
    .map_err(ApprovalError::Core)?;

    Ok(Html(success_page_html(
        "Rejected",
        &claims.invoice_id.to_string(),
        "#ef4444",
        "✕",
    )))
}

async fn comment_via_link(
    State(state): State<AppState>,
    Query(query): Query<CommentQuery>,
) -> ApiResult<impl IntoResponse> {
    let claims = verify_approval_token(&query.token).await?;

    if !claims.action_scope.contains(&"comment".to_string()) {
        return Err(ApprovalError::ActionNotInScope.into());
    }

    let tenant_id = billforge_core::TenantId(claims.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

    // Atomically consume the token *before* performing the action.
    consume_token(
        &pool,
        claims.jti,
        claims.tenant_id,
        claims.invoice_id,
        claims.exp,
    )
    .await?;

    let body = query.body.unwrap_or_default();

    // Insert a comment-style audit row without changing the invoice status.
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'comment_via_email', $5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(claims.tenant_id)
    .bind(claims.invoice_id)
    .bind(Uuid::nil())
    .bind(serde_json::json!({
        "approver_email": claims.approver_email,
        "channel": "email",
        "jti": claims.jti.to_string(),
        "comment_body": body,
    }))
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to insert comment audit row: {}", e))
    })?;

    Ok(Html(success_page_html(
        "Comment Added",
        &claims.invoice_id.to_string(),
        "#3b82f6",
        "💬",
    )))
}

// ---------------------------------------------------------------------------
// HTML helpers
// ---------------------------------------------------------------------------

fn success_page_html(action_text: &str, invoice_id: &str, color: &str, icon: &str) -> String {
    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Invoice {action_text}</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
               line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto;
               padding: 20px; background-color: #f5f5f5; }}
        .container {{ background: #fff; border-radius: 8px; padding: 40px;
                      box-shadow: 0 2px 4px rgba(0,0,0,.1); text-align: center; }}
        .icon {{ width: 60px; height: 60px; border-radius: 50%; background: {color};
                 display: flex; align-items: center; justify-content: center;
                 margin: 0 auto 20px; color: #fff; font-size: 30px; }}
        h1 {{ color: #1f2937; font-size: 24px; margin-bottom: 10px; }}
        p  {{ color: #6b7280; margin-bottom: 30px; }}
        .button {{ display: inline-block; background: #2563eb; color: #fff;
                   padding: 12px 24px; text-decoration: none; border-radius: 6px;
                   font-weight: 500; }}
        .footer {{ margin-top: 30px; padding-top: 20px; border-top: 1px solid #e5e7eb;
                   font-size: 12px; color: #9ca3af; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="icon">{icon}</div>
        <h1>Invoice {action_text}</h1>
        <p>The invoice has been successfully {action_text_lower}.</p>
        <a href="{app_url}/invoices/{invoice_id}" class="button">View Invoice</a>
        <div class="footer"><p>This action was performed via a secure email link.</p></div>
    </div>
</body>
</html>"#,
        action_text = action_text,
        action_text_lower = action_text.to_lowercase(),
        color = color,
        icon = icon,
        app_url = app_url,
        invoice_id = invoice_id,
    )
}

// ---------------------------------------------------------------------------
// Public helpers (used by tests and future email-sending code)
// ---------------------------------------------------------------------------

/// Build a signed approval token for the given parameters.
/// Defaults TTL to [`TOKEN_TTL_SECS`] (7 days) and generates a random `jti`.
/// Delivery channel defaults to [`DeliveryChannel::Email`].
pub fn create_approval_token(
    invoice_id: Uuid,
    approver_email: String,
    tenant_id: Uuid,
    action_scope: Vec<String>,
) -> Result<String, ApprovalError> {
    mint_approval_token(
        invoice_id,
        approver_email,
        tenant_id,
        action_scope,
        DeliveryChannel::Email,
        TOKEN_TTL_SECS,
    )
}

/// Build a signed approval token with a custom expiry (for testing).
pub fn create_approval_token_with_exp(
    invoice_id: Uuid,
    approver_email: String,
    tenant_id: Uuid,
    action_scope: Vec<String>,
    exp: i64,
) -> Result<String, ApprovalError> {
    let claims = ApprovalTokenClaims {
        invoice_id,
        approver_email,
        tenant_id,
        action_scope,
        exp,
        jti: Uuid::new_v4(),
        delivery_channel: DeliveryChannel::Email,
    };
    sign_approval_token(&claims)
}

/// Build a signed approval token for a specific delivery channel and TTL.
///
/// Single source of truth for the single-use `jti` + TTL signing logic shared
/// by the email and SMS/WhatsApp delivery paths. Returns a signed JWT whose
/// claims can be verified via [`verify_approval_token`] and consumed via
/// [`consume_token`].
pub fn mint_approval_token(
    invoice_id: Uuid,
    approver_email: String,
    tenant_id: Uuid,
    action_scope: Vec<String>,
    delivery_channel: DeliveryChannel,
    ttl_secs: i64,
) -> Result<String, ApprovalError> {
    let claims = ApprovalTokenClaims {
        invoice_id,
        approver_email,
        tenant_id,
        action_scope,
        exp: Utc::now().timestamp() + ttl_secs,
        jti: Uuid::new_v4(),
        delivery_channel,
    };
    sign_approval_token(&claims)
}

// ---------------------------------------------------------------------------
// SMS / WhatsApp delivery surface
// ---------------------------------------------------------------------------

/// Build the short URL a recipient taps from an SMS / WhatsApp message.
///
/// Format: `{APP_URL}/a/{token}`. The frontend resolves `/a/{token}` to the
/// biometric confirmation interstitial (for SMS / WhatsApp channels) or the
/// plain approve/reject page (email).
pub fn build_approval_short_url(token: &str) -> String {
    let app_url = std::env::var("APP_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:3000".to_string());
    format!("{}/a/{}", app_url.trim_end_matches('/'), token)
}

/// Build the SMS / WhatsApp message body for an approval link.
///
/// Mirrors the email body shape: vendor-free summary + signed deep link. The
/// amount is rendered as a plain cents value (localization is deferred).
pub fn build_approval_sms_body(
    invoice_number: &str,
    amount_cents: i64,
    short_url: &str,
) -> String {
    format!(
        "Approve invoice {} for {} cents: {}",
        invoice_number, amount_cents, short_url
    )
}

/// Tenant-scoped lookup of a recipient's mobile number (E.164) and email.
///
/// Phone is resolved from the existing `users.settings->>'phone'` JSONB field
/// so no schema migration is required. Returns `None` when the user is not in
/// the caller's tenant or has no phone configured, so the caller can fail
/// closed.
pub async fn resolve_recipient_phone(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::TenantId,
    recipient_user_id: Uuid,
) -> Result<Option<(String, String)>, ApprovalError> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT settings->>'phone' AS phone, email \
         FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(recipient_user_id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| ApprovalError::Core(billforge_core::Error::Database(e.to_string())))?;

    Ok(row.and_then(|(phone, email)| phone.map(|p| (p, email))))
}

/// Outcome of dispatching an SMS / WhatsApp approval link.
#[derive(Debug, Clone, Serialize)]
pub struct SmsDispatchOutcome {
    /// Provider-issued message id (correlates with delivery receipts).
    pub message_id: String,
    /// Short signed URL the recipient taps.
    pub short_url: String,
    /// The message body that was sent.
    pub body: String,
    /// Delivery channel that was used.
    pub channel: DeliveryChannel,
}

/// Core SMS / WhatsApp delivery: mint a single-use approval token for the
/// recipient, build the short URL + body, and dispatch via `provider`.
///
/// Extracted from the HTTP handler so tests can exercise it with a
/// [`sms::NoopSmsProvider`] without a database or HTTP stack.
pub async fn dispatch_sms_approval_link(
    invoice_number: &str,
    amount_cents: i64,
    invoice_id: Uuid,
    approver_email: String,
    tenant_id: Uuid,
    to_e164: &str,
    channel: SmsChannel,
    provider: &dyn SmsProvider,
) -> Result<SmsDispatchOutcome, ApprovalError> {
    let delivery = DeliveryChannel::from_sms_channel(channel);
    let token = mint_approval_token(
        invoice_id,
        approver_email,
        tenant_id,
        vec!["approve".to_string(), "reject".to_string()],
        delivery,
        TOKEN_TTL_SECS,
    )?;
    let short_url = build_approval_short_url(&token);
    let body = build_approval_sms_body(invoice_number, amount_cents, &short_url);

    let message_id = provider
        .send(to_e164, &body, channel)
        .await
        .map_err(|e| {
            ApprovalError::Core(billforge_core::Error::ExternalService {
                service: channel.as_str().to_string(),
                message: e.to_string(),
            })
        })?;

    Ok(SmsDispatchOutcome {
        message_id,
        short_url,
        body,
        channel: delivery,
    })
}

/// Build the `approval_link.sent` audit metadata for an SMS dispatch.
pub fn approval_link_sent_metadata(
    channel: DeliveryChannel,
    recipient_user_id: Uuid,
    phone: &str,
    message_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "event": "approval_link.sent",
        "channel": channel.as_str(),
        "recipient_user_id": recipient_user_id.to_string(),
        "recipient_phone": phone,
        "provider_message_id": message_id,
    })
}

/// Request body for `POST /api/v1/mobile/approval-links/send-sms`.
#[derive(Debug, Deserialize, Serialize)]
pub struct SendSmsRequest {
    pub invoice_id: Uuid,
    pub recipient_user_id: Uuid,
    pub channel: SmsChannel,
}

/// Response for the SMS send endpoint.
#[derive(Debug, Serialize)]
pub struct SendSmsResponse {
    pub message_id: String,
    pub channel: DeliveryChannel,
    pub short_url: String,
}

/// Write the `approval_link.sent` audit row to `invoice_audit_log`.
async fn record_sms_send_audit(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    invoice_id: Uuid,
    actor_id: Uuid,
    metadata: serde_json::Value,
) -> Result<(), ApprovalError> {
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type, metadata)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', 'approval_link.sent', $5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(actor_id)
    .bind(metadata)
    .execute(pool)
    .await
    .map_err(|e| ApprovalError::Core(billforge_core::Error::Database(e.to_string())))?;
    Ok(())
}

/// `POST /api/v1/mobile/approval-links/send-sms`
///
/// Resolves the recipient (tenant-scoped), mints a single-use approval token,
/// builds the short URL + body, dispatches via the configured SMS provider,
/// and records an `approval_link.sent` audit event.
pub async fn send_sms_approval_link(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(req): Json<SendSmsRequest>,
) -> ApiResult<Json<SendSmsResponse>> {
    let tenant_id = tenant.tenant_id.clone();
    let pool = state.db.tenant(&tenant_id).await?;

    // 1. Resolve recipient within the caller's tenant (fail closed).
    let (phone, email) = resolve_recipient_phone(&pool, &tenant_id, req.recipient_user_id)
        .await?
        .ok_or_else(|| {
            ApprovalError::Core(billforge_core::Error::NotFound {
                resource_type: "User".to_string(),
                id: req.recipient_user_id.to_string(),
            })
        })?;

    // 2. Look up invoice details (tenant-scoped).
    let invoice: Option<(String, i64)> = sqlx::query_as(
        "SELECT invoice_number, COALESCE(total_amount_cents, 0) \
         FROM invoices WHERE id = $1 AND tenant_id = $2",
    )
    .bind(req.invoice_id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| ApprovalError::Core(billforge_core::Error::Database(e.to_string())))?;

    let (invoice_number, amount_cents) = invoice.ok_or_else(|| {
        ApprovalError::Core(billforge_core::Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: req.invoice_id.to_string(),
        })
    })?;

    // 3. Dispatch via the configured provider (Noop in dev / CI).
    let provider = sms::provider_from_env();
    let outcome = dispatch_sms_approval_link(
        &invoice_number,
        amount_cents,
        req.invoice_id,
        email,
        *tenant_id.as_uuid(),
        &phone,
        req.channel,
        provider.as_ref(),
    )
    .await?;

    // 4. Audit the send.
    let metadata = approval_link_sent_metadata(
        outcome.channel,
        req.recipient_user_id,
        &phone,
        &outcome.message_id,
    );
    record_sms_send_audit(&pool, *tenant_id.as_uuid(), req.invoice_id, *user.user_id.as_uuid(), metadata)
        .await?;

    Ok(Json(SendSmsResponse {
        message_id: outcome.message_id,
        channel: outcome.channel,
        short_url: outcome.short_url,
    }))
}
