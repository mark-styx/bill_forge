//! Integration tests for the Stripe client using wiremock.
//!
//! These tests prove that StripeClient parses real-shaped API responses
//! rather than fabricating data locally.

use billforge_billing::stripe::{
    CreateCustomerParams, StripeClient, StripeSubscription,
};
use std::collections::HashMap;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn make_client(server: &MockServer) -> StripeClient {
    StripeClient::new_with_base_url(
        "sk_test_abc123".to_string(),
        server.uri(),
    )
}

#[tokio::test]
async fn create_customer_posts_form_and_parses_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/customers"))
        .and(header("authorization", "Bearer sk_test_abc123"))
        .and(body_string_contains("email"))
        .and(body_string_contains("user%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "cus_ABC123",
            "email": "user@example.com",
            "name": "Jane Doe",
            "metadata": {}
        })))
        .mount(&server)
        .await;

    let client = make_client(&server).await;
    let customer = client
        .create_customer(CreateCustomerParams {
            email: "user@example.com".to_string(),
            name: Some("Jane Doe".to_string()),
            metadata: HashMap::new(),
        })
        .await
        .expect("create_customer should succeed");

    // The ID must come from the server response, not from a fabricated UUID.
    assert_eq!(customer.id, "cus_ABC123");
    assert_eq!(customer.email, "user@example.com");
    assert_eq!(customer.name.as_deref(), Some("Jane Doe"));
}

#[tokio::test]
async fn get_subscription_parses_status_from_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/sub_X"))
        .and(header("authorization", "Bearer sk_test_abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "sub_X",
            "customer": "cus_ABC123",
            "status": "past_due",
            "current_period_start": 1700000000,
            "current_period_end": 1702678400,
            "cancel_at_period_end": false,
            "items": {
                "data": []
            }
        })))
        .mount(&server)
        .await;

    let client = make_client(&server).await;
    let sub: StripeSubscription = client
        .get_subscription("sub_X")
        .await
        .expect("get_subscription should succeed");

    // Status must come from the server, not be hardcoded to "active".
    assert_eq!(sub.status, "past_due");
    assert_eq!(sub.id, "sub_X");
    assert_eq!(sub.customer, "cus_ABC123");
}
