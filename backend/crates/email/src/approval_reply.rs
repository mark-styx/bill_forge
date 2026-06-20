//! Reply-by-email approval ingest.
//!
//! Approvers receive invoice-approval notification emails and reply directly
//! from their inbox with `APPROVE`, `REJECT [reason]`, or `DELEGATE user@co.com`.
//! The signed action token that was minted for the notification is recovered
//! from one of three carriers (Reply-To plus-address, subject marker, or hidden
//! HTML span), validated, and consumed as a single-use credential to perform
//! the approval action with the same audit trail as the link-click path.

use crate::inbound::{extract_email, InboundEmailPayload};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Reply command parsing
// ---------------------------------------------------------------------------

/// A recognised approver reply command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplyCommand {
    Approve,
    Reject(Option<String>),
    Delegate(String),
}

/// Strip quoted lines, signature blocks, and provider quote headers from the
/// reply body, then return the first non-empty actionable line.
fn first_actionable_line(body_text: &str) -> Option<String> {
    for raw in body_text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Quoted prior content (gmail/outlook style)
        if line.starts_with('>') {
            continue;
        }
        // Signature delimiter "-- "
        if line == "--" || line == "-- " {
            return None;
        }
        // "On <date>, <person> wrote:" quote header
        let lower = line.to_lowercase();
        if lower.starts_with("on ") && lower.contains("wrote:") {
            return None;
        }
        // "From: ..." style forwarded/quoted header
        if lower.starts_with("from:") || lower.starts_with("sent from") {
            return None;
        }
        return Some(line.to_string());
    }
    None
}

/// Parse the first actionable line of a reply body into a reply command.
pub fn parse_reply_command(body_text: &str) -> Option<ReplyCommand> {
    let line = first_actionable_line(body_text)?;
    let upper = line.to_uppercase();

    if upper == "APPROVE" || upper.starts_with("APPROVE ") || upper.starts_with("APPROVED") {
        return Some(ReplyCommand::Approve);
    }

    if upper == "REJECT" || upper.starts_with("REJECT ") || upper.starts_with("REJECTED") {
        let rest = line
            .chars()
            .skip("REJECT".len())
            .collect::<String>()
            .trim()
            .trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace())
            .to_string();
        let reason = if rest.is_empty() { None } else { Some(rest) };
        return Some(ReplyCommand::Reject(reason));
    }

    if upper.starts_with("DELEGATE ") || upper.starts_with("DELEGATE\t") {
        let rest = line
            .chars()
            .skip("DELEGATE".len())
            .collect::<String>();
        let email = rest
            .split_whitespace()
            .find(|w| w.contains('@'))
            .map(|s| s.trim_matches(|c: char| c == '<' || c == '>' || c == ',' || c == ';'))
            .map(|s| s.to_string())?;
        return Some(ReplyCommand::Delegate(email));
    }

    None
}

// ---------------------------------------------------------------------------
// Token extraction
// ---------------------------------------------------------------------------

/// Recover the signed action token associated with an approval-reply email.
///
/// Carriers (in priority order):
/// 1. `Reply-To: approvals+<token>@<inbox-domain>` plus-addressing.
/// 2. Subject line marker `[BF-APR-<token>]` (survives `Re:` chains).
/// 3. Hidden HTML span `[bf-token:<token>]` carried through quoted replies.
pub fn extract_action_token(msg: &InboundEmailPayload) -> Option<String> {
    if let Some(reply_to) = msg.reply_to.as_deref() {
        if let Some(tok) = extract_token_from_plus_address(reply_to) {
            return Some(tok);
        }
    }
    // Some providers don't surface the original Reply-To; fall back to the
    // To: address which also carries the plus-addressed mailbox on direct
    // replies that aren't run through Reply-To rewriting.
    if let Some(tok) = extract_token_from_plus_address(&msg.to) {
        return Some(tok);
    }

    if let Some(subject) = msg.subject.as_deref() {
        if let Some(tok) = extract_token_from_subject(subject) {
            return Some(tok);
        }
    }

    if let Some(html) = msg.html_body.as_deref() {
        if let Some(tok) = extract_token_from_html(html) {
            return Some(tok);
        }
    }
    if let Some(text) = msg.text_body.as_deref() {
        if let Some(tok) = extract_token_from_html(text) {
            return Some(tok);
        }
    }

    None
}

fn extract_token_from_plus_address(raw: &str) -> Option<String> {
    let email = extract_email(raw);
    let local = email.split('@').next()?;
    let suffix = local.strip_prefix("approvals+")?;
    if suffix.is_empty() {
        None
    } else {
        Some(suffix.to_string())
    }
}

fn extract_token_from_subject(subject: &str) -> Option<String> {
    // Looks for "[BF-APR-<token>]".
    let start = subject.find("[BF-APR-")?;
    let after = &subject[start + "[BF-APR-".len()..];
    let end = after.find(']')?;
    let token = &after[..end];
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn extract_token_from_html(body: &str) -> Option<String> {
    // Looks for "[bf-token:<token>]".
    let start = body.find("[bf-token:")?;
    let after = &body[start + "[bf-token:".len()..];
    let end = after.find(']')?;
    let token = &after[..end];
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

// ---------------------------------------------------------------------------
// Outcome + handler
// ---------------------------------------------------------------------------

/// What happened when processing an approval reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalReplyOutcome {
    /// The reply was applied to the approval request (Approve / Reject / Delegate).
    Applied {
        action: &'static str,
        invoice_id: Uuid,
    },
    /// The token was valid but the reply command failed (e.g. already consumed,
    /// no matching pending request). The original inbound email is routed to
    /// triage with the given reason.
    Triaged { reason: String },
}

/// Process a reply-as-approval email end-to-end.
///
/// - Validates and consumes the signed token via `EmailActionTokenService`.
/// - Maps the `ReplyCommand` to the existing approval-request mutation,
///   matching the link-click semantics in `routes::email_actions::update_approval_request`.
/// - Writes an audit log entry with `channel = email_reply`.
///
/// Returns `Triaged` (with a reason that the caller will record in
/// `email_triage_queue`) on validation / replay / unknown-target failures.
pub async fn handle_approval_reply(
    metadata_pool: &PgPool,
    tenant_pool: &PgPool,
    inbound_email_id: Uuid,
    _msg: &InboundEmailPayload,
    token: &str,
    cmd: &ReplyCommand,
) -> Result<ApprovalReplyOutcome, String> {
    use billforge_core::services::EmailActionTokenService;
    use std::sync::Arc;

    let secret =
        std::env::var("TOKEN_SECRET_KEY").unwrap_or_else(|_| "secret".to_string());
    let token_service =
        EmailActionTokenService::new(Arc::new(metadata_pool.clone()), secret);

    let token_data = match token_service.validate_token(token).await {
        Ok(t) => t,
        Err(e) => {
            let reason = format!("approval_reply_failed: invalid_token: {}", e);
            tracing::warn!(
                inbound_email_id = %inbound_email_id,
                error = %e,
                "Approval reply rejected: token validation failed"
            );
            return Ok(ApprovalReplyOutcome::Triaged { reason });
        }
    };

    let tenant_uuid = Uuid::parse_str(&token_data.tenant_id)
        .map_err(|e| format!("invalid tenant id in token: {}", e))?;
    let invoice_id = token_data.resource_id;
    let user_uuid = token_data.user_id;

    let (action_str, new_status, reason_text, delegate_email) = match cmd {
        ReplyCommand::Approve => ("approved", "approved", None, None),
        ReplyCommand::Reject(reason) => ("rejected", "rejected", reason.clone(), None),
        ReplyCommand::Delegate(target) => ("delegated", "delegated", None, Some(target.clone())),
    };

    // Apply mutation to approval_requests in TENANT DB. Mirrors the matching
    // logic in routes::email_actions::update_approval_request: direct user
    // target first, then AnyOf group membership, then delegate fallback.
    let direct_rows = sqlx::query(
        r#"UPDATE approval_requests
           SET status = $1,
               responded_by = $2,
               responded_at = NOW(),
               updated_at = NOW(),
               comments = COALESCE($3, comments)
           WHERE tenant_id = $4 AND invoice_id = $5 AND status = 'pending'
             AND (
               requested_from->>'User' = $6
               OR (requested_from ? 'AnyOf' AND requested_from->'AnyOf' @> to_jsonb($6::text))
             )"#,
    )
    .bind(new_status)
    .bind(user_uuid)
    .bind(reason_text.as_deref())
    .bind(tenant_uuid)
    .bind(invoice_id)
    .bind(user_uuid.to_string())
    .execute(tenant_pool)
    .await
    .map_err(|e| format!("Failed to update approval_requests: {}", e))?
    .rows_affected();

    let mut delegate_rows = 0u64;
    if direct_rows == 0 {
        delegate_rows = sqlx::query(
            r#"UPDATE approval_requests ar
               SET status = $1,
                   responded_by = $2,
                   responded_at = NOW(),
                   updated_at = NOW(),
                   comments = COALESCE($3, ar.comments)
               FROM approval_delegations ad
               WHERE ar.tenant_id = $4 AND ar.invoice_id = $5 AND ar.status = 'pending'
                 AND ad.tenant_id = ar.tenant_id
                 AND ad.delegate_id = $2
                 AND ad.is_active = true
                 AND ad.start_date <= NOW()
                 AND ad.end_date > NOW()
                 AND (
                   ar.requested_from->>'User' = ad.delegator_id::text
                   OR (ar.requested_from ? 'AnyOf' AND ar.requested_from->'AnyOf' @> to_jsonb(ad.delegator_id::text))
                 )"#,
        )
        .bind(new_status)
        .bind(user_uuid)
        .bind(reason_text.as_deref())
        .bind(tenant_uuid)
        .bind(invoice_id)
        .execute(tenant_pool)
        .await
        .map_err(|e| format!("Failed to update via delegation: {}", e))?
        .rows_affected();
    }

    if direct_rows == 0 && delegate_rows == 0 {
        let reason = "approval_reply_failed: no_matching_pending_approval".to_string();
        tracing::warn!(
            inbound_email_id = %inbound_email_id,
            invoice_id = %invoice_id,
            user_id = %user_uuid,
            "Approval reply: token valid but no matching pending approval request"
        );
        return Ok(ApprovalReplyOutcome::Triaged { reason });
    }

    // For DELEGATE, also create a new pending approval_request targeted at
    // the delegate so the workflow continues. If the delegate email can't be
    // resolved to a user we still record the delegation but flag triage.
    let mut delegate_user_resolved = true;
    if let Some(ref target_email) = delegate_email {
        match resolve_user_by_email(tenant_pool, tenant_uuid, target_email).await? {
            Some(delegate_user_id) => {
                sqlx::query(
                    r#"INSERT INTO approval_requests
                           (id, tenant_id, invoice_id, requested_from, status, created_at, updated_at)
                       VALUES ($1, $2, $3, $4, 'pending', NOW(), NOW())"#,
                )
                .bind(Uuid::new_v4())
                .bind(tenant_uuid)
                .bind(invoice_id)
                .bind(serde_json::json!({ "User": delegate_user_id.to_string() }))
                .execute(tenant_pool)
                .await
                .map_err(|e| format!("Failed to insert delegated approval request: {}", e))?;
            }
            None => {
                delegate_user_resolved = false;
                tracing::warn!(
                    invoice_id = %invoice_id,
                    target_email = %target_email,
                    "Delegate target email did not resolve to a tenant user; original request was marked delegated but no new approval was created"
                );
            }
        }
    }

    // Mark token consumed (single-use semantics).
    if let Err(e) = token_service.mark_used(token).await {
        tracing::warn!(error = %e, "Failed to mark approval-reply token used");
    }

    // Audit log entry in METADATA DB. Mirrors the metadata written by
    // routes::email_actions::handle_email_action for parity with link clicks.
    let audit_action = match cmd {
        ReplyCommand::Approve => "invoice_approved",
        ReplyCommand::Reject(_) => "invoice_rejected",
        ReplyCommand::Delegate(_) => "update",
    };
    let metadata_json = serde_json::json!({
        "channel": "email_reply",
        "token_nonce": token_data.nonce.to_string(),
        "inbound_email_id": inbound_email_id.to_string(),
        "command": action_str,
        "reject_reason": reason_text,
        "delegate_target_email": delegate_email,
        "delegate_user_resolved": delegate_user_resolved,
        "via_delegation_fallback": direct_rows == 0 && delegate_rows > 0,
    });
    let changes = serde_json::json!({
        "description": format!("Invoice {} via email reply", action_str),
        "metadata": metadata_json,
    });

    if let Err(e) = sqlx::query(
        r#"INSERT INTO audit_log (
               id, tenant_id, user_id, action, resource_type, resource_id,
               changes, created_at
           ) VALUES ($1, $2, $3, $4, 'invoice', $5, $6, $7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_uuid)
    .bind(user_uuid)
    .bind(audit_action)
    .bind(invoice_id.to_string())
    .bind(&changes)
    .bind(Utc::now())
    .execute(metadata_pool)
    .await
    {
        tracing::error!(
            error = %e,
            audit = %changes,
            "SOX: failed to persist approval-reply audit entry"
        );
    }

    Ok(ApprovalReplyOutcome::Applied {
        action: action_str,
        invoice_id,
    })
}

async fn resolve_user_by_email(
    tenant_pool: &PgPool,
    tenant_uuid: Uuid,
    raw_email: &str,
) -> Result<Option<Uuid>, String> {
    let email = extract_email(raw_email);
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND LOWER(email) = LOWER($2)")
            .bind(tenant_uuid)
            .bind(email)
            .fetch_optional(tenant_pool)
            .await
            .map_err(|e| format!("Failed to resolve delegate email: {}", e))?;
    Ok(user_id)
}

// ---------------------------------------------------------------------------
// Tests (pure-function coverage; DB-backed cases live in the tests/ dir)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn payload_with(reply_to: Option<&str>, subject: Option<&str>, html: Option<&str>) -> InboundEmailPayload {
        InboundEmailPayload {
            from: "approver@acme.com".to_string(),
            to: "ap@meridian.billforge.com".to_string(),
            subject: subject.map(|s| s.to_string()),
            message_id: None,
            text_body: None,
            html_body: html.map(|s| s.to_string()),
            reply_to: reply_to.map(|s| s.to_string()),
            attachments: vec![],
        }
    }

    #[test]
    fn parses_approve() {
        assert_eq!(parse_reply_command("APPROVE"), Some(ReplyCommand::Approve));
        assert_eq!(parse_reply_command("approve"), Some(ReplyCommand::Approve));
        assert_eq!(
            parse_reply_command("Approve\n--\nSent from my iPhone"),
            Some(ReplyCommand::Approve)
        );
    }

    #[test]
    fn parses_reject_with_reason() {
        assert_eq!(
            parse_reply_command("REJECT duplicate invoice"),
            Some(ReplyCommand::Reject(Some("duplicate invoice".to_string())))
        );
        assert_eq!(
            parse_reply_command("reject"),
            Some(ReplyCommand::Reject(None))
        );
    }

    #[test]
    fn parses_delegate() {
        assert_eq!(
            parse_reply_command("DELEGATE jane@acme.com"),
            Some(ReplyCommand::Delegate("jane@acme.com".to_string()))
        );
        assert_eq!(
            parse_reply_command("delegate <jane@acme.com>"),
            Some(ReplyCommand::Delegate("jane@acme.com".to_string()))
        );
    }

    #[test]
    fn ignores_quoted_lines_and_garbage() {
        assert_eq!(
            parse_reply_command("> APPROVE\n> previous email"),
            None
        );
        assert_eq!(parse_reply_command("thanks!"), None);
        assert_eq!(parse_reply_command(""), None);
    }

    #[test]
    fn extracts_token_from_plus_address() {
        let msg = payload_with(Some("\"Approvals\" <approvals+abc.def@billforge.com>"), None, None);
        assert_eq!(extract_action_token(&msg), Some("abc.def".to_string()));
    }

    #[test]
    fn extracts_token_from_subject_marker() {
        let msg = payload_with(None, Some("Re: [BF-APR-tok123] Approval Required"), None);
        assert_eq!(extract_action_token(&msg), Some("tok123".to_string()));
    }

    #[test]
    fn extracts_token_from_hidden_html_span() {
        let html = r#"<p>Yes</p><span style="display:none">[bf-token:hidden-tok]</span>"#;
        let msg = payload_with(None, None, Some(html));
        assert_eq!(extract_action_token(&msg), Some("hidden-tok".to_string()));
    }

    #[test]
    fn returns_none_when_no_carrier_present() {
        let msg = payload_with(None, Some("Re: Approval Required"), Some("<p>ok</p>"));
        assert_eq!(extract_action_token(&msg), None);
    }
}
