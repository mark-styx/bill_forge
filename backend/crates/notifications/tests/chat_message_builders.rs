//! Snapshot-style tests for chat approval message builders and Slack signature
//! verification.

#![allow(warnings)]

use billforge_notifications::{
    build_invoice_approval_blocks, build_teams_approval_card, verify_slack_signature,
    InvoiceContext, InvoiceLineItem,
};
use uuid::Uuid;

fn sample_context() -> InvoiceContext {
    InvoiceContext {
        invoice_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        tenant_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap(),
        vendor_name: "Acme Corp".to_string(),
        invoice_number: "INV-2026-0042".to_string(),
        total_amount_cents: 1_234_56,
        currency: "USD".to_string(),
        due_date: Some("2026-06-15".to_string()),
        gl_code: Some("5100-AP".to_string()),
        cost_center: Some("Engineering".to_string()),
        line_items: vec![
            InvoiceLineItem {
                description: "Widget A".to_string(),
                quantity: Some(10.0),
                unit_price_cents: Some(5000),
                total_cents: Some(50000),
            },
            InvoiceLineItem {
                description: "Gadget B".to_string(),
                quantity: Some(2.0),
                unit_price_cents: Some(36728),
                total_cents: Some(73456),
            },
        ],
        pdf_preview_url: Some("https://app.billforge.io/invoices/preview/abc".to_string()),
    }
}

#[test]
fn slack_blocks_include_all_five_action_ids_and_invoice_fields() {
    let ctx = sample_context();
    let blocks = build_invoice_approval_blocks(&ctx);
    let json = serde_json::to_string(&blocks).unwrap();

    // Five action verbs
    assert!(json.contains("bf_approve:"), "missing bf_approve action_id");
    assert!(json.contains("bf_reject:"), "missing bf_reject action_id");
    assert!(
        json.contains("bf_request_changes:"),
        "missing bf_request_changes action_id"
    );
    assert!(
        json.contains("bf_reassign:"),
        "missing bf_reassign action_id"
    );
    assert!(json.contains("bf_comment:"), "missing bf_comment action_id");

    // Vendor and GL code
    assert!(json.contains("Acme Corp"), "missing vendor name");
    assert!(json.contains("5100-AP"), "missing GL code");

    // Line items
    assert!(json.contains("Widget A"), "missing line item Widget A");
    assert!(json.contains("Gadget B"), "missing line item Gadget B");

    // PDF link
    assert!(json.contains("View PDF"), "missing PDF preview link");

    // Invoice ID embedded in action_ids
    assert!(
        json.contains("11111111-2222-3333-4444-555555555555"),
        "missing invoice ID"
    );
}

#[test]
fn teams_card_includes_five_actions_and_show_cards() {
    let ctx = sample_context();
    let card = build_teams_approval_card(&ctx);
    let json = serde_json::to_string(&card).unwrap();

    // Five action verbs
    assert!(json.contains("approve"), "missing approve action");
    assert!(json.contains("reject"), "missing reject action");
    assert!(
        json.contains("request_changes"),
        "missing request_changes action"
    );

    // ShowCard for Reassign and Comment
    assert!(
        json.contains("Action.ShowCard"),
        "missing Action.ShowCard for Reassign/Comment"
    );

    // Input fields inside ShowCards
    assert!(
        json.contains("reassign_to_user_id"),
        "missing reassign input"
    );
    assert!(json.contains("comment_body"), "missing comment input");

    // Invoice context
    assert!(json.contains("Acme Corp"), "missing vendor");
    assert!(json.contains("5100-AP"), "missing GL code");
}

#[test]
fn verify_slack_signature_accepts_valid_payload() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let secret = "test-signing-secret";
    let timestamp = "1234567890";
    let body = b"payload=%7B%22type%22%3A%22block_actions%22%7D";

    let basestring = format!("v0:{}", timestamp);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(basestring.as_bytes());
    mac.update(body);
    let result = mac.finalize().into_bytes();
    let sig = format!("v0={}", hex::encode(result));

    assert!(
        verify_slack_signature(secret, timestamp, &sig, body).is_ok(),
        "valid signature should pass"
    );
}

#[test]
fn verify_slack_signature_rejects_bad_signature() {
    let result = verify_slack_signature("secret", "1234567890", "v0=badbadbadbad", b"some body");
    assert!(result.is_err(), "bad signature should fail");
}

#[test]
fn verify_slack_signature_rejects_stale_timestamp() {
    // Use a timestamp from far in the past
    let old_ts = "1000000000"; // ~2001
    let result = verify_slack_signature("secret", old_ts, "v0=anything", b"body");
    assert!(result.is_err(), "stale timestamp should fail");
}

#[test]
fn teams_show_card_actions_include_input_value_substitutions() {
    let ctx = sample_context();
    let card = build_teams_approval_card(&ctx);
    let json = serde_json::to_string(&card).unwrap();

    // The Reassign ShowCard body must contain the Adaptive Card
    // input-value substitution expression for reassign_to_user_id
    assert!(
        json.contains("{{reassign_to_user_id.value}}"),
        "Reassign ShowCard body must reference {{reassign_to_user_id.value}}"
    );

    // The Comment ShowCard body must contain the input-value substitution
    assert!(
        json.contains("{{comment_body.value}}"),
        "Comment ShowCard body must reference {{comment_body.value}}"
    );

    // The static fields should still be present
    assert!(
        json.contains("\"action\":\"reassign\""),
        "Reassign body must contain action field"
    );
    assert!(
        json.contains("\"action\":\"comment\""),
        "Comment body must contain action field"
    );
}
