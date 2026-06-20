//! Integration tests for the AI Decision Explainability ("Show Your Work")
//! endpoints (refs #409).
//!
//! Covers the response contract for the categorization explanation: the
//! response carries non-empty `top_signals`, at least one citation, a
//! counterfactual whose `alternative` differs from `current`, and serializes
//! Citation's `r#ref` field as plain `ref` (the JSON the frontend consumes).
//! Override request DTO is also exercised.
//!
//! Run: `cargo test -p billforge-api --test explain_categorization`

#![cfg(feature = "processing")]

use billforge_api::routes::explain::{
    Citation, Counterfactual, ExplanationResponse, OverrideRequest, OverrideResponse, Signal,
};
use serde_json::json;
use uuid::Uuid;

fn make_response() -> ExplanationResponse {
    ExplanationResponse {
        decision_id: Uuid::nil(),
        decision_kind: "categorization".to_string(),
        inputs: json!({
            "vendor_name": "Acme Software",
            "amount_cents": 50000_i64,
            "line_text": "Annual SaaS license",
            "current_gl_code": "6000-Software & Subscriptions",
        }),
        top_signals: vec![
            Signal {
                name: "keyword_match".to_string(),
                weight: 0.45,
                direction: "+".to_string(),
                value: "'software' keyword family".to_string(),
            },
            Signal {
                name: "vendor_history".to_string(),
                weight: 0.35,
                direction: "+".to_string(),
                value: "5 prior invoices from Acme Software".to_string(),
            },
            Signal {
                name: "model_confidence".to_string(),
                weight: 0.20,
                direction: "+".to_string(),
                value: "92% scoring confidence".to_string(),
            },
        ],
        citations: vec![
            Citation {
                kind: "keyword".to_string(),
                r#ref: "software".to_string(),
                span: "keyword family 'software' in line text".to_string(),
            },
            Citation {
                kind: "prior_coding".to_string(),
                r#ref: Uuid::nil().to_string(),
                span: "INV-1 — Acme Software (coded 6000-Software & Subscriptions)".to_string(),
            },
        ],
        counterfactual: Counterfactual {
            variable: "vendor".to_string(),
            current: "Acme Software".to_string(),
            alternative: "Marketing Maven Agency".to_string(),
            predicted_outcome: "7000-Marketing".to_string(),
        },
        current_outcome: "6000-Software & Subscriptions".to_string(),
        rationale_text: "Keyword 'software' + vendor history".to_string(),
    }
}

#[test]
fn response_has_inputs_signals_citations_counterfactual() {
    let resp = make_response();
    let v = serde_json::to_value(&resp).unwrap();

    assert_eq!(v["decision_kind"], "categorization");
    assert!(v["inputs"].is_object());
    assert!(v["inputs"]["vendor_name"].is_string());

    let signals = v["top_signals"].as_array().unwrap();
    assert!(!signals.is_empty(), "top_signals must be non-empty");
    for s in signals {
        assert!(s["name"].is_string());
        assert!(s["weight"].as_f64().is_some());
        assert!(s["direction"].is_string());
    }

    let citations = v["citations"].as_array().unwrap();
    assert!(!citations.is_empty(), "citations must be non-empty");
}

#[test]
fn citation_serializes_ref_field_not_raw_keyword() {
    // `Citation.r#ref` must serialize as plain `ref` so the frontend can
    // consume it without dealing with Rust's raw-identifier prefix.
    let resp = make_response();
    let v = serde_json::to_value(&resp).unwrap();
    let first = &v["citations"][0];
    assert!(first.get("ref").is_some(), "citation missing 'ref' field");
    assert!(
        first.get("r#ref").is_none(),
        "citation should not expose raw identifier"
    );
}

#[test]
fn counterfactual_alternative_differs_from_current() {
    let resp = make_response();
    assert_ne!(resp.counterfactual.current, resp.counterfactual.alternative);
    assert_eq!(resp.counterfactual.variable, "vendor");
    assert_eq!(resp.counterfactual.predicted_outcome, "7000-Marketing");
}

#[test]
fn override_request_parses_with_optional_reason() {
    let req: OverrideRequest = serde_json::from_value(json!({
        "corrected_gl_code": "7000-Marketing",
        "reason": "vendor switched to ad-buy"
    }))
    .unwrap();
    assert_eq!(req.corrected_gl_code, "7000-Marketing");
    assert_eq!(req.reason.as_deref(), Some("vendor switched to ad-buy"));
}

#[test]
fn override_request_accepts_missing_reason() {
    let req: OverrideRequest = serde_json::from_value(json!({
        "corrected_gl_code": "5000-Office Supplies & Equipment"
    }))
    .unwrap();
    assert!(req.reason.is_none());
}

#[test]
fn override_response_reports_correction_type_gl_recode() {
    let resp = OverrideResponse {
        recorded: true,
        correction_type: "gl_recode".to_string(),
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["recorded"], true);
    // Must be `gl_recode` so it lands in the same correction stream as
    // other categorization corrections (#404 / ContinuousLearningEngine).
    assert_eq!(v["correction_type"], "gl_recode");
}

#[test]
fn signal_weights_can_sum_to_unit() {
    // The route builder normalizes signal weights to a unit total so the UI
    // can render proportional bars. Confirm the response shape supports it.
    let resp = make_response();
    let total: f32 = resp.top_signals.iter().map(|s| s.weight).sum();
    assert!((total - 1.0).abs() < 1e-3, "signals sum to {}", total);
}
