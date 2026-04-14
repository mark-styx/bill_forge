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
//! - Move the in-memory `USED_TOKENS` set to a DB-backed store (e.g. a
//!   `used_approval_tokens` table with a UNIQUE constraint on `jti`) for
//!   persistence across server restarts.

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;
use crate::state_machine::{transition, InvoiceStatus};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default token time-to-live: 7 days (in seconds).
const TOKEN_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Environment variable holding the HMAC signing secret.
const SECRET_ENV: &str = "TOKEN_SECRET_KEY";

lazy_static::lazy_static! {
    /// In-memory single-use token store.
    /// TODO: replace with a DB table (`used_approval_tokens`) for durability.
    static ref USED_TOKENS: Arc<Mutex<HashSet<Uuid>>> = Arc::new(Mutex::new(HashSet::new()));

    /// Cached signing secret so sign/verify always use the same value within a process.
    static ref SIGNING_SECRET: String = std::env::var(SECRET_ENV)
        .unwrap_or_else(|_| "development-approval-secret".to_string());
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

/// Verify a signed JWT, checking signature, expiry, and single-use constraint.
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

    // Single-use check
    {
        let used = USED_TOKENS.lock().await;
        if used.contains(&token_data.claims.jti) {
            return Err(ApprovalError::TokenAlreadyUsed);
        }
    }

    Ok(token_data.claims)
}

/// Mark a token's `jti` as used so it cannot be replayed.
async fn mark_token_used(jti: Uuid) {
    let mut used = USED_TOKENS.lock().await;
    used.insert(jti);
}

/// Public wrapper for marking a token as used (exposed for tests).
pub async fn mark_token_used_pub(jti: Uuid) {
    mark_token_used(jti).await
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
}

#[derive(Debug, Deserialize)]
struct RejectQuery {
    token: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommentQuery {
    token: String,
    body: Option<String>,
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

    let tenant_id = billforge_core::TenantId(claims.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

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
            "channel": "email",
            "jti": claims.jti.to_string(),
        }),
    )
    .await
    .map_err(ApprovalError::Core)?;

    mark_token_used(claims.jti).await;

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

    let tenant_id = billforge_core::TenantId(claims.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

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
            "channel": "email",
            "jti": claims.jti.to_string(),
            "reason": reason,
        }),
    )
    .await
    .map_err(ApprovalError::Core)?;

    mark_token_used(claims.jti).await;

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

    mark_token_used(claims.jti).await;

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
    let app_url =
        std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

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
pub fn create_approval_token(
    invoice_id: Uuid,
    approver_email: String,
    tenant_id: Uuid,
    action_scope: Vec<String>,
) -> Result<String, ApprovalError> {
    let claims = ApprovalTokenClaims {
        invoice_id,
        approver_email,
        tenant_id,
        action_scope,
        exp: Utc::now().timestamp() + TOKEN_TTL_SECS,
        jti: Uuid::new_v4(),
    };
    sign_approval_token(&claims)
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
    };
    sign_approval_token(&claims)
}
