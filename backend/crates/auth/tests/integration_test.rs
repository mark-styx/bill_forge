//! Black-box integration tests for billforge-auth public API.
//!
//! Uses only types re-exported from `billforge_auth::*` (no `crate::` imports),
//! exercising the same surface the `api` crate's auth middleware depends on.

use billforge_auth::{Claims, JwtConfig, JwtService, PasswordService, TokenType};
use billforge_core::{Error, Role, TenantId, UserId};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_jwt_config() -> JwtConfig {
    JwtConfig {
        secret: "test-integration-secret-32-bytes!".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    }
}

fn alt_jwt_config() -> JwtConfig {
    JwtConfig {
        secret: "different-integration-secret-32-b!".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 1,
    }
}

// ---------------------------------------------------------------------------
// JWT access-token round-trip
// ---------------------------------------------------------------------------

#[test]
fn access_token_round_trip() {
    let svc = JwtService::new(test_jwt_config());
    let uid = UserId::new();
    let tid = TenantId::new();
    let email = "alice@example.com";
    let roles = vec![Role::ApUser, Role::Approver];

    let token = svc
        .create_access_token(&uid, &tid, email, &roles)
        .expect("create_access_token");
    let claims: Claims = svc.validate_access_token(&token).expect("validate_access_token");

    assert_eq!(claims.sub, uid.to_string());
    assert_eq!(claims.tenant_id, tid.as_str());
    assert_eq!(claims.email, email);
    assert_eq!(claims.token_type, TokenType::Access);
    assert!(claims.roles.contains(&"ap_user".to_string()));
    assert!(claims.roles.contains(&"approver".to_string()));
}

// ---------------------------------------------------------------------------
// JWT refresh-token round-trip
// ---------------------------------------------------------------------------

#[test]
fn refresh_token_round_trip() {
    let svc = JwtService::new(test_jwt_config());
    let uid = UserId::new();
    let tid = TenantId::new();

    let token = svc
        .create_refresh_token(&uid, &tid)
        .expect("create_refresh_token");
    let claims = svc.validate_refresh_token(&token).expect("validate_refresh_token");

    assert_eq!(claims.sub, uid.to_string());
    assert_eq!(claims.tenant_id, tid.as_str());
    assert_eq!(claims.token_type, TokenType::Refresh);
}

// ---------------------------------------------------------------------------
// Cross-type rejection
// ---------------------------------------------------------------------------

#[test]
fn access_validator_rejects_refresh_token() {
    let svc = JwtService::new(test_jwt_config());
    let uid = UserId::new();
    let tid = TenantId::new();

    let refresh = svc
        .create_refresh_token(&uid, &tid)
        .expect("create_refresh_token");

    let err = svc
        .validate_access_token(&refresh)
        .expect_err("access validator should reject refresh token");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}

#[test]
fn refresh_validator_rejects_access_token() {
    let svc = JwtService::new(test_jwt_config());
    let uid = UserId::new();
    let tid = TenantId::new();

    let access = svc
        .create_access_token(&uid, &tid, "bob@example.com", &[])
        .expect("create_access_token");

    let err = svc
        .validate_refresh_token(&access)
        .expect_err("refresh validator should reject access token");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Tampered token rejection
// ---------------------------------------------------------------------------

#[test]
fn tampered_signature_rejected() {
    let svc = JwtService::new(test_jwt_config());
    let uid = UserId::new();
    let tid = TenantId::new();

    let mut token = svc
        .create_access_token(&uid, &tid, "eve@example.com", &[])
        .expect("create_access_token");

    // Flip a byte near the end of the token (the signature segment).
    let last = token.len() - 1;
    token.replace_range(last.., if token.as_bytes()[last] == b'A' { "B" } else { "A" });

    let err = svc
        .validate_access_token(&token)
        .expect_err("tampered token should be rejected");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Wrong-secret rejection
// ---------------------------------------------------------------------------

#[test]
fn wrong_secret_rejected() {
    let svc_a = JwtService::new(test_jwt_config());
    let svc_b = JwtService::new(alt_jwt_config());

    let uid = UserId::new();
    let tid = TenantId::new();

    let token = svc_a
        .create_access_token(&uid, &tid, "carol@example.com", &[])
        .expect("create_access_token");

    let err = svc_b
        .validate_access_token(&token)
        .expect_err("wrong secret should be rejected");

    assert!(
        matches!(err, Error::InvalidToken(_)),
        "expected InvalidToken, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Password hash / verify
// ---------------------------------------------------------------------------

#[test]
fn password_hash_verify_round_trip() {
    let svc = PasswordService::new();
    let password = "SecurePass123";

    let hash = svc.hash(password).expect("hash");
    assert!(
        svc.verify(password, &hash).expect("verify"),
        "correct password should verify"
    );
}

#[test]
fn password_wrong_password_rejected() {
    let svc = PasswordService::new();
    let hash = svc.hash("SecurePass123").expect("hash");

    assert!(
        !svc.verify("WrongPass456", &hash).expect("verify"),
        "wrong password must not verify"
    );
}

#[test]
fn password_strength_valid() {
    let svc = PasswordService::new();
    assert!(svc.validate_password_strength("ValidPass123").is_ok());
}

#[test]
fn password_strength_too_short() {
    let svc = PasswordService::new();
    assert!(svc.validate_password_strength("Sh0rt").is_err());
}

#[test]
fn password_strength_no_digit() {
    let svc = PasswordService::new();
    assert!(svc.validate_password_strength("NoDigitsHere").is_err());
}

// ---------------------------------------------------------------------------
// Combined auth flow (simulates api-middleware minimum surface)
// ---------------------------------------------------------------------------

#[test]
fn combined_password_and_jwt_flow() {
    let jwt = JwtService::new(test_jwt_config());
    let pw = PasswordService::new();

    let uid = UserId::new();
    let tid = TenantId::new();
    let email = "dave@example.com";
    let password = "StrongPass1";
    let roles = vec![Role::TenantAdmin];

    // 1. Password survives hashing
    let hash = pw.hash(password).expect("hash");
    assert!(pw.verify(password, &hash).expect("verify"));

    // 2. Create access token
    let token = jwt
        .create_access_token(&uid, &tid, email, &roles)
        .expect("create_access_token");

    // 3. Decode and assert every claim the middleware would inspect
    let claims = jwt.validate_access_token(&token).expect("validate");

    assert_eq!(claims.user_id().expect("user_id"), uid);
    assert_eq!(claims.tenant_id().expect("tenant_id"), tid);
    assert_eq!(claims.email, email);
    assert_eq!(claims.token_type, TokenType::Access);
    assert!(claims.roles().contains(&Role::TenantAdmin));

    // 4. Password still verifies after the full round-trip
    assert!(pw.verify(password, &hash).expect("verify again"));
}
