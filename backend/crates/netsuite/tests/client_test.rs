//! Integration tests for NetSuite client

use billforge_netsuite::{ClientError, NetSuiteClient, NetSuiteConfig};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config_with_base(base_url: &str) -> NetSuiteConfig {
    NetSuiteConfig {
        account_id: "TSTDRV1234567".to_string(),
        client_id: "test_client".to_string(),
        client_secret: "test_secret".to_string(),
        base_url: Some(base_url.to_string()),
    }
}

#[tokio::test]
async fn test_authenticate_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/services/rest/auth/oauth2/v1/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"tok_123","token_type":"Bearer"}"#),
        )
        .mount(&mock_server)
        .await;

    let mut client = NetSuiteClient::new(config_with_base(&mock_server.uri()));
    let result = client.authenticate().await;

    assert!(result.is_ok(), "authenticate should succeed: {:?}", result);
}

#[tokio::test]
async fn test_list_vendors_parses_items() {
    let mock_server = MockServer::start().await;

    // First: authenticate mock
    Mock::given(method("POST"))
        .and(path("/services/rest/auth/oauth2/v1/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"tok_123"}"#),
        )
        .mount(&mock_server)
        .await;

    // Second: vendor list mock with Bearer header matcher
    Mock::given(method("GET"))
        .and(path("/services/rest/record/v1/vendor"))
        .and(header("Authorization", "Bearer tok_123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                r#"{"items":[{"id":"42","companyName":"Acme","email":"ap@acme.com"}]}"#,
            ),
        )
        .mount(&mock_server)
        .await;

    let mut client = NetSuiteClient::new(config_with_base(&mock_server.uri()));
    client.authenticate().await.expect("auth should succeed");

    let vendors = client.list_vendors().await.expect("list_vendors should succeed");
    assert_eq!(vendors.len(), 1);
    assert_eq!(vendors[0].id, "42");
    assert_eq!(vendors[0].company_name.as_deref(), Some("Acme"));
    assert_eq!(vendors[0].email.as_deref(), Some("ap@acme.com"));
}

#[tokio::test]
async fn test_list_vendors_requires_auth() {
    let mock_server = MockServer::start().await;

    // No mocks needed – should fail before making any request
    let client = NetSuiteClient::new(config_with_base(&mock_server.uri()));
    let result = client.list_vendors().await;

    assert!(result.is_err(), "list_vendors should fail without auth");
    match result.unwrap_err() {
        ClientError::MissingToken => {}
        other => panic!("Expected MissingToken, got {:?}", other),
    }
}
