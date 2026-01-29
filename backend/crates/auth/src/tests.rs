//! Comprehensive tests for billforge-auth

use crate::*;
use billforge_core::{Role, TenantId, UserId};

// ============================================================================
// JWT Service Tests
// ============================================================================

#[cfg(test)]
mod jwt_tests {
    use super::*;

    fn create_test_jwt_service() -> JwtService {
        JwtService::new(JwtConfig {
            secret: "test-secret-key-32-bytes-long-xx".to_string(),
            access_token_expiry_hours: 24,
            refresh_token_expiry_days: 7,
        })
    }

    #[test]
    fn test_create_and_validate_access_token() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();
        let email = "test@example.com";
        let roles = vec![Role::ApUser, Role::Approver];

        let token = service
            .create_access_token(&user_id, &tenant_id, email, &roles)
            .unwrap();

        let claims = service.validate_access_token(&token).unwrap();

        assert_eq!(claims.sub, user_id.0.to_string());
        assert_eq!(claims.tenant_id, tenant_id.as_str());
        assert_eq!(claims.email, email);
        assert_eq!(claims.token_type, jwt::TokenType::Access);
    }

    #[test]
    fn test_create_and_validate_refresh_token() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();

        let token = service
            .create_refresh_token(&user_id, &tenant_id)
            .unwrap();

        let claims = service.validate_refresh_token(&token).unwrap();

        assert_eq!(claims.sub, user_id.0.to_string());
        assert_eq!(claims.tenant_id, tenant_id.as_str());
        assert_eq!(claims.token_type, jwt::TokenType::Refresh);
    }

    #[test]
    fn test_access_token_cannot_be_used_as_refresh() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();

        let access_token = service
            .create_access_token(&user_id, &tenant_id, "test@example.com", &[])
            .unwrap();

        let result = service.validate_refresh_token(&access_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_token_cannot_be_used_as_access() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();

        let refresh_token = service
            .create_refresh_token(&user_id, &tenant_id)
            .unwrap();

        let result = service.validate_access_token(&refresh_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_token_rejected() {
        let service = create_test_jwt_service();
        let result = service.validate_access_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let service1 = create_test_jwt_service();
        let service2 = JwtService::new(JwtConfig {
            secret: "different-secret-key-32-bytes-xx".to_string(),
            access_token_expiry_hours: 24,
            refresh_token_expiry_days: 7,
        });

        let user_id = UserId::new();
        let tenant_id = TenantId::new();
        let token = service1
            .create_access_token(&user_id, &tenant_id, "test@example.com", &[])
            .unwrap();

        let result = service2.validate_access_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_claims_user_id() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();

        let token = service
            .create_access_token(&user_id, &tenant_id, "test@example.com", &[])
            .unwrap();

        let claims = service.validate_access_token(&token).unwrap();
        let extracted_user_id = claims.user_id().unwrap();

        assert_eq!(extracted_user_id.0, user_id.0);
    }

    #[test]
    fn test_claims_tenant_id() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();

        let token = service
            .create_access_token(&user_id, &tenant_id, "test@example.com", &[])
            .unwrap();

        let claims = service.validate_access_token(&token).unwrap();
        let extracted_tenant_id = claims.tenant_id().unwrap();

        assert_eq!(extracted_tenant_id, tenant_id);
    }

    #[test]
    fn test_claims_roles() {
        let service = create_test_jwt_service();
        let user_id = UserId::new();
        let tenant_id = TenantId::new();
        let roles = vec![Role::TenantAdmin, Role::ApUser, Role::Approver];

        let token = service
            .create_access_token(&user_id, &tenant_id, "test@example.com", &roles)
            .unwrap();

        let claims = service.validate_access_token(&token).unwrap();
        let extracted_roles = claims.roles();

        assert!(extracted_roles.contains(&Role::TenantAdmin));
        assert!(extracted_roles.contains(&Role::ApUser));
        assert!(extracted_roles.contains(&Role::Approver));
    }

    #[test]
    fn test_jwt_config_default() {
        let config = JwtConfig::default();
        assert_eq!(config.access_token_expiry_hours, 24);
        assert_eq!(config.refresh_token_expiry_days, 7);
    }
}

// ============================================================================
// Password Service Tests
// ============================================================================

#[cfg(test)]
mod password_tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let service = PasswordService::new();
        let password = "SecurePassword123!";

        let hash = service.hash(password).unwrap();
        assert!(service.verify(password, &hash).unwrap());
    }

    #[test]
    fn test_wrong_password_fails_verification() {
        let service = PasswordService::new();
        let password = "SecurePassword123!";
        let wrong_password = "WrongPassword456!";

        let hash = service.hash(password).unwrap();
        assert!(!service.verify(wrong_password, &hash).unwrap());
    }

    #[test]
    fn test_different_hashes_for_same_password() {
        let service = PasswordService::new();
        let password = "SecurePassword123!";

        let hash1 = service.hash(password).unwrap();
        let hash2 = service.hash(password).unwrap();

        // Due to salt, hashes should be different
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(service.verify(password, &hash1).unwrap());
        assert!(service.verify(password, &hash2).unwrap());
    }

    #[test]
    fn test_password_validation_length() {
        let service = PasswordService::new();

        // Too short
        assert!(service.validate_password_strength("Short1!").is_err());

        // Minimum length (8 chars)
        assert!(service.validate_password_strength("Longenough1").is_ok());
    }

    #[test]
    fn test_password_validation_uppercase() {
        let service = PasswordService::new();

        // No uppercase
        assert!(service.validate_password_strength("nouppercase1").is_err());

        // Has uppercase
        assert!(service.validate_password_strength("HasUppercase1").is_ok());
    }

    #[test]
    fn test_password_validation_lowercase() {
        let service = PasswordService::new();

        // No lowercase
        assert!(service.validate_password_strength("NOLOWERCASE1").is_err());

        // Has lowercase
        assert!(service.validate_password_strength("HASLOWERcase1").is_ok());
    }

    #[test]
    fn test_password_validation_number() {
        let service = PasswordService::new();

        // No number
        assert!(service.validate_password_strength("NoNumbersHere").is_err());

        // Has number
        assert!(service.validate_password_strength("HasNumber1").is_ok());
    }

    #[test]
    fn test_empty_password() {
        let service = PasswordService::new();
        assert!(service.validate_password_strength("").is_err());
    }

    #[test]
    fn test_hash_not_plaintext() {
        let service = PasswordService::new();
        let password = "SecurePassword123!";

        let hash = service.hash(password).unwrap();

        // Hash should not contain the original password
        assert!(!hash.contains(password));
        // Hash should start with argon2 identifier
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_password_service_default() {
        let service = PasswordService::default();
        let hash = service.hash("TestPassword1").unwrap();
        assert!(service.verify("TestPassword1", &hash).unwrap());
    }
}

// ============================================================================
// Token Type Tests
// ============================================================================

#[cfg(test)]
mod token_type_tests {
    use super::jwt::TokenType;

    #[test]
    fn test_token_type_serde() {
        let access = TokenType::Access;
        let json = serde_json::to_string(&access).unwrap();
        assert_eq!(json, "\"access\"");

        let refresh = TokenType::Refresh;
        let json = serde_json::to_string(&refresh).unwrap();
        assert_eq!(json, "\"refresh\"");
    }

    #[test]
    fn test_token_type_deserde() {
        let access: TokenType = serde_json::from_str("\"access\"").unwrap();
        assert_eq!(access, TokenType::Access);

        let refresh: TokenType = serde_json::from_str("\"refresh\"").unwrap();
        assert_eq!(refresh, TokenType::Refresh);
    }

    #[test]
    fn test_token_type_equality() {
        assert_eq!(TokenType::Access, TokenType::Access);
        assert_eq!(TokenType::Refresh, TokenType::Refresh);
        assert_ne!(TokenType::Access, TokenType::Refresh);
    }
}
