//! Metadata database (PostgreSQL) for tenant registry, auth, and system data

use billforge_core::{Error, Module, Result, Role, TenantId, TenantSettings, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Metadata database for system-wide data
pub struct MetadataDatabase {
    pool: PgPool,
}

impl MetadataDatabase {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| Error::Database(format!("Failed to connect to metadata database: {}", e)))?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
        sqlx::raw_sql(include_str!("../../../migrations/001_create_tenants.sql"))
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Migration(format!("Failed to run metadata migrations: {}", e)))?;

        Ok(())
    }

    /// Check if a tenant exists
    pub async fn tenant_exists(&self, tenant_id: &TenantId) -> Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1)"
        )
        .bind(tenant_id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check tenant: {}", e)))?;

        Ok(exists)
    }

    /// Create a new tenant
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        let slug = slugify(name);
        sqlx::query(
            "INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3)"
        )
        .bind(tenant_id.as_str())
        .bind(name)
        .bind(&slug)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create tenant: {}", e)))?;

        Ok(())
    }

    /// Delete a tenant
    pub async fn delete_tenant(&self, tenant_id: &TenantId) -> Result<()> {
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete tenant: {}", e)))?;

        Ok(())
    }

    /// List all tenant IDs
    pub async fn list_all_tenants(&self) -> Result<Vec<TenantId>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM tenants")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list tenants: {}", e)))?;

        let tenants = rows
            .into_iter()
            .filter_map(|(id,)| id.parse().ok())
            .collect();

        Ok(tenants)
    }

    /// Get tenant settings and enabled modules
    pub async fn get_tenant(&self, tenant_id: &TenantId) -> Result<Option<TenantRecord>> {
        let result = sqlx::query_as::<_, TenantRecord>(
            "SELECT id, name, settings, enabled_modules, is_active FROM tenants WHERE id = $1"
        )
        .bind(tenant_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get tenant: {}", e)))?;

        Ok(result)
    }

    /// Update tenant settings
    pub async fn update_tenant_settings(
        &self,
        tenant_id: &TenantId,
        settings: &TenantSettings,
    ) -> Result<()> {
        let settings_json = serde_json::to_value(settings)
            .map_err(|e| Error::Database(format!("Failed to serialize settings: {}", e)))?;

        sqlx::query(
            "UPDATE tenants SET settings = $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(&settings_json)
        .bind(tenant_id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update tenant settings: {}", e)))?;

        Ok(())
    }

    /// Update tenant enabled modules
    pub async fn update_tenant_modules(
        &self,
        tenant_id: &TenantId,
        modules: &[Module],
    ) -> Result<()> {
        let modules_json = serde_json::to_value(modules)
            .map_err(|e| Error::Database(format!("Failed to serialize modules: {}", e)))?;

        sqlx::query(
            "UPDATE tenants SET enabled_modules = $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(&modules_json)
        .bind(tenant_id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update tenant modules: {}", e)))?;

        Ok(())
    }

    /// Create a new user
    pub async fn create_user(&self, user: &CreateUserInput) -> Result<UserRecord> {
        let id = UserId::new();
        let roles_json = serde_json::to_value(&user.roles)
            .map_err(|e| Error::Database(format!("Failed to serialize roles: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
               VALUES ($1, $2, $3, $4, $5, $6)"#
        )
        .bind(id.0)
        .bind(user.tenant_id.as_str())
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.name)
        .bind(&roles_json)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                Error::AlreadyExists {
                    resource_type: "User".to_string(),
                }
            } else {
                Error::Database(format!("Failed to create user: {}", e))
            }
        })?;

        Ok(UserRecord {
            id: id.0,
            tenant_id: user.tenant_id.to_string(),
            email: user.email.clone(),
            password_hash: user.password_hash.clone(),
            name: user.name.clone(),
            roles: sqlx::types::Json(roles_json),
            is_active: true,
            email_verified: false,
        })
    }

    /// Get user by email within a tenant
    pub async fn get_user_by_email(
        &self,
        tenant_id: &TenantId,
        email: &str,
    ) -> Result<Option<UserRecord>> {
        let result = sqlx::query_as::<_, UserRecord>(
            r#"SELECT id, tenant_id, email, password_hash, name, roles::jsonb, is_active, email_verified
               FROM users WHERE tenant_id = $1 AND email = $2"#
        )
        .bind(tenant_id.as_str())
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get user: {}", e)))?;

        Ok(result)
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<UserRecord>> {
        let result = sqlx::query_as::<_, UserRecord>(
            r#"SELECT id, tenant_id, email, password_hash, name, roles::jsonb, is_active, email_verified
               FROM users WHERE id = $1"#
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get user: {}", e)))?;

        Ok(result)
    }

    /// Update user's last login time
    pub async fn update_last_login(&self, user_id: &UserId) -> Result<()> {
        sqlx::query(
            "UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id = $1"
        )
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update last login: {}", e)))?;

        Ok(())
    }

    /// Store a refresh token
    pub async fn store_refresh_token(
        &self,
        user_id: &UserId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<String> {
        let id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)"
        )
        .bind(id)
        .bind(user_id.0)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to store refresh token: {}", e)))?;

        Ok(id.to_string())
    }

    /// Validate and get user for refresh token
    pub async fn validate_refresh_token(&self, token_hash: &str) -> Result<Option<UserId>> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"SELECT user_id FROM refresh_tokens
               WHERE token_hash = $1
               AND revoked_at IS NULL
               AND expires_at > NOW()"#
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to validate refresh token: {}", e)))?;

        Ok(result.and_then(|(user_id,)| user_id.parse().ok()).map(UserId))
    }

    /// Revoke a refresh token
    pub async fn revoke_refresh_token(&self, token_hash: &str) -> Result<()> {
        sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to revoke refresh token: {}", e)))?;

        Ok(())
    }

    /// Revoke all refresh tokens for a user
    pub async fn revoke_all_user_tokens(&self, user_id: &UserId) -> Result<()> {
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL"
        )
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to revoke user tokens: {}", e)))?;

        Ok(())
    }

    /// Health check - verifies database connectivity
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Health check failed: {}", e)))?;

        Ok(())
    }

    /// Create a new API key
    pub async fn create_api_key(
        &self,
        id: uuid::Uuid,
        tenant_id: &TenantId,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
        roles: &[billforge_core::Role],
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<()> {
        let now = chrono::Utc::now();
        let roles_json = serde_json::to_value(roles)
            .map_err(|e| Error::Database(format!("Failed to serialize roles: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO api_keys (id, tenant_id, name, key_prefix, key_hash, roles, is_active, expires_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(id)
        .bind(tenant_id.as_str())
        .bind(name)
        .bind(key_prefix)
        .bind(key_hash)
        .bind(sqlx::types::Json(roles_json))
        .bind(true)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create API key: {}", e)))?;

        Ok(())
    }

    /// Get API key by prefix
    pub async fn get_api_key_by_prefix(&self, key_prefix: &str) -> Result<Option<ApiKeyRecord>> {
        let result = sqlx::query_as::<_, ApiKeyRecord>(
            "SELECT * FROM api_keys WHERE key_prefix = $1 AND revoked_at IS NULL"
        )
        .bind(key_prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get API key: {}", e)))?;

        Ok(result)
    }

    /// Update API key last used timestamp
    pub async fn update_api_key_last_used(&self, key_id: uuid::Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
            .bind(key_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to update API key: {}", e)))?;

        Ok(())
    }

    /// List API keys for a tenant
    pub async fn list_api_keys(&self, tenant_id: &TenantId) -> Result<Vec<ApiKeyRecord>> {
        let results = sqlx::query_as::<_, ApiKeyRecord>(
            "SELECT * FROM api_keys WHERE tenant_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC"
        )
        .bind(tenant_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list API keys: {}", e)))?;

        Ok(results)
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, tenant_id: &TenantId, key_id: uuid::Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND tenant_id = $2")
            .bind(key_id)
            .bind(tenant_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to revoke API key: {}", e)))?;

        Ok(())
    }
}

/// Convert a string to a URL-safe slug
fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Tenant record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantRecord {
    pub id: String,
    pub name: String,
    pub settings: sqlx::types::Json<serde_json::Value>,
    pub enabled_modules: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
}

/// User record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRecord {
    pub id: sqlx::types::Uuid,
    pub tenant_id: String,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub roles: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
    pub email_verified: bool,
}

/// Input for creating a user
#[derive(Debug, Clone)]
pub struct CreateUserInput {
    pub tenant_id: TenantId,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub roles: Vec<Role>,
}

/// API key record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRecord {
    pub id: sqlx::types::Uuid,
    pub tenant_id: String,
    pub user_id: sqlx::types::Uuid,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub roles: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
