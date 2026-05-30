//! Chat-native approval surface — Slack interactions, events, slash commands,
//! and Teams actionable-message callbacks.
//!
//! All routes bypass the standard JWT middleware; they authenticate via:
//! - Slack: HMAC-SHA256 signing-secret verification
//! - Teams: Bearer JWT from Microsoft (feature-flagged skip in dev)
//!
//! **Security notes:**
//! - The Teams `/teams/actions` endpoint is gated behind `TEAMS_ACTIONS_ENABLED=true`
//!   and **disabled by default**. Full JWT validation against the Microsoft OpenID
//!   cert endpoint must be implemented before enabling in production.
//! - The handler resolves the acting user from `teams_webhooks` rather than using a
//!   sentinel nil UUID, preserving the audit-trail contract and FK integrity.
//!
//! After verifying the caller, the handler resolves the acting BillForge user
//! via the `slack_connections` / `teams_webhooks` user_id mapping, then writes
//! the approval action to the existing invoice state machine and audit trail.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use billforge_core::{TenantId, UserId};
use billforge_notifications::{
    build_invoice_approval_blocks, build_teams_approval_card, verify_slack_signature,
    InvoiceContext, InvoiceLineItem,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    let mut router = Router::new()
        .route("/slack/interactions", post(slack_interactions))
        .route("/slack/events", post(slack_events))
        .route("/slack/commands", post(slack_commands));

    // Teams actions endpoint is gated behind TEAMS_ACTIONS_ENABLED=true.
    // The route is disabled by default because JWT validation against the
    // Microsoft OpenID cert endpoint (issuer, audience, signature, expiry)
    // has not been implemented yet. Enable only after that lands.
    if std::env::var("TEAMS_ACTIONS_ENABLED").as_deref() == Ok("true") {
        tracing::warn!(
            "Teams actions endpoint is ENABLED. Ensure TEAMS_SKIP_JWT_VALIDATION is NOT \
             set to 'true' in production. Full Microsoft JWT validation must be \
             implemented before this flag is used outside dev/test."
        );
        router = router.route("/teams/actions", post(teams_actions));
    } else {
        // Register a catch-all that returns 404 so callers get a clear signal.
        router = router.route("/teams/actions", post(teams_actions_disabled));
    }

    router
}

/// Handler returned when `TEAMS_ACTIONS_ENABLED` is not set.
async fn teams_actions_disabled() -> Result<StatusCode, ApiError> {
    tracing::warn!(
        "Teams actions endpoint called but TEAMS_ACTIONS_ENABLED is not set. \
         Rejecting request."
    );
    Err(ApiError(billforge_core::Error::Validation(
        "Teams actions endpoint is disabled. Set TEAMS_ACTIONS_ENABLED=true to enable.".to_string(),
    )))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the Slack signing secret from env (set during Slack app installation).
///
/// **Security:** Returns an error if the variable is not set. There is no
/// hardcoded fallback — a missing secret means the interaction/event/command
/// routes cannot verify request authenticity and must reject all calls.
fn slack_signing_secret() -> Result<String, ApiError> {
    std::env::var("SLACK_SIGNING_SECRET").map_err(|_| {
        tracing::error!(
            "SLACK_SIGNING_SECRET env var is not set. Slack interaction routes will reject all requests."
        );
        ApiError(billforge_core::Error::Validation(
            "SLACK_SIGNING_SECRET is not configured. Refusing to process Slack requests.".to_string(),
        ))
    })
}

/// Look up the BillForge user_id for a Slack user via `slack_connections`.
async fn resolve_slack_user(
    pool: &sqlx::PgPool,
    slack_user_id: &str,
) -> Result<(Uuid, Uuid), ApiError> {
    // Returns (user_id, tenant_id)
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT user_id, tenant_id FROM slack_connections WHERE slack_user_id = $1 AND is_active = true LIMIT 1",
    )
    .bind(slack_user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    row.ok_or_else(|| {
        ApiError(billforge_core::Error::Validation(format!(
            "No active Slack connection for user {}",
            slack_user_id
        )))
    })
}

/// Write an audit row for a chat-initiated action.
async fn write_chat_audit(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    invoice_id: Uuid,
    actor_id: Uuid,
    event_type: &str,
    source_channel: &str,
    source_message_ts: Option<&str>,
    metadata: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO invoice_audit_log
               (id, tenant_id, invoice_id, actor_id, from_status, to_status, event_type,
                metadata, source_channel, source_message_ts)
           VALUES ($1, $2, $3, $4, 'pending_approval', 'pending_approval', $5, $6, $7, $8)"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(actor_id)
    .bind(event_type)
    .bind(serde_json::to_string(&metadata).unwrap_or_default())
    .bind(source_channel)
    .bind(source_message_ts)
    .execute(pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(())
}

/// Parse an `action_id` of the form `bf_{verb}:{invoice_id}`.
fn parse_action_id(action_id: &str) -> Option<(&str, Uuid)> {
    let (verb, id_str) = action_id.split_once(':')?;
    let uuid = id_str.parse::<Uuid>().ok()?;
    Some((verb, uuid))
}

/// Persist a chat approval thread mapping for future event routing.
async fn persist_thread_mapping(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    invoice_id: Uuid,
    provider: &str,
    channel_id: &str,
    message_ts: &str,
    approver_user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO chat_approval_threads
               (tenant_id, invoice_id, provider, channel_id, message_ts, approver_user_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (provider, channel_id, message_ts) DO NOTHING"#,
    )
    .bind(tenant_id)
    .bind(invoice_id)
    .bind(provider)
    .bind(channel_id)
    .bind(message_ts)
    .bind(approver_user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    Ok(())
}

/// Execute an approval state transition via the existing state machine.
async fn transition_invoice(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: &Uuid,
    actor_id: &Uuid,
    new_status: crate::state_machine::InvoiceStatus,
    event_type: &str,
    metadata: serde_json::Value,
) -> Result<(), ApiError> {
    crate::state_machine::transition(
        pool, tenant_id, invoice_id, actor_id, new_status, event_type, metadata,
    )
    .await
    .map_err(ApiError)
}

/// Resolve the approval request after a chat action, reusing the email-actions helper.
async fn resolve_approval(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    invoice_id: Uuid,
    user_id: &UserId,
    new_status: &str,
) -> Result<(), ApiError> {
    super::email_actions::update_approval_request(pool, tenant_id, invoice_id, user_id, new_status)
        .await
        .map_err(ApiError)?;
    super::workflows::resolve_invoice_approval_status(
        &mut *pool
            .acquire()
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?,
        tenant_id,
        invoice_id,
    )
    .await
    .map(|_| ())
    .map_err(ApiError)
}

// ---------------------------------------------------------------------------
// Slack interactions handler
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SlackInteractionsBody {
    #[serde(rename = "type")]
    type_: String,
    /// URL-encoded JSON payload for block_actions / interactive_message
    payload: Option<String>,
    /// For view_submission the payload is at the top level
    actions: Option<Vec<SlackActionSlim>>,
    user: Option<SlackUserSlim>,
    channel: Option<SlackChannelSlim>,
    container: Option<SlackContainerSlim>,
    /// Raw `payload` field comes URL-encoded as `payload=...` for block_actions
    #[serde(rename = "payload")]
    payload_raw: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackActionSlim {
    action_id: String,
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackUserSlim {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackChannelSlim {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackContainerSlim {
    message_ts: Option<String>,
    channel_id: Option<String>,
}

/// Generalized Slack interaction body (parsed from URL-encoded `payload` field)
#[derive(Debug, Deserialize)]
struct SlackInteractionParsed {
    #[serde(rename = "type")]
    type_: String,
    actions: Option<Vec<SlackActionSlim>>,
    user: Option<SlackUserSlim>,
    channel: Option<SlackChannelSlim>,
    container: Option<SlackContainerSlim>,
}

async fn slack_interactions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    let secret = slack_signing_secret()?;
    let ts = headers
        .get("X-Slack-Request-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sig = headers
        .get("X-Slack-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    verify_slack_signature(&secret, ts, sig, &body).map_err(|_| {
        ApiError(billforge_core::Error::Validation(
            "Invalid Slack signature".to_string(),
        ))
    })?;

    // Slack sends block_actions payloads as URL-encoded form with a `payload` field
    let body_str = String::from_utf8_lossy(&body);
    let parsed: SlackInteractionParsed = if body_str.contains("payload=") {
        if let Some(start) = body_str.find("payload=") {
            let encoded = &body_str[start + 8..];
            let decoded = urlencoding::decode(encoded).unwrap_or_default();
            serde_json::from_str(&decoded).unwrap_or_else(|_| SlackInteractionParsed {
                type_: "unknown".to_string(),
                actions: None,
                user: None,
                channel: None,
                container: None,
            })
        } else {
            serde_json::from_slice(&body).unwrap_or_else(|_| SlackInteractionParsed {
                type_: "unknown".to_string(),
                actions: None,
                user: None,
                channel: None,
                container: None,
            })
        }
    } else {
        serde_json::from_slice(&body).unwrap_or_else(|_| SlackInteractionParsed {
            type_: "unknown".to_string(),
            actions: None,
            user: None,
            channel: None,
            container: None,
        })
    };

    let slack_user_id = parsed.user.as_ref().map(|u| u.id.as_str()).unwrap_or("");

    let pool = state.db.metadata();
    let (user_id, tenant_id) = resolve_slack_user(&pool, slack_user_id).await?;
    let tenant_pool = state
        .db
        .tenant(&TenantId(tenant_id))
        .await
        .map_err(ApiError)?;

    if let Some(actions) = &parsed.actions {
        if let Some(action) = actions.first() {
            if let Some((verb, invoice_id)) = parse_action_id(&action.action_id) {
                let msg_ts = parsed.container.as_ref().and_then(|c| c.message_ts.clone());
                let channel_id = parsed
                    .container
                    .as_ref()
                    .and_then(|c| c.channel_id.clone())
                    .or_else(|| parsed.channel.as_ref().map(|c| c.id.clone()))
                    .unwrap_or_default();

                match verb {
                    "bf_approve" => {
                        transition_invoice(
                            &tenant_pool,
                            &TenantId(tenant_id),
                            &invoice_id,
                            &user_id,
                            crate::state_machine::InvoiceStatus::Approved,
                            "approve_via_slack",
                            serde_json::json!({ "channel": "slack", "message_ts": msg_ts }),
                        )
                        .await?;
                        resolve_approval(
                            &tenant_pool,
                            &TenantId(tenant_id),
                            invoice_id,
                            &UserId(user_id),
                            "approved",
                        )
                        .await?;
                    }
                    "bf_reject" => {
                        transition_invoice(
                            &tenant_pool,
                            &TenantId(tenant_id),
                            &invoice_id,
                            &user_id,
                            crate::state_machine::InvoiceStatus::Rejected,
                            "reject_via_slack",
                            serde_json::json!({ "channel": "slack", "message_ts": msg_ts }),
                        )
                        .await?;
                        resolve_approval(
                            &tenant_pool,
                            &TenantId(tenant_id),
                            invoice_id,
                            &UserId(user_id),
                            "rejected",
                        )
                        .await?;
                    }
                    "bf_request_changes" | "bf_reassign" | "bf_comment" => {
                        // These open a Slack modal (views.open) or write a comment.
                        // For this slice, record the action as an audit comment.
                        let event_type = format!("{}_via_slack", verb.replace("bf_", ""));
                        write_chat_audit(
                            &tenant_pool,
                            tenant_id,
                            invoice_id,
                            user_id,
                            &event_type,
                            "slack",
                            msg_ts.as_deref(),
                            serde_json::json!({ "action": verb }),
                        )
                        .await?;
                    }
                    _ => {}
                }

                // Persist thread mapping for future event routing
                if let (Some(ref ts), ref ch) = (msg_ts, channel_id) {
                    if !ch.is_empty() {
                        let _ = persist_thread_mapping(
                            &tenant_pool,
                            tenant_id,
                            invoice_id,
                            "slack",
                            ch,
                            ts,
                            user_id,
                        )
                        .await;
                    }
                }
            }
        }
    }

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Slack events handler (thread replies)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SlackEventBody {
    event: Option<SlackEventMessage>,
    #[serde(rename = "type")]
    type_: Option<String>,
    challenge: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackEventMessage {
    #[serde(rename = "type")]
    type_: String,
    thread_ts: Option<String>,
    text: Option<String>,
    user: Option<String>,
    ts: Option<String>,
    channel: Option<String>,
}

async fn slack_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Verify Slack signature on the raw body bytes before any parsing.
    let secret = slack_signing_secret()?;
    let ts = headers
        .get("X-Slack-Request-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sig = headers
        .get("X-Slack-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    verify_slack_signature(&secret, ts, sig, &raw_body).map_err(|_| {
        ApiError(billforge_core::Error::Validation(
            "Invalid Slack signature".to_string(),
        ))
    })?;

    let body: serde_json::Value = serde_json::from_slice(&raw_body).map_err(|e| {
        ApiError(billforge_core::Error::Validation(format!(
            "Invalid JSON body: {}",
            e
        )))
    })?;

    // Handle URL verification challenge
    if body.get("type").and_then(|v| v.as_str()) == Some("url_verification") {
        let challenge = body
            .get("challenge")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Ok(Json(serde_json::json!({ "challenge": challenge })));
    }

    let event = match body.get("event") {
        Some(e) => e,
        None => return Ok(Json(serde_json::json!({ "ok": true }))),
    };

    let thread_ts = match event.get("thread_ts").and_then(|v| v.as_str()) {
        Some(ts) => ts.to_string(),
        None => {
            return Ok(Json(
                serde_json::json!({ "ok": true, "skipped": "no_thread_ts" }),
            ))
        }
    };

    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type != "message" {
        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    // Skip bot messages
    if event.get("bot_id").is_some()
        || event.get("subtype").and_then(|v| v.as_str()) == Some("bot_message")
    {
        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    let slack_user_id = event.get("user").and_then(|v| v.as_str()).unwrap_or("");
    let message_text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let message_ts = event.get("ts").and_then(|v| v.as_str()).unwrap_or("");
    let channel_id = event.get("channel").and_then(|v| v.as_str()).unwrap_or("");

    let pool = state.db.metadata();
    let (user_id, tenant_id) = resolve_slack_user(&pool, slack_user_id).await?;
    let tenant_pool = state
        .db
        .tenant(&TenantId(tenant_id))
        .await
        .map_err(ApiError)?;

    // Look up the invoice for this thread
    let thread: Option<(Uuid,)> = sqlx::query_as(
        "SELECT invoice_id FROM chat_approval_threads WHERE provider = 'slack' AND channel_id = $1 AND message_ts = $2 LIMIT 1",
    )
    .bind(channel_id)
    .bind(&thread_ts)
    .fetch_optional(&*tenant_pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    if let Some((invoice_id,)) = thread {
        write_chat_audit(
            &tenant_pool,
            tenant_id,
            invoice_id,
            user_id,
            "thread_comment_via_slack",
            "slack",
            Some(message_ts),
            serde_json::json!({
                "comment_body": message_text,
                "thread_ts": thread_ts,
            }),
        )
        .await?;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Slack slash commands (/billforge)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SlackCommandBody {
    text: Option<String>,
    user_id: Option<String>,
    user_name: Option<String>,
    channel_id: Option<String>,
    response_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct SlackCommandResponse {
    response_type: String,
    text: String,
    blocks: Vec<serde_json::Value>,
}

async fn slack_commands(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: axum::body::Bytes,
) -> Result<Json<SlackCommandResponse>, ApiError> {
    let secret = slack_signing_secret()?;
    let ts = headers
        .get("X-Slack-Request-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sig = headers
        .get("X-Slack-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    verify_slack_signature(&secret, ts, sig, &raw_body).map_err(|_| {
        ApiError(billforge_core::Error::Validation(
            "Invalid Slack signature".to_string(),
        ))
    })?;

    // Parse the form-encoded body after signature verification succeeds.
    let body: SlackCommandBody = serde_urlencoded::from_bytes(&raw_body).map_err(|e| {
        ApiError(billforge_core::Error::Validation(format!(
            "Invalid form body: {}",
            e
        )))
    })?;

    let slack_user_id = body.user_id.as_deref().unwrap_or("");
    let pool = state.db.metadata();
    let (user_id, tenant_id) = resolve_slack_user(&pool, slack_user_id).await?;
    let tenant_pool = state
        .db
        .tenant(&TenantId(tenant_id))
        .await
        .map_err(ApiError)?;

    let text = body.text.as_deref().unwrap_or("").trim();
    let parts: Vec<&str> = text.splitn(2, char::is_whitespace).collect();
    let subcmd = parts.first().copied().unwrap_or("");
    let arg = parts.get(1).copied().unwrap_or("");

    match subcmd {
        "pending" => {
            // List the caller's pending approval requests
            let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(
                r#"SELECT i.id, i.vendor_name, i.invoice_number, i.total_amount_cents
                   FROM approval_requests ar
                   JOIN invoices i ON i.id = ar.invoice_id AND i.tenant_id = ar.tenant_id
                   WHERE ar.tenant_id = $1
                     AND ar.status = 'pending'
                     AND (ar.requested_from->>'User' = $2
                          OR (ar.requested_from ? 'AnyOf' AND ar.requested_from->'AnyOf' @> to_jsonb($2::text)))
                   ORDER BY ar.created_at DESC
                   LIMIT 10"#,
            )
            .bind(tenant_id)
            .bind(user_id.to_string())
            .fetch_all(&*tenant_pool)
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

            let mut blocks: Vec<serde_json::Value> = Vec::new();
            blocks.push(serde_json::json!({
                "type": "section",
                "text": { "type": "mrkdwn", "text": format!("*Your Pending Approvals ({})*", rows.len()) }
            }));

            for (inv_id, vendor, inv_num, amount_cents) in &rows {
                let amount = *amount_cents as f64 / 100.0;
                blocks.push(serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": format!("• *{}* — {} ${:.2} (`{}`)", vendor, inv_num, amount, inv_id) }
                }));
            }

            if rows.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": "No pending approvals. :tada:" }
                }));
            }

            Ok(Json(SlackCommandResponse {
                response_type: "ephemeral".to_string(),
                text: "Pending approvals".to_string(),
                blocks,
            }))
        }
        "status" => {
            let invoice_id_str = arg.trim();
            let invoice_id = invoice_id_str
                .parse::<Uuid>()
                .map_err(|_| ApiError(billforge_core::Error::Validation("Invalid invoice ID".to_string())))?;

            let row: Option<(String, String, i64)> = sqlx::query_as(
                "SELECT vendor_name, status, total_amount_cents FROM invoices WHERE id = $1 AND tenant_id = $2",
            )
            .bind(invoice_id)
            .bind(tenant_id)
            .fetch_optional(&*tenant_pool)
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

            let blocks = if let Some((vendor, status, amount_cents)) = row {
                let amount = amount_cents as f64 / 100.0;
                vec![serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": format!("*{}* — {} ${:.2}\nStatus: `{}`", vendor, invoice_id, amount, status) }
                })]
            } else {
                vec![serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": format!("Invoice `{}` not found.", invoice_id) }
                })]
            };

            Ok(Json(SlackCommandResponse {
                response_type: "ephemeral".to_string(),
                text: "Invoice status".to_string(),
                blocks,
            }))
        }
        "search" => {
            let query = arg.trim();
            let pattern = format!("%{}%", query);

            let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(
                r#"SELECT id, vendor_name, invoice_number, total_amount_cents
                   FROM invoices
                   WHERE tenant_id = $1 AND (vendor_name ILIKE $2 OR invoice_number ILIKE $2)
                   ORDER BY created_at DESC LIMIT 5"#,
            )
            .bind(tenant_id)
            .bind(&pattern)
            .fetch_all(&*tenant_pool)
            .await
            .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

            let mut blocks: Vec<serde_json::Value> = Vec::new();
            blocks.push(serde_json::json!({
                "type": "section",
                "text": { "type": "mrkdwn", "text": format!("*Search results for \"{}\"*", query) }
            }));

            for (inv_id, vendor, inv_num, amount_cents) in &rows {
                let amount = *amount_cents as f64 / 100.0;
                blocks.push(serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": format!("• *{}* — {} ${:.2} (`{}`)", vendor, inv_num, amount, inv_id) }
                }));
            }

            if rows.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": "No matching invoices found." }
                }));
            }

            Ok(Json(SlackCommandResponse {
                response_type: "ephemeral".to_string(),
                text: "Search results".to_string(),
                blocks,
            }))
        }
        _ => Ok(Json(SlackCommandResponse {
            response_type: "ephemeral".to_string(),
            text: "Unknown command. Use: `/billforge pending`, `/billforge status <id>`, `/billforge search <query>`".to_string(),
            blocks: vec![],
        })),
    }
}

// ---------------------------------------------------------------------------
// Teams actions handler
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TeamsActionBody {
    action: Option<String>,
    invoice_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
    comment_body: Option<String>,
    reassign_to_user_id: Option<Uuid>,
}

async fn teams_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TeamsActionBody>,
) -> Result<StatusCode, ApiError> {
    // Bearer JWT validation for Teams Actionable Message callbacks.
    // In production, validate the JWT against the Microsoft cert endpoint.
    // Set TEAMS_SKIP_JWT_VALIDATION=true only in development/test environments.
    let skip_jwt = std::env::var("TEAMS_SKIP_JWT_VALIDATION").as_deref() == Ok("true");
    if !skip_jwt {
        let auth_header = headers.get("Authorization").ok_or_else(|| {
            ApiError(billforge_core::Error::Validation(
                "Missing Authorization header".to_string(),
            ))
        })?;
        let auth_str = auth_header.to_str().map_err(|_| {
            ApiError(billforge_core::Error::Validation(
                "Invalid Authorization header".to_string(),
            ))
        })?;
        if !auth_str.starts_with("Bearer ") {
            return Err(ApiError(billforge_core::Error::Validation(
                "Authorization header must be a Bearer token".to_string(),
            )));
        }
        let token = &auth_str[7..];
        // Production: decode and validate the JWT against the Microsoft
        // OpenID discovery endpoint (issuer, audience, expiry).
        // Reject expired or malformed tokens here.
        if token.is_empty() {
            return Err(ApiError(billforge_core::Error::Validation(
                "Empty Bearer token".to_string(),
            )));
        }
    }

    let action = body.action.as_deref().unwrap_or("");
    let invoice_id = body.invoice_id.ok_or_else(|| {
        ApiError(billforge_core::Error::Validation(
            "Missing invoice_id".to_string(),
        ))
    })?;
    let tenant_id = body.tenant_id.ok_or_else(|| {
        ApiError(billforge_core::Error::Validation(
            "Missing tenant_id".to_string(),
        ))
    })?;

    // Resolve the actor from the teams_webhooks table for this tenant.
    // In the future the Teams Adaptive Card payload should carry a signed
    // `acting_user_id` so we can verify the exact caller. For now we pick
    // the first active webhook user for the tenant as the approver.
    let pool = state.db.metadata();
    let actor_row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM teams_webhooks WHERE tenant_id = $1 AND is_active = true LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;

    let actor_id = actor_row
        .ok_or_else(|| {
            ApiError(billforge_core::Error::Validation(
                "No active Teams webhook configuration found for this tenant. \
                 Cannot attribute action to a real user."
                    .to_string(),
            ))
        })?
        .0;

    let tenant_pool = state
        .db
        .tenant(&TenantId(tenant_id))
        .await
        .map_err(ApiError)?;

    match action {
        "approve" => {
            transition_invoice(
                &tenant_pool,
                &TenantId(tenant_id),
                &invoice_id,
                &actor_id,
                crate::state_machine::InvoiceStatus::Approved,
                "approve_via_teams",
                serde_json::json!({ "channel": "teams" }),
            )
            .await?;
        }
        "reject" => {
            transition_invoice(
                &tenant_pool,
                &TenantId(tenant_id),
                &invoice_id,
                &actor_id,
                crate::state_machine::InvoiceStatus::Rejected,
                "reject_via_teams",
                serde_json::json!({ "channel": "teams" }),
            )
            .await?;
        }
        "request_changes" | "reassign" | "comment" => {
            let event_type = format!("{}_via_teams", action);
            let mut meta = serde_json::json!({ "action": action });
            if let Some(comment) = &body.comment_body {
                meta["comment_body"] = serde_json::Value::String(comment.clone());
            }
            if let Some(reassign_to) = &body.reassign_to_user_id {
                meta["reassign_to_user_id"] = serde_json::Value::String(reassign_to.to_string());
            }
            write_chat_audit(
                &tenant_pool,
                tenant_id,
                invoice_id,
                actor_id,
                &event_type,
                "teams",
                None,
                meta,
            )
            .await?;
        }
        _ => {
            return Err(ApiError(billforge_core::Error::Validation(format!(
                "Unknown action: {}",
                action
            ))));
        }
    }

    Ok(StatusCode::OK)
}
