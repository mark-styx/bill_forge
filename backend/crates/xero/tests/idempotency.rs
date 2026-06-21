//! Integration tests for Xero outbound idempotency.
//!
//! Verifies that the `Idempotency-Key` header is sent on POSTs and preserved
//! across retries inside `execute_with_retry`, so Xero can dedup a retried
//! create on its side.

use billforge_xero::{XeroClient, XeroEnvironment};
use std::sync::{Arc, Mutex};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

struct RecordingResponder {
    seen: Arc<Mutex<Vec<Option<String>>>>,
    attempts: Arc<Mutex<u32>>,
}

impl Respond for RecordingResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let key = request
            .headers
            .get("Idempotency-Key")
            .map(|v| v.to_str().unwrap_or_default().to_string());
        self.seen.lock().unwrap().push(key);

        let mut attempts = self.attempts.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            // First attempt: 429 to force a retry. Use Retry-After: 0 so the
            // test doesn't slow down the suite.
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("rate limited")
        } else {
            ResponseTemplate::new(200).set_body_string(r#"{"Items":[]}"#)
        }
    }
}

#[tokio::test]
async fn idempotency_key_is_stable_across_retries() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/api.xro/2.0/Invoices", mock_server.uri());

    let seen = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
    let attempts = Arc::new(Mutex::new(0u32));

    Mock::given(method("POST"))
        .respond_with(RecordingResponder {
            seen: seen.clone(),
            attempts: attempts.clone(),
        })
        .mount(&mock_server)
        .await;

    let client = XeroClient::new(
        "test-access-token".to_string(),
        "test-tenant-id".to_string(),
        XeroEnvironment::Production,
    );

    let idempotency_key = "stable-test-idempotency-key-9999";
    let result = client
        .execute_post_for_test(&url, b"{}".to_vec(), Some(idempotency_key))
        .await;

    assert!(
        result.is_ok(),
        "Expected POST to succeed on retry, got: {:?}",
        result.err()
    );

    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.len(),
        2,
        "Expected two attempts (initial 429 + retry 200), got {}",
        seen.len()
    );
    assert_eq!(
        seen[0].as_deref(),
        Some(idempotency_key),
        "First attempt missing Idempotency-Key header"
    );
    assert_eq!(
        seen[1].as_deref(),
        Some(idempotency_key),
        "Retry used a different Idempotency-Key: {:?} vs {:?}",
        seen[1],
        idempotency_key
    );
}

struct HeaderProbeResponder {
    saw_header: Arc<Mutex<Option<bool>>>,
}

impl Respond for HeaderProbeResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        *self.saw_header.lock().unwrap() = Some(request.headers.contains_key("Idempotency-Key"));
        ResponseTemplate::new(200).set_body_string(r#"{"Items":[]}"#)
    }
}

/// Sanity check: when no key is passed, the header is absent.
#[tokio::test]
async fn no_idempotency_key_when_none_passed() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/api.xro/2.0/Contacts", mock_server.uri());

    let saw_header: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));

    Mock::given(method("POST"))
        .respond_with(HeaderProbeResponder {
            saw_header: saw_header.clone(),
        })
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = XeroClient::new(
        "test-access-token".to_string(),
        "test-tenant-id".to_string(),
        XeroEnvironment::Production,
    );

    let result = client
        .execute_post_for_test(&url, b"{}".to_vec(), None)
        .await;
    assert!(result.is_ok(), "Expected POST to succeed: {:?}", result.err());

    let was_present = saw_header.lock().unwrap();
    assert_eq!(
        *was_present,
        Some(false),
        "Idempotency-Key header should be absent when key is None"
    );
    mock_server.verify().await;
}
