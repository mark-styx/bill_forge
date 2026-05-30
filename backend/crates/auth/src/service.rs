//! Authentication service

use crate::jwt::{JwtConfig, JwtService};
use crate::password::PasswordService;
use billforge_core::{Error, Result, Role, TenantContext, TenantId, UserContext, UserId};
use billforge_db::metadata_db::{CreateUserInput, MetadataDatabase, UserRecord};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

    async fn auth_response_for_user(&self, user: UserRecord) -> Result<AuthResponse> {
        if !user.is_active {
            return Err(Error::Forbidden("Account is disabled".to_string()));
        }

        let user_id = UserId(user.id);
        self.metadata_db.update_last_login(&user_id).await?;

        let tenant_id = TenantId::from_uuid(user.tenant_id);
        let tenant = self
            .metadata_db
            .get_tenant(&tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.to_string()))?;

        if !tenant.is_active {
            return Err(Error::Forbidden("Tenant is disabled".to_string()));
        }

        let roles: Vec<Role> = serde_json::from_value(user.roles.clone().0).unwrap_or_default();
        let access_token =
            self.jwt_service
                .create_access_token(&user_id, &tenant_id, &user.email, &roles)?;
        let refresh_token = self
            .jwt_service
            .create_refresh_token(&user_id, &tenant_id)?;
        self.store_refresh_token_hash(&user_id, &refresh_token)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: UserId(user.id),
                tenant_id: TenantId::from_uuid(user.tenant_id),
                email: user.email,
                name: user.name,
                roles: roles.clone(),
            },
            tenant: TenantInfo {
                id: TenantId::from_uuid(tenant.id),
                name: tenant.name,
                enabled_modules: tenant
                    .enabled_modules
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                settings: TenantSettingsInfo {
                    logo_url: tenant
                        .settings
                        .get("logo_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    primary_color: tenant
                        .settings
                        .get("primary_color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    company_name: tenant
                        .settings
                        .get("company_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Company")
                        .to_string(),
                    timezone: tenant
                        .settings
                        .get("timezone")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UTC")
                        .to_string(),
                    default_currency: tenant
                        .settings
                        .get("default_currency")
                        .and_then(|v| v.as_str())
                        .unwrap_or("USD")
                        .to_string(),
                    ocr_provider: tenant
                        .settings
                        .get("ocr_provider")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                },
            },
        })
    }

    /// Hash a token (e.g. refresh token JWT) for database storage using SHA-256
    fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(&hasher.finalize())
    }

    /// Compute the expiry datetime for a new refresh token
    fn refresh_token_expiry(&self) -> chrono::DateTime<Utc> {
        Utc::now() + Duration::days(self.jwt_service.refresh_token_expiry_days())
    }

    /// Store a refresh token hash in the database
    async fn store_refresh_token_hash(&self, user_id: &UserId, refresh_token: &str) -> Result<()> {
        let hash = Self::hash_token(refresh_token);
        let expires_at = self.refresh_token_expiry();
        self.metadata_db
            .store_refresh_token(user_id, &hash, expires_at)
            .await?;
        Ok(())
    }

    /// Register a new user
    pub async fn register(&self, input: RegisterInput) -> Result<AuthResponse> {
        // Validate password strength
        self.password_service
            .validate_password_strength(&input.password)?;

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
        let tenant_id: TenantId = TenantId::from_uuid(user.tenant_id);
        let tenant = self
            .metadata_db
            .get_tenant(&tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.to_string()))?;

        // Convert roles from JSON
        let roles: Vec<Role> = serde_json::from_value(user.roles.clone().0).unwrap_or_default();

        // Generate tokens
        let user_id = UserId(user.id);
        let access_token =
            self.jwt_service
                .create_access_token(&user_id, &tenant_id, &user.email, &roles)?;
        let refresh_token = self
            .jwt_service
            .create_refresh_token(&user_id, &tenant_id)?;
        self.store_refresh_token_hash(&user_id, &refresh_token)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: UserId(user.id),
                tenant_id: TenantId::from_uuid(user.tenant_id),
                email: user.email,
                name: user.name,
                roles: roles.clone(),
            },
            tenant: TenantInfo {
                id: TenantId::from_uuid(tenant.id),
                name: tenant.name,
                enabled_modules: tenant
                    .enabled_modules
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                settings: TenantSettingsInfo {
                    logo_url: tenant
                        .settings
                        .get("logo_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    primary_color: tenant
                        .settings
                        .get("primary_color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    company_name: tenant
                        .settings
                        .get("company_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Company")
                        .to_string(),
                    timezone: tenant
                        .settings
                        .get("timezone")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UTC")
                        .to_string(),
                    default_currency: tenant
                        .settings
                        .get("default_currency")
                        .and_then(|v| v.as_str())
                        .unwrap_or("USD")
                        .to_string(),
                    ocr_provider: tenant
                        .settings
                        .get("ocr_provider")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                },
            },
        })
    }

    /// Provision a new tenant with an admin user (self-service signup)
    pub async fn provision(&self, input: ProvisionInput) -> Result<AuthResponse> {
        // Validate password strength
        self.password_service
            .validate_password_strength(&input.admin_password)?;

        // Create a new tenant
        let tenant_id = TenantId::new();
        self.metadata_db
            .create_tenant(&tenant_id, &input.company_name)
            .await?;

        // Set tenant settings
        let settings = billforge_core::TenantSettings {
            logo_url: None,
            primary_color: None,
            company_name: input.company_name.clone(),
            timezone: input.timezone.unwrap_or_else(|| "UTC".to_string()),
            default_currency: input.default_currency.unwrap_or_else(|| "USD".to_string()),
            ocr_provider: input.ocr_provider.clone(),
            features: billforge_core::TenantFeatures {
                advanced_ocr: matches!(
                    input.ocr_provider.as_deref(),
                    Some("aws_textract" | "textract" | "google_vision" | "google")
                ),
                local_ocr_required: input.local_ocr_required.unwrap_or(false),
                ..Default::default()
            },
        };
        self.metadata_db
            .update_tenant_settings(&tenant_id, &settings)
            .await?;

        // Enable default modules
        let default_modules = vec![
            billforge_core::Module::InvoiceCapture,
            billforge_core::Module::InvoiceProcessing,
            billforge_core::Module::VendorManagement,
            billforge_core::Module::Reporting,
        ];
        self.metadata_db
            .update_tenant_modules(&tenant_id, &default_modules)
            .await?;

        // Create admin user with all roles
        let password_hash = self.password_service.hash(&input.admin_password)?;
        let admin_roles = vec![
            Role::TenantAdmin,
            Role::ApUser,
            Role::Approver,
            Role::VendorManager,
        ];
        let user = self
            .metadata_db
            .create_user(&CreateUserInput {
                tenant_id: tenant_id.clone(),
                email: input.admin_email.clone(),
                password_hash,
                name: input.admin_name.clone(),
                roles: admin_roles.clone(),
            })
            .await?;

        // Get the freshly created tenant
        let tenant = self
            .metadata_db
            .get_tenant(&tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(tenant_id.as_str()))?;

        // Generate tokens
        let user_id = UserId(user.id);
        let access_token = self.jwt_service.create_access_token(
            &user_id,
            &tenant_id,
            &user.email,
            &admin_roles,
        )?;
        let refresh_token = self
            .jwt_service
            .create_refresh_token(&user_id, &tenant_id)?;
        self.store_refresh_token_hash(&user_id, &refresh_token)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserInfo {
                id: UserId(user.id),
                tenant_id: TenantId::from_uuid(user.tenant_id),
                email: user.email,
                name: user.name,
                roles: admin_roles,
            },
            tenant: TenantInfo {
                id: TenantId::from_uuid(tenant.id),
                name: tenant.name,
                enabled_modules: tenant
                    .enabled_modules
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                settings: TenantSettingsInfo {
                    logo_url: None,
                    primary_color: None,
                    company_name: input.company_name,
                    timezone: settings.timezone,
                    default_currency: settings.default_currency,
                    ocr_provider: settings.ocr_provider,
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

        self.auth_response_for_user(user).await
    }

    /// Mobile login with tenant discovery by email.
    pub async fn mobile_login(&self, input: MobileLoginInput) -> Result<MobileLoginResult> {
        let users = self.metadata_db.list_users_by_email(&input.email).await?;
        let mut verified = Vec::new();

        for user in users {
            if !user.is_active || !user.tenant_is_active {
                continue;
            }

            if self
                .password_service
                .verify(&input.password, &user.password_hash)?
            {
                verified.push(user);
            }
        }

        if verified.is_empty() {
            return Err(Error::InvalidCredentials);
        }

        if verified.len() == 1 {
            let user = verified.remove(0);
            let response = self
                .auth_response_for_user(UserRecord {
                    id: user.id,
                    tenant_id: user.tenant_id,
                    email: user.email,
                    password_hash: user.password_hash,
                    name: user.name,
                    roles: user.roles,
                    is_active: user.is_active,
                    email_verified: user.email_verified,
                })
                .await?;
            return Ok(MobileLoginResult::LoggedIn(Box::new(response)));
        }

        let picker_user = UserId(verified[0].id);
        let picker_tenant = TenantId::from_uuid(uuid::Uuid::nil());
        let picker_token = self.jwt_service.create_access_token(
            &picker_user,
            &picker_tenant,
            &input.email,
            &[],
        )?;
        let tenants = verified
            .into_iter()
            .map(|user| {
                let roles: Vec<Role> =
                    serde_json::from_value(user.roles.clone().0).unwrap_or_default();
                MobileTenantOption {
                    tenant_id: TenantId::from_uuid(user.tenant_id),
                    tenant_name: user.tenant_name,
                    role: mobile_role_label(&roles),
                }
            })
            .collect();

        Ok(MobileLoginResult::TenantPicker {
            jwt: picker_token,
            email: input.email,
            tenants,
        })
    }

    /// Complete a mobile multi-tenant login after the user chooses a tenant.
    pub async fn mobile_login_with_tenant(
        &self,
        picker_token: &str,
        tenant_id: TenantId,
    ) -> Result<AuthResponse> {
        let context = self.validate_token(picker_token).await?;
        if !context.tenant_id.as_uuid().is_nil() {
            return Err(Error::InvalidToken(
                "Expected a mobile tenant picker token".to_string(),
            ));
        }

        let user = self
            .metadata_db
            .get_user_by_email(&tenant_id, &context.email)
            .await?
            .ok_or(Error::InvalidCredentials)?;

        self.auth_response_for_user(user).await
    }

    /// Refresh access token using refresh token
    pub async fn refresh(&self, refresh_token: &str) -> Result<AuthResponse> {
        // Validate refresh token (cryptographic check)
        let claims = self.jwt_service.validate_refresh_token(refresh_token)?;
        let user_id = claims.user_id()?;

        // Check revocation status in DB
        let old_hash = Self::hash_token(refresh_token);
        let stored_user_id = self
            .metadata_db
            .validate_refresh_token(&old_hash)
            .await?
            .ok_or_else(|| Error::InvalidToken("Refresh token has been revoked".to_string()))?;

        // Verify the token belongs to the claimed user
        if stored_user_id != user_id {
            return Err(Error::InvalidToken(
                "Refresh token user mismatch".to_string(),
            ));
        }

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
        let tenant_id: TenantId = TenantId::from_uuid(user.tenant_id);
        let tenant = self
            .metadata_db
            .get_tenant(&tenant_id)
            .await?
            .ok_or_else(|| Error::TenantNotFound(user.tenant_id.to_string()))?;

        // Convert roles from JSON
        let roles: Vec<Role> = serde_json::from_value(user.roles.clone().0).unwrap_or_default();

        // Generate new tokens
        let user_id = UserId(user.id);
        let access_token =
            self.jwt_service
                .create_access_token(&user_id, &tenant_id, &user.email, &roles)?;
        let new_refresh_token = self
            .jwt_service
            .create_refresh_token(&user_id, &tenant_id)?;

        // Revoke old refresh token (token rotation)
        self.metadata_db.revoke_refresh_token(&old_hash).await?;

        // Store new refresh token hash
        self.store_refresh_token_hash(&user_id, &new_refresh_token)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token: new_refresh_token,
            user: UserInfo {
                id: UserId(user.id),
                tenant_id: TenantId::from_uuid(user.tenant_id),
                email: user.email,
                name: user.name,
                roles: roles.clone(),
            },
            tenant: TenantInfo {
                id: TenantId::from_uuid(tenant.id),
                name: tenant.name,
                enabled_modules: tenant
                    .enabled_modules
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                settings: TenantSettingsInfo {
                    logo_url: tenant
                        .settings
                        .get("logo_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    primary_color: tenant
                        .settings
                        .get("primary_color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    company_name: tenant
                        .settings
                        .get("company_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Company")
                        .to_string(),
                    timezone: tenant
                        .settings
                        .get("timezone")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UTC")
                        .to_string(),
                    default_currency: tenant
                        .settings
                        .get("default_currency")
                        .and_then(|v| v.as_str())
                        .unwrap_or("USD")
                        .to_string(),
                    ocr_provider: tenant
                        .settings
                        .get("ocr_provider")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                },
            },
        })
    }

    /// Access the underlying JwtService (used by vendor-portal token validation)
    pub fn jwt_service(&self) -> &JwtService {
        &self.jwt_service
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
            tenant_id: TenantId::from_uuid(tenant.id),
            tenant_name: tenant.name,
            enabled_modules: serde_json::from_value(tenant.enabled_modules.0.clone())
                .unwrap_or_default(),
            settings: serde_json::from_value(tenant.settings.0.clone()).unwrap_or_default(),
        })
    }

    /// Logout (revoke refresh token)
    pub async fn logout(&self, user_id: &UserId) -> Result<()> {
        self.metadata_db.revoke_all_user_tokens(user_id).await
    }
}

/// Self-service tenant provisioning input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionInput {
    pub company_name: String,
    pub admin_email: String,
    pub admin_password: String,
    pub admin_name: String,
    pub timezone: Option<String>,
    pub default_currency: Option<String>,
    pub ocr_provider: Option<String>,
    pub local_ocr_required: Option<bool>,
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

/// Mobile login input for tenant discovery by email.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileLoginInput {
    pub email: String,
    pub password: String,
}

/// Tenant choice returned to mobile users with multiple memberships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileTenantOption {
    pub tenant_id: TenantId,
    pub tenant_name: String,
    pub role: String,
}

/// Mobile login can either produce a full auth response or a tenant picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MobileLoginResult {
    LoggedIn(Box<AuthResponse>),
    TenantPicker {
        jwt: String,
        email: String,
        tenants: Vec<MobileTenantOption>,
    },
}

fn mobile_role_label(roles: &[Role]) -> String {
    if roles.contains(&Role::Approver) {
        return Role::Approver.as_str().to_string();
    }

    roles
        .first()
        .map(|role| role.as_str().to_string())
        .unwrap_or_else(|| "user".to_string())
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
    pub ocr_provider: Option<String>,
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
        let expires_at = input
            .expires_in_days
            .map(|days| now + chrono::Duration::days(days as i64));

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
        if !self
            .password_service
            .verify(api_key, &stored_key.key_hash)?
        {
            return Err(Error::InvalidToken("Invalid API key".to_string()));
        }

        // Update last used timestamp
        self.metadata_db
            .update_api_key_last_used(stored_key.id)
            .await?;

        // Return a synthetic user context for API key access
        let roles: Vec<Role> =
            serde_json::from_value(stored_key.roles.0.clone()).unwrap_or_default();
        Ok(UserContext {
            user_id: UserId::from_uuid(stored_key.id), // Use key ID as user ID
            tenant_id: TenantId::from_uuid(stored_key.tenant_id),
            email: format!("api-key:{}", stored_key.name),
            name: format!("API Key: {}", stored_key.name),
            roles,
        })
    }

    /// List API keys for a tenant
    pub async fn list_api_keys(&self, tenant_id: &TenantId) -> Result<Vec<ApiKey>> {
        let records = self.metadata_db.list_api_keys(tenant_id).await?;
        Ok(records
            .into_iter()
            .map(|r| {
                let roles: Vec<Role> =
                    serde_json::from_value(r.roles.0.clone()).unwrap_or_default();
                ApiKey {
                    id: r.id,
                    tenant_id: TenantId::from_uuid(r.tenant_id),
                    name: r.name,
                    key_prefix: r.key_prefix,
                    key_hash: r.key_hash,
                    roles,
                    is_active: r.is_active,
                    last_used_at: r.last_used_at,
                    expires_at: r.expires_at,
                    created_at: r.created_at,
                }
            })
            .collect())
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, tenant_id: &TenantId, key_id: uuid::Uuid) -> Result<()> {
        self.metadata_db.revoke_api_key(tenant_id, key_id).await
    }
}

// Helper for hex encoding (would normally use the `hex` crate)
pub(crate) mod hex {
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
