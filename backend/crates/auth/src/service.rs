//! Authentication service

use crate::jwt::{JwtConfig, JwtService};
use crate::password::PasswordService;
use billforge_core::{Error, Module, Result, Role, TenantContext, TenantId, UserContext, UserId};
use billforge_db::metadata_db::{CreateUserInput, MetadataDatabase};
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
