//! Integration tests for QuickBooks outbound idempotency.
//!
//! Verifies that the `requestid` query parameter is attached to POSTs and is
//! preserved across retries inside `execute_with_retry`, so QBO can dedup a
//! retried create on its side.

use billforge_quickbooks::{QuickBooksClient, QuickBooksEnvironment};
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Custom responder that records every requestid query value seen, then
/// returns 503 once, followed by 200. The closure-based responder lets us
/// inspect requests across retries.
struct RecordingResponder {
    seen: Arc<Mutex<Vec<Option<String>>>>,
    attempts: Arc<Mutex<u32>>,
}

impl Respond for RecordingResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let request_id = request
            .url
            .query_pairs()
            .find(|(k, _)| k == "requestid")
            .map(|(_, v)| v.into_owned());
        self.seen.lock().unwrap().push(request_id);

        let mut attempts = self.attempts.lock().unwrap();
        *attempts += 1;
        if *attempts == 1 {
            ResponseTemplate::new(503).set_body_string("service unavailable")
        } else {
            ResponseTemplate::new(200).set_body_string("{}")
        }
    }
}

#[tokio::test]
async fn requestid_is_stable_across_retries() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/bill", mock_server.uri());

    let seen = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
    let attempts = Arc::new(Mutex::new(0u32));

    Mock::given(method("POST"))
        .respond_with(RecordingResponder {
            seen: seen.clone(),
            attempts: attempts.clone(),
        })
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let request_id = "stable-test-request-id-1234";
    let result = client
        .execute_post_for_test(&url, b"{}".to_vec(), Some(request_id))
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
        "Expected two attempts (initial 503 + retry 200), got {}",
        seen.len()
    );
    assert_eq!(
        seen[0].as_deref(),
        Some(request_id),
        "First attempt missing requestid"
    );
    assert_eq!(
        seen[1].as_deref(),
        Some(request_id),
        "Retry attempt used a different requestid: {:?} vs {:?}",
        seen[1],
        request_id
    );
}

/// Sanity check: when no requestid is passed, the parameter is absent.
#[tokio::test]
async fn no_requestid_when_none_passed() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/vendor", mock_server.uri());

    Mock::given(method("POST"))
        .and(query_param("requestid", "should-not-appear"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let result = client
        .execute_post_for_test(&url, b"{}".to_vec(), None)
        .await;
    assert!(result.is_ok(), "Expected POST to succeed: {:?}", result.err());

    mock_server.verify().await;
}
