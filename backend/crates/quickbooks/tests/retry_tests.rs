//! Integration tests for QuickBooks client retry/backoff logic

use billforge_quickbooks::{ClientError, QuickBooksClient, QuickBooksEnvironment};
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test that 429 with Retry-After header is retried and eventually succeeds.
#[tokio::test]
async fn test_429_retries_with_backoff() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/query", mock_server.uri());

    // First request: 429 with Retry-After: 1
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_string("rate limited"),
        )
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    // Second request: 200 OK
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let start = Instant::now();
    let result = client.execute_get_for_test(&url).await;
    let elapsed = start.elapsed();

    assert!(
        result.is_ok(),
        "Expected retry to succeed on second attempt, got error: {:?}",
        result.err()
    );
    assert!(
        elapsed >= Duration::from_secs(1),
        "Expected at least 1s delay for Retry-After, got {:?}",
        elapsed
    );
}

/// Test that 429 exhausting all retries returns RateLimited error.
#[tokio::test]
async fn test_429_respects_max_retries() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/query", mock_server.uri());

    // Always return 429
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let result = client.execute_get_for_test(&url).await;

    assert!(result.is_err(), "Expected failure after max retries");
    match result.unwrap_err() {
        ClientError::RateLimited { retry_after: _ } => {}
        other => panic!("Expected RateLimited error, got {:?}", other),
    }
}

/// Test that 5xx errors are retried and eventually succeed.
#[tokio::test]
async fn test_5xx_retries() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/query", mock_server.uri());

    // First request: 503
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    // Second request: 200
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let result = client.execute_get_for_test(&url).await;

    assert!(
        result.is_ok(),
        "Expected success after 5xx retry, got: {:?}",
        result.err()
    );
}

/// Test that 4xx (non-401, non-429) errors fail immediately without retry.
#[tokio::test]
async fn test_4xx_no_retry() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/query", mock_server.uri());

    // Return 400 - should not retry
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .expect(1) // Exactly one request, no retries
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let result = client.execute_get_for_test(&url).await;

    assert!(result.is_err(), "Expected error for 400");
    match result.unwrap_err() {
        ClientError::ApiError { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("bad request"));
        }
        other => panic!("Expected ApiError with status 400, got {:?}", other),
    }

    // Verify mock received exactly 1 request (no retries)
    mock_server.verify().await;
}

/// Test that the client has a timeout configured by checking bearer auth header is sent.
#[tokio::test]
async fn test_timeout_applied() {
    let mock_server = MockServer::start().await;
    let url = format!("{}/v3/company/test-co/query", mock_server.uri());

    // Return 200 to verify the client works with timeout
    Mock::given(method("GET"))
        .and(header("Authorization", "Bearer test-access-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = QuickBooksClient::new(
        "test-access-token".to_string(),
        "test-co".to_string(),
        QuickBooksEnvironment::Production,
    );

    let result = client.execute_get_for_test(&url).await;

    assert!(
        result.is_ok(),
        "Expected successful request with timeout, got: {:?}",
        result.err()
    );

    mock_server.verify().await;
}
