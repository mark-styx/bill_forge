//! Integration tests for NetSuite client.
//!
//! The connect path is intentionally unimplemented (real NetSuite OAuth 2.0
//! M2M requires JWT client_assertion signing). These tests assert the typed
//! `JwtNotImplemented` failure surface rather than exercising HTTP mocks for
//! a flow that does not exist in production.

use billforge_netsuite::{ClientError, NetSuiteClient, NetSuiteConfig};

fn fake_config() -> NetSuiteConfig {
    NetSuiteConfig {
        account_id: "TSTDRV1234567".to_string(),
        client_id: "test_client".to_string(),
        client_secret: "test_secret".to_string(),
        base_url: Some("http://127.0.0.1:1".to_string()),
    }
}

#[tokio::test]
async fn test_authenticate_returns_jwt_not_implemented() {
    let mut client = NetSuiteClient::new(fake_config());
    let result = client.authenticate().await;

    match result {
        Err(ClientError::JwtNotImplemented) => {}
        other => panic!("Expected ClientError::JwtNotImplemented, got {:?}", other),
    }
}

#[tokio::test]
async fn test_list_vendors_requires_auth() {
    let client = NetSuiteClient::new(fake_config());
    let result = client.list_vendors().await;

    assert!(result.is_err(), "list_vendors should fail without auth");
    match result.unwrap_err() {
        ClientError::MissingToken => {}
        other => panic!("Expected MissingToken, got {:?}", other),
    }
}
