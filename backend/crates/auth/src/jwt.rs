//! JWT token handling

use billforge_core::{Error, Result, Role, TenantId, UserId};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Tenant ID
    pub tenant_id: String,
    /// User email
    pub email: String,
    /// User roles
    pub roles: Vec<String>,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// Token type
    pub token_type: TokenType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

impl Claims {
    pub fn user_id(&self) -> Result<UserId> {
        let uuid = uuid::Uuid::parse_str(&self.sub)
            .map_err(|_| Error::InvalidToken("Invalid user ID in token".to_string()))?;
        Ok(UserId::from_uuid(uuid))
    }

    pub fn tenant_id(&self) -> Result<TenantId> {
        self.tenant_id
            .parse()
            .map_err(|_| Error::InvalidToken("Invalid tenant ID in token".to_string()))
    }

    pub fn roles(&self) -> Vec<Role> {
        self.roles
            .iter()
            .filter_map(|r| match r.as_str() {
                "tenant_admin" => Some(Role::TenantAdmin),
                "ap_user" => Some(Role::ApUser),
                "approver" => Some(Role::Approver),
                "vendor_manager" => Some(Role::VendorManager),
                "report_viewer" => Some(Role::ReportViewer),
                _ => None,
            })
            .collect()
    }
}

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expiry_hours: i64,
    pub refresh_token_expiry_days: i64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: "change-me-in-production".to_string(),
            access_token_expiry_hours: 24,
            refresh_token_expiry_days: 7,
        }
    }
}

/// JWT service for token creation and validation
pub struct JwtService {
    config: JwtConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtService {
    pub fn new(config: JwtConfig) -> Self {
        let encoding_key = EncodingKey::from_secret(config.secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());
        Self {
            config,
            encoding_key,
            decoding_key,
        }
    }

    /// Create an access token
    pub fn create_access_token(
        &self,
        user_id: &UserId,
        tenant_id: &TenantId,
        email: &str,
        roles: &[Role],
    ) -> Result<String> {
        let now = Utc::now();
        let exp = now + Duration::hours(self.config.access_token_expiry_hours);

        let claims = Claims {
            sub: user_id.0.to_string(),
            tenant_id: tenant_id.as_str(),
            email: email.to_string(),
            roles: roles.iter().map(|r| r.as_str().to_string()).collect(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            token_type: TokenType::Access,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| Error::Internal(format!("Failed to create token: {}", e)))
    }

    /// Create a refresh token
    pub fn create_refresh_token(
        &self,
        user_id: &UserId,
        tenant_id: &TenantId,
    ) -> Result<String> {
        let now = Utc::now();
        let exp = now + Duration::days(self.config.refresh_token_expiry_days);

        let claims = Claims {
            sub: user_id.0.to_string(),
            tenant_id: tenant_id.as_str(),
            email: String::new(),
            roles: Vec::new(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            token_type: TokenType::Refresh,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| Error::Internal(format!("Failed to create refresh token: {}", e)))
    }

    /// Validate and decode an access token
    pub fn validate_access_token(&self, token: &str) -> Result<Claims> {
        let validation = Validation::default();
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::TokenExpired,
                _ => Error::InvalidToken(e.to_string()),
            })?;

        if token_data.claims.token_type != TokenType::Access {
            return Err(Error::InvalidToken("Not an access token".to_string()));
        }

        Ok(token_data.claims)
    }

    /// Validate and decode a refresh token
    pub fn validate_refresh_token(&self, token: &str) -> Result<Claims> {
        let validation = Validation::default();
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::TokenExpired,
                _ => Error::InvalidToken(e.to_string()),
            })?;

        if token_data.claims.token_type != TokenType::Refresh {
            return Err(Error::InvalidToken("Not a refresh token".to_string()));
        }

        Ok(token_data.claims)
    }
}
