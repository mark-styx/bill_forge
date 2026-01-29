//! Authentication service

use crate::jwt::{JwtConfig, JwtService};
use crate::password::PasswordService;
use billforge_core::{Error, Module, Result, Role, TenantContext, TenantId, UserContext, UserId};
use billforge_db::metadata_db::{CreateUserInput, MetadataDatabase, ApiKeyRecord};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Authentication service
pub struct AuthService {
    jwt_service: JwtService,
    password_service: PasswordService,
    metadata_db: Arc<MetadataDatabase>,
}

impl AuthService {
    pub fn new(jwt_config: JwtConfig, metadata_db: Arc<MetadataDatabase>) -> Self {
        Self {
            jwt_service: JwtService::new(jwt_config),
            password_service: PasswordService::new(),
            metadata_db,
        }
    }

    /// Register a new user
    pub async fn register(
        &self,
        input: RegisterInput,
    ) -> Result<AuthResponse> {
        // Validate password strength
        self.password_service.validate_password_strength(&input.password)?;

        // Check if user already exists
        if self
            .metadata_db
            .get_user_by_email(&input.tenant_id, &input.email)
            .await?
            .is_some()
        {
            return Err(Error::AlreadyExists {
                resource_type: "User".to_string(),
            });
        }

        // Hash password
        let password_hash = self.password_service.hash(&input.password)?;

        // Create user
        let user = self
            .metadata_db
            .create_user(&CreateUserInput {
                tenant_id: input.tenant_id.clone(),
                email: input.email.clone(),
                password_hash,
                name: input.name.clone(),
                roles: input.roles.clone(),
            })
            .await?;

        // Get tenant info
        let tenant = self
            .metadata_db
            .get_tenant(&user.tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.as_str()))?;

        // Generate tokens
        let access_token = self.jwt_service.create_access_token(
            &user.id,
            &user.tenant_id,
            &user.email,
            &user.roles,
        )?;
        let refresh_token = self
            .jwt_service
            .create_refresh_token(&user.id, &user.tenant_id)?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: user.id,
                tenant_id: user.tenant_id,
                email: user.email,
                name: user.name,
                roles: user.roles,
            },
            tenant: TenantInfo {
                id: tenant.id,
                name: tenant.name,
                enabled_modules: tenant.enabled_modules.iter().map(|m| m.as_str().to_string()).collect(),
                settings: TenantSettingsInfo {
                    logo_url: tenant.settings.logo_url,
                    primary_color: tenant.settings.primary_color,
                    company_name: tenant.settings.company_name,
                    timezone: tenant.settings.timezone,
                    default_currency: tenant.settings.default_currency,
                },
            },
        })
    }

    /// Login with email and password
    pub async fn login(&self, input: LoginInput) -> Result<AuthResponse> {
        // Find user
        let user = self
            .metadata_db
            .get_user_by_email(&input.tenant_id, &input.email)
            .await?
            .ok_or(Error::InvalidCredentials)?;

        // Verify password
        if !self
            .password_service
            .verify(&input.password, &user.password_hash)?
        {
            return Err(Error::InvalidCredentials);
        }

        // Check if user is active
        if !user.is_active {
            return Err(Error::Forbidden("Account is disabled".to_string()));
        }

        // Update last login
        self.metadata_db.update_last_login(&user.id).await?;

        // Get tenant info
        let tenant = self
            .metadata_db
            .get_tenant(&user.tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.as_str()))?;

        // Generate tokens
        let access_token = self.jwt_service.create_access_token(
            &user.id,
            &user.tenant_id,
            &user.email,
            &user.roles,
        )?;
        let refresh_token = self
            .jwt_service
            .create_refresh_token(&user.id, &user.tenant_id)?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: user.id,
                tenant_id: user.tenant_id,
                email: user.email,
                name: user.name,
                roles: user.roles,
            },
            tenant: TenantInfo {
                id: tenant.id,
                name: tenant.name,
                enabled_modules: tenant.enabled_modules.iter().map(|m| m.as_str().to_string()).collect(),
                settings: TenantSettingsInfo {
                    logo_url: tenant.settings.logo_url,
                    primary_color: tenant.settings.primary_color,
                    company_name: tenant.settings.company_name,
                    timezone: tenant.settings.timezone,
                    default_currency: tenant.settings.default_currency,
                },
            },
        })
    }

    /// Refresh access token using refresh token
    pub async fn refresh(&self, refresh_token: &str) -> Result<AuthResponse> {
        // Validate refresh token
        let claims = self.jwt_service.validate_refresh_token(refresh_token)?;
        let user_id = claims.user_id()?;

        // Get user
        let user = self
            .metadata_db
            .get_user_by_id(&user_id)
            .await?
            .ok_or(Error::InvalidToken("User not found".to_string()))?;

        // Check if user is active
        if !user.is_active {
            return Err(Error::Forbidden("Account is disabled".to_string()));
        }

        // Get tenant info
        let tenant = self
            .metadata_db
            .get_tenant(&user.tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.as_str()))?;

        // Generate new tokens
        let access_token = self.jwt_service.create_access_token(
            &user.id,
            &user.tenant_id,
            &user.email,
            &user.roles,
        )?;
        let new_refresh_token = self
            .jwt_service
            .create_refresh_token(&user.id, &user.tenant_id)?;

        Ok(AuthResponse {
            access_token,
            refresh_token: new_refresh_token,
            user: UserInfo {
                id: user.id,
                tenant_id: user.tenant_id,
                email: user.email,
                name: user.name,
                roles: user.roles,
            },
            tenant: TenantInfo {
                id: tenant.id,
                name: tenant.name,
                enabled_modules: tenant.enabled_modules.iter().map(|m| m.as_str().to_string()).collect(),
                settings: TenantSettingsInfo {
                    logo_url: tenant.settings.logo_url,
                    primary_color: tenant.settings.primary_color,
                    company_name: tenant.settings.company_name,
                    timezone: tenant.settings.timezone,
                    default_currency: tenant.settings.default_currency,
                },
            },
        })
    }

    /// Validate access token and return user context
    pub async fn validate_token(&self, token: &str) -> Result<UserContext> {
        let claims = self.jwt_service.validate_access_token(token)?;
        let roles = claims.roles();

        Ok(UserContext {
            user_id: claims.user_id()?,
            tenant_id: claims.tenant_id()?,
            email: claims.email,
            name: String::new(), // Would need to fetch from DB if needed
            roles,
        })
    }

    /// Get tenant context for a tenant ID
    pub async fn get_tenant_context(&self, tenant_id: &TenantId) -> Result<TenantContext> {
        let tenant = self
            .metadata_db
            .get_tenant(tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(tenant_id.as_str()))?;

        Ok(TenantContext {
            tenant_id: tenant.id,
            tenant_name: tenant.name,
            enabled_modules: tenant.enabled_modules,
            settings: tenant.settings,
        })
    }

    /// Logout (revoke refresh token)
    pub async fn logout(&self, user_id: &UserId) -> Result<()> {
        self.metadata_db.revoke_all_user_tokens(user_id).await
    }
}

/// Registration input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterInput {
    pub tenant_id: TenantId,
    pub email: String,
    pub password: String,
    pub name: String,
    pub roles: Vec<Role>,
}

/// Login input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginInput {
    pub tenant_id: TenantId,
    pub email: String,
    pub password: String,
}

/// Authentication response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserInfo,
    pub tenant: TenantInfo,
}

/// User information in auth response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub name: String,
    pub roles: Vec<Role>,
}

/// Tenant information in auth response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: TenantId,
    pub name: String,
    pub enabled_modules: Vec<String>,
    pub settings: TenantSettingsInfo,
}

/// Tenant settings in auth response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSettingsInfo {
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub company_name: String,
    pub timezone: String,
    pub default_currency: String,
}

// =============================================================================
// API Key Authentication
// =============================================================================

/// API key for programmatic access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: uuid::Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub roles: Vec<Role>,
    pub is_active: bool,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Input for creating an API key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyInput {
    pub name: String,
    pub roles: Vec<Role>,
    pub expires_in_days: Option<u32>,
}

/// Response when creating an API key (contains the actual key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: uuid::Uuid,
    pub name: String,
    /// The actual API key - only returned once on creation
    pub key: String,
    /// Prefix for identification
    pub key_prefix: String,
    pub roles: Vec<Role>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AuthService {
    /// Create a new API key for a tenant
    pub async fn create_api_key(
        &self,
        tenant_id: &TenantId,
        input: CreateApiKeyInput,
    ) -> Result<CreateApiKeyResponse> {
        use rand::Rng;

        // Generate a secure random API key
        let key_bytes: [u8; 32] = rand::thread_rng().gen();
        let key = format!("bf_{}", hex::encode(&key_bytes));
        let key_prefix = key[..12].to_string(); // "bf_" + first 8 hex chars

        // Hash the key for storage
        let key_hash = self.password_service.hash(&key)?;

        let id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();
        let expires_at = input.expires_in_days.map(|days| {
            now + chrono::Duration::days(days as i64)
        });

        // Store the API key
        self.metadata_db
            .create_api_key(
                id,
                tenant_id,
                &input.name,
                &key_prefix,
                &key_hash,
                &input.roles,
                expires_at,
            )
            .await?;

        Ok(CreateApiKeyResponse {
            id,
            name: input.name,
            key,
            key_prefix,
            roles: input.roles,
            expires_at,
        })
    }

    /// Validate an API key and return user context
    pub async fn validate_api_key(&self, api_key: &str) -> Result<UserContext> {
        // API keys start with "bf_"
        if !api_key.starts_with("bf_") {
            return Err(Error::InvalidToken("Invalid API key format".to_string()));
        }

        let key_prefix = api_key[..12].to_string();

        // Find API key by prefix
        let stored_key = self
            .metadata_db
            .get_api_key_by_prefix(&key_prefix)
            .await?
            .ok_or_else(|| Error::InvalidToken("API key not found".to_string()))?;

        // Verify the key hasn't expired
        if let Some(expires_at) = stored_key.expires_at {
            if expires_at < chrono::Utc::now() {
                return Err(Error::TokenExpired);
            }
        }

        // Verify the key is active
        if !stored_key.is_active {
            return Err(Error::Forbidden("API key is disabled".to_string()));
        }

        // Verify the key hash matches
        if !self.password_service.verify(api_key, &stored_key.key_hash)? {
            return Err(Error::InvalidToken("Invalid API key".to_string()));
        }

        // Update last used timestamp
        self.metadata_db
            .update_api_key_last_used(stored_key.id)
            .await?;

        // Return a synthetic user context for API key access
        Ok(UserContext {
            user_id: UserId::from_uuid(stored_key.id), // Use key ID as user ID
            tenant_id: stored_key.tenant_id,
            email: format!("api-key:{}", stored_key.name),
            name: format!("API Key: {}", stored_key.name),
            roles: stored_key.roles,
        })
    }

    /// List API keys for a tenant
    pub async fn list_api_keys(&self, tenant_id: &TenantId) -> Result<Vec<ApiKey>> {
        let records = self.metadata_db.list_api_keys(tenant_id).await?;
        Ok(records
            .into_iter()
            .map(|r| ApiKey {
                id: r.id,
                tenant_id: r.tenant_id,
                name: r.name,
                key_prefix: r.key_prefix,
                key_hash: r.key_hash,
                roles: r.roles,
                is_active: r.is_active,
                last_used_at: r.last_used_at,
                expires_at: r.expires_at,
                created_at: r.created_at,
            })
            .collect())
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, tenant_id: &TenantId, key_id: uuid::Uuid) -> Result<()> {
        self.metadata_db.revoke_api_key(tenant_id, key_id).await
    }
}

// Helper for hex encoding (would normally use the `hex` crate)
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut result = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            result.push(HEX_CHARS[(byte >> 4) as usize] as char);
            result.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
        }
        result
    }
}
