//! Metadata database (PostgreSQL) for tenant registry, auth, and system data

use billforge_core::{Error, Module, Result, Role, TenantId, TenantSettings, UserId};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Metadata database for system-wide data
pub struct MetadataDatabase {
    pool: PgPool,
}

impl MetadataDatabase {
    /// Create from an existing connection pool (useful for testing)
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn new(database_url: &str) -> Result<Self> {
        let mut connect_opts = sqlx::postgres::PgConnectOptions::from_str(database_url)
            .map_err(|e| Error::Database(format!("Invalid database URL: {}", e)))?;

        // Pass BILLFORGE_APP_PASSWORD to migration 120 via a session GUC so
        // the role's password stays in sync with compose env vars.
        if let Ok(pw) = std::env::var("BILLFORGE_APP_PASSWORD") {
            if !pw.is_empty() {
                connect_opts = connect_opts.options([("billforge.app_password", pw.as_str())]);
            }
        }

        let pool = PgPool::connect_with(connect_opts).await.map_err(|e| {
            Error::Database(format!("Failed to connect to metadata database: {}", e))
        })?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
        sqlx::raw_sql(include_str!("../../../migrations/001_create_tenants.sql"))
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Migration(format!("Failed to run metadata migrations: {}", e)))?;

        // EDI receiver ID -> tenant mapping for webhook tenant lookup
        sqlx::raw_sql(include_str!(
            "../../../migrations/064_create_edi_receiver_map.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!("Failed to run EDI receiver map migration: {}", e))
        })?;

        sqlx::raw_sql(include_str!(
            "../../../migrations/073_create_tenant_subscriptions.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!(
                "Failed to run tenant subscriptions migration: {}",
                e
            ))
        })?;

        sqlx::raw_sql(include_str!(
            "../../../migrations/090_subscription_module_addons.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!(
                "Failed to run subscription module add-ons migration: {}",
                e
            ))
        })?;

        // Per-tenant forwarding addresses, inbound email log, triage queue
        sqlx::raw_sql(include_str!(
            "../../../migrations/098_create_inbound_email.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!(
                "Failed to run inbound email metadata migration: {}",
                e
            ))
        })?;

        // Create dedicated app role + FORCE RLS on tenant-scoped tables.
        // The FORCE statements are wrapped in existence checks so this is safe
        // to run on the control-plane database (which lacks tenant tables).
        sqlx::raw_sql(include_str!(
            "../../../migrations/120_force_rls_and_app_role.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!(
                "Failed to run FORCE RLS / app-role migration: {}",
                e
            ))
        })?;

        // Add is_sandbox flag to tenants table (self-serve sandbox provisioning).
        sqlx::raw_sql(include_str!(
            "../../../migrations/128_add_tenant_is_sandbox.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to run is_sandbox migration: {}", e)))?;

        // Benchmark peer insights: opt-in columns, KPI rollup table, cohort percentile function.
        sqlx::raw_sql(include_str!(
            "../../../migrations/130_benchmark_peer_insights.up.sql"
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            Error::Migration(format!(
                "Failed to run benchmark peer insights migration: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Check if a tenant exists
    pub async fn tenant_exists(&self, tenant_id: &TenantId) -> Result<bool> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1)")
            .bind(tenant_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to check tenant: {}", e)))?;

        Ok(exists)
    }

    /// Create a new tenant
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        let slug = slugify(name);
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3)")
            .bind(tenant_id.as_uuid())
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
            .bind(tenant_id.as_uuid())
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
            "SELECT id, name, settings, enabled_modules, is_active FROM tenants WHERE id = $1",
        )
        .bind(tenant_id.as_uuid())
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

        sqlx::query("UPDATE tenants SET settings = $1, updated_at = NOW() WHERE id = $2")
            .bind(&settings_json)
            .bind(tenant_id.as_uuid())
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

        sqlx::query("UPDATE tenants SET enabled_modules = $1, updated_at = NOW() WHERE id = $2")
            .bind(&modules_json)
            .bind(tenant_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to update tenant modules: {}", e)))?;

        Ok(())
    }

    /// Set the is_sandbox flag on a tenant.
    pub async fn set_tenant_sandbox(&self, tenant_id: &TenantId, is_sandbox: bool) -> Result<()> {
        sqlx::query("UPDATE tenants SET is_sandbox = $1, updated_at = NOW() WHERE id = $2")
            .bind(is_sandbox)
            .bind(tenant_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to set sandbox flag: {}", e)))?;

        Ok(())
    }

    /// Check whether a tenant is marked as sandbox.
    pub async fn is_tenant_sandbox(&self, tenant_id: &TenantId) -> Result<bool> {
        let is_sandbox: bool = sqlx::query_scalar("SELECT is_sandbox FROM tenants WHERE id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to check sandbox flag: {}", e)))?;

        Ok(is_sandbox)
    }

    /// Update the tenant's subscription plan.
    pub async fn update_tenant_plan(&self, tenant_id: &TenantId, plan_id: &str) -> Result<()> {
        // Use the tenant_subscriptions table if it exists, otherwise update
        // a plan_id column on tenants (or settings). Best-effort: try
        // tenant_subscriptions first, fall back to settings jsonb.
        let result = sqlx::query(
            "INSERT INTO tenant_subscriptions (tenant_id, plan_id, status, created_at, updated_at)
             VALUES ($1, $2, 'active', NOW(), NOW())
             ON CONFLICT (tenant_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, updated_at = NOW()",
        )
        .bind(tenant_id.as_uuid())
        .bind(plan_id)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(_) => {
                // Fallback: store plan_id in settings jsonb
                sqlx::query(
                    "UPDATE tenants SET settings = jsonb_set(COALESCE(settings, '{}'), '{plan_id}', $1::jsonb), updated_at = NOW() WHERE id = $2",
                )
                .bind(serde_json::Value::String(plan_id.to_string()))
                .bind(tenant_id.as_uuid())
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to update tenant plan: {}", e)))?;
                Ok(())
            }
        }
    }

    /// Create a new user
    pub async fn create_user(&self, user: &CreateUserInput) -> Result<UserRecord> {
        let id = UserId::new();
        let roles_json = serde_json::to_value(&user.roles)
            .map_err(|e| Error::Database(format!("Failed to serialize roles: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(id.0)
        .bind(user.tenant_id.as_uuid())
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
            tenant_id: *user.tenant_id.as_uuid(),
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
        .bind(tenant_id.as_uuid())
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get user: {}", e)))?;

        Ok(result)
    }

    /// List active tenant memberships for an email address across tenants.
    pub async fn list_users_by_email(&self, email: &str) -> Result<Vec<UserTenantRecord>> {
        let results = sqlx::query_as::<_, UserTenantRecord>(
            r#"
            SELECT
                u.id,
                u.tenant_id,
                u.email,
                u.password_hash,
                u.name,
                u.roles::jsonb,
                u.is_active,
                u.email_verified,
                t.name AS tenant_name,
                t.is_active AS tenant_is_active
            FROM users u
            JOIN tenants t ON t.id = u.tenant_id
            WHERE lower(u.email) = lower($1)
            ORDER BY t.name ASC
            "#,
        )
        .bind(email)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list users by email: {}", e)))?;

        Ok(results)
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
        sqlx::query("UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id = $1")
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
               AND expires_at > NOW()"#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to validate refresh token: {}", e)))?;

        Ok(result
            .and_then(|(user_id,)| user_id.parse().ok())
            .map(UserId))
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
    #[allow(clippy::too_many_arguments)]
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
        .bind(tenant_id.as_uuid())
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
            "SELECT * FROM api_keys WHERE key_prefix = $1 AND revoked_at IS NULL",
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
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list API keys: {}", e)))?;

        Ok(results)
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, tenant_id: &TenantId, key_id: uuid::Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND tenant_id = $2")
            .bind(key_id)
            .bind(tenant_id.as_uuid())
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
    pub id: sqlx::types::Uuid,
    pub name: String,
    pub settings: sqlx::types::Json<serde_json::Value>,
    pub enabled_modules: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
}

/// User record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRecord {
    pub id: sqlx::types::Uuid,
    pub tenant_id: sqlx::types::Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub roles: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
    pub email_verified: bool,
}

/// User record plus tenant metadata for cross-tenant email discovery.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserTenantRecord {
    pub id: sqlx::types::Uuid,
    pub tenant_id: sqlx::types::Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub roles: sqlx::types::Json<serde_json::Value>,
    pub is_active: bool,
    pub email_verified: bool,
    pub tenant_name: String,
    pub tenant_is_active: bool,
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
    pub tenant_id: sqlx::types::Uuid,
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

/// Implements TenantSettingsProvider by reading from the tenants table.
pub struct TenantSettingsFromDb {
    pool: std::sync::Arc<PgPool>,
}

impl TenantSettingsFromDb {
    pub fn new(pool: std::sync::Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl billforge_core::traits::TenantSettingsProvider for TenantSettingsFromDb {
    async fn get(&self, tenant_id: &TenantId) -> billforge_core::Result<TenantSettings> {
        let row: Option<(sqlx::types::Json<serde_json::Value>,)> =
            sqlx::query_as("SELECT settings FROM tenants WHERE id = $1")
                .bind(tenant_id.as_uuid())
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| {
                    billforge_core::Error::Database(format!(
                        "Failed to fetch tenant settings: {}",
                        e
                    ))
                })?;

        let settings = match row {
            Some((json_val,)) => serde_json::from_value(json_val.0).unwrap_or_default(),
            None => TenantSettings::default(),
        };
        Ok(settings)
    }
}
