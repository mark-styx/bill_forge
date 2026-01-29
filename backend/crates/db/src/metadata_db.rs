//! Metadata database (SQLite) for tenant registry, auth, and system data

use billforge_core::{Error, Module, Result, Role, TenantId, TenantSettings, UserId};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Metadata database for system-wide data
pub struct MetadataDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl MetadataDatabase {
    pub async fn new(db_url: &str) -> Result<Self> {
        // Parse sqlite:// URL
        let path = db_url.strip_prefix("sqlite://").unwrap_or(db_url);
        
        // Create parent directory if needed
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Database(format!("Failed to create db directory: {}", e)))?;
        }

        let conn = Connection::open(path)
            .map_err(|e| Error::Database(format!("Failed to open metadata db: {}", e)))?;
        
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        
        db.run_migrations().await?;
        
        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        
        conn.execute_batch(
            r#"
            -- Tenants table
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                settings TEXT NOT NULL DEFAULT '{}',
                enabled_modules TEXT NOT NULL DEFAULT '[]',
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Users table
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                email TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                name TEXT NOT NULL,
                roles TEXT NOT NULL DEFAULT '[]',
                is_active INTEGER NOT NULL DEFAULT 1,
                email_verified INTEGER NOT NULL DEFAULT 0,
                last_login_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
                UNIQUE(tenant_id, email)
            );

            -- Refresh tokens table
            CREATE TABLE IF NOT EXISTS refresh_tokens (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                token_hash TEXT NOT NULL UNIQUE,
                expires_at TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                revoked_at TEXT,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );

            -- API keys table
            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                scopes TEXT NOT NULL DEFAULT '[]',
                last_used_at TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                revoked_at TEXT,
                FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
            );

            -- System audit log (for admin actions)
            CREATE TABLE IF NOT EXISTS system_audit_log (
                id TEXT PRIMARY KEY,
                actor_type TEXT NOT NULL,
                actor_id TEXT,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT,
                details TEXT,
                ip_address TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_users_tenant_email ON users(tenant_id, email);
            CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
            CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id);
            "#,
        )
        .map_err(|e| Error::Migration(format!("Failed to run metadata migrations: {}", e)))?;

        Ok(())
    }

    /// Check if a tenant exists
    pub async fn tenant_exists(&self, tenant_id: &TenantId) -> Result<bool> {
        let conn = self.conn.lock().await;
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = ?)",
                params![tenant_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("Failed to check tenant: {}", e)))?;
        Ok(exists)
    }

    /// Create a new tenant
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO tenants (id, name) VALUES (?, ?)",
            params![tenant_id.as_str(), name],
        )
        .map_err(|e| Error::Database(format!("Failed to create tenant: {}", e)))?;
        Ok(())
    }

    /// Delete a tenant
    pub async fn delete_tenant(&self, tenant_id: &TenantId) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM tenants WHERE id = ?",
            params![tenant_id.as_str()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete tenant: {}", e)))?;
        Ok(())
    }

    /// List all tenant IDs
    pub async fn list_all_tenants(&self) -> Result<Vec<TenantId>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT id FROM tenants WHERE is_active = 1")
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;
        
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(id.parse::<TenantId>().unwrap())
            })
            .map_err(|e| Error::Database(format!("Failed to list tenants: {}", e)))?;
        
        let mut tenants = Vec::new();
        for row in rows {
            tenants.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }
        
        Ok(tenants)
    }

    /// Get tenant settings and enabled modules
    pub async fn get_tenant(&self, tenant_id: &TenantId) -> Result<Option<TenantRecord>> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                "SELECT id, name, settings, enabled_modules, is_active FROM tenants WHERE id = ?",
                params![tenant_id.as_str()],
                |row| {
                    let settings_json: String = row.get(2)?;
                    let modules_json: String = row.get(3)?;
                    Ok(TenantRecord {
                        id: row.get::<_, String>(0)?.parse().unwrap(),
                        name: row.get(1)?,
                        settings: serde_json::from_str(&settings_json).unwrap_or_default(),
                        enabled_modules: serde_json::from_str(&modules_json).unwrap_or_default(),
                        is_active: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("Failed to get tenant: {}", e)))?;
        
        Ok(result)
    }

    /// Update tenant settings
    pub async fn update_tenant_settings(
        &self,
        tenant_id: &TenantId,
        settings: &TenantSettings,
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        let settings_json = serde_json::to_string(settings)
            .map_err(|e| Error::Database(format!("Failed to serialize settings: {}", e)))?;
        
        conn.execute(
            "UPDATE tenants SET settings = ?, updated_at = datetime('now') WHERE id = ?",
            params![settings_json, tenant_id.as_str()],
        )
        .map_err(|e| Error::Database(format!("Failed to update tenant settings: {}", e)))?;
        
        Ok(())
    }

    /// Update tenant enabled modules
    pub async fn update_tenant_modules(
        &self,
        tenant_id: &TenantId,
        modules: &[Module],
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        let modules_json = serde_json::to_string(modules)
            .map_err(|e| Error::Database(format!("Failed to serialize modules: {}", e)))?;
        
        conn.execute(
            "UPDATE tenants SET enabled_modules = ?, updated_at = datetime('now') WHERE id = ?",
            params![modules_json, tenant_id.as_str()],
        )
        .map_err(|e| Error::Database(format!("Failed to update tenant modules: {}", e)))?;
        
        Ok(())
    }

    /// Create a new user
    pub async fn create_user(&self, user: &CreateUserInput) -> Result<UserRecord> {
        let conn = self.conn.lock().await;
        let id = UserId::new();
        let roles_json = serde_json::to_string(&user.roles)
            .map_err(|e| Error::Database(format!("Failed to serialize roles: {}", e)))?;
        
        conn.execute(
            r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            params![
                id.0.to_string(),
                user.tenant_id.as_str(),
                user.email,
                user.password_hash,
                user.name,
                roles_json,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                Error::AlreadyExists {
                    resource_type: "User".to_string(),
                }
            } else {
                Error::Database(format!("Failed to create user: {}", e))
            }
        })?;
        
        Ok(UserRecord {
            id,
            tenant_id: user.tenant_id.clone(),
            email: user.email.clone(),
            password_hash: user.password_hash.clone(),
            name: user.name.clone(),
            roles: user.roles.clone(),
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
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                r#"SELECT id, tenant_id, email, password_hash, name, roles, is_active, email_verified
                   FROM users WHERE tenant_id = ? AND email = ?"#,
                params![tenant_id.as_str(), email],
                |row| {
                    let roles_json: String = row.get(5)?;
                    Ok(UserRecord {
                        id: UserId::from_uuid(Uuid::parse_str(&row.get::<_, String>(0)?).unwrap()),
                        tenant_id: row.get::<_, String>(1)?.parse().unwrap(),
                        email: row.get(2)?,
                        password_hash: row.get(3)?,
                        name: row.get(4)?,
                        roles: serde_json::from_str(&roles_json).unwrap_or_default(),
                        is_active: row.get(6)?,
                        email_verified: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("Failed to get user: {}", e)))?;
        
        Ok(result)
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<UserRecord>> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                r#"SELECT id, tenant_id, email, password_hash, name, roles, is_active, email_verified
                   FROM users WHERE id = ?"#,
                params![user_id.0.to_string()],
                |row| {
                    let roles_json: String = row.get(5)?;
                    Ok(UserRecord {
                        id: UserId::from_uuid(Uuid::parse_str(&row.get::<_, String>(0)?).unwrap()),
                        tenant_id: row.get::<_, String>(1)?.parse().unwrap(),
                        email: row.get(2)?,
                        password_hash: row.get(3)?,
                        name: row.get(4)?,
                        roles: serde_json::from_str(&roles_json).unwrap_or_default(),
                        is_active: row.get(6)?,
                        email_verified: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("Failed to get user: {}", e)))?;
        
        Ok(result)
    }

    /// Update user's last login time
    pub async fn update_last_login(&self, user_id: &UserId) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE users SET last_login_at = datetime('now'), updated_at = datetime('now') WHERE id = ?",
            params![user_id.0.to_string()],
        )
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
        let conn = self.conn.lock().await;
        let id = Uuid::new_v4().to_string();
        
        conn.execute(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) VALUES (?, ?, ?, ?)",
            params![id, user_id.0.to_string(), token_hash, expires_at.to_rfc3339()],
        )
        .map_err(|e| Error::Database(format!("Failed to store refresh token: {}", e)))?;
        
        Ok(id)
    }

    /// Validate and get user for refresh token
    pub async fn validate_refresh_token(&self, token_hash: &str) -> Result<Option<UserId>> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                r#"SELECT user_id FROM refresh_tokens 
                   WHERE token_hash = ? 
                   AND revoked_at IS NULL 
                   AND expires_at > datetime('now')"#,
                params![token_hash],
                |row| {
                    let user_id: String = row.get(0)?;
                    Ok(UserId::from_uuid(Uuid::parse_str(&user_id).unwrap()))
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("Failed to validate refresh token: {}", e)))?;
        
        Ok(result)
    }

    /// Revoke a refresh token
    pub async fn revoke_refresh_token(&self, token_hash: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE refresh_tokens SET revoked_at = datetime('now') WHERE token_hash = ?",
            params![token_hash],
        )
        .map_err(|e| Error::Database(format!("Failed to revoke refresh token: {}", e)))?;
        Ok(())
    }

    /// Revoke all refresh tokens for a user
    pub async fn revoke_all_user_tokens(&self, user_id: &UserId) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE refresh_tokens SET revoked_at = datetime('now') WHERE user_id = ? AND revoked_at IS NULL",
            params![user_id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to revoke user tokens: {}", e)))?;
        Ok(())
    }

    /// Health check - verifies database connectivity
    pub async fn health_check(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch("SELECT 1")
            .map_err(|e| Error::Database(format!("Health check failed: {}", e)))?;
        Ok(())
    }
}

/// Tenant record from database
#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: TenantId,
    pub name: String,
    pub settings: TenantSettings,
    pub enabled_modules: Vec<Module>,
    pub is_active: bool,
}

/// User record from database
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub roles: Vec<Role>,
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
