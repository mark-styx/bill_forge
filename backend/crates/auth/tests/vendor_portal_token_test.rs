//! Round-trip tests for vendor-portal JWT tokens.

use billforge_auth::{Claims, JwtConfig, JwtService, TokenType};
use billforge_core::domain::VendorId;
use billforge_core::{Error, TenantId};

fn test_jwt_config() -> JwtConfig {
    JwtConfig {
        secret: "test-vendor-portal-secret-32ch!".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    }
}

#[test]
fn vendor_portal_token_round_trip() {
    let svc = JwtService::new(test_jwt_config());
    let tid = TenantId::new();
    let vid = VendorId(uuid::Uuid::new_v4());

    let token = svc
        .create_vendor_portal_token(&tid, &vid)
        .expect("create_vendor_portal_token");
    let claims: Claims = svc
        .validate_vendor_portal_token(&token)
        .expect("validate_vendor_portal_token");

    assert_eq!(claims.tenant_id, tid.as_str());
    assert_eq!(claims.vendor_id, Some(vid.0.to_string()));
    assert_eq!(claims.token_type, TokenType::VendorPortal);
}

#[test]
fn access_token_rejected_by_vendor_portal_validator() {
    let svc = JwtService::new(test_jwt_config());
    let uid = billforge_core::UserId::new();
    let tid = TenantId::new();

    let access = svc
        .create_access_token(&uid, &tid, "test@example.com", &[])
        .expect("create_access_token");

    let err = svc
        .validate_vendor_portal_token(&access)
        .expect_err("should reject access token");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}

#[test]
fn vendor_portal_token_rejected_by_access_validator() {
    let svc = JwtService::new(test_jwt_config());
    let tid = TenantId::new();
    let vid = VendorId(uuid::Uuid::new_v4());

    let vp_token = svc
        .create_vendor_portal_token(&tid, &vid)
        .expect("create_vendor_portal_token");

    let err = svc
        .validate_access_token(&vp_token)
        .expect_err("should reject vendor portal token");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}
