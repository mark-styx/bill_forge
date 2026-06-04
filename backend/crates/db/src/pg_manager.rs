//! Multi-tenant PostgreSQL connection pool manager
//!
//! Implements database-per-tenant isolation using PostgreSQL schemas.
//! Each tenant gets a dedicated database with its own connection pool.

use crate::migrations::MigrationRunner;
use billforge_core::{Error, Result, TenantId};
use sqlx::{postgres::PgPoolOptions, PgConnection, PgPool};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages PostgreSQL connections with tenant isolation
pub struct PgManager {
    /// Connection to the metadata/registry database
    metadata_pool: PgPool,
    /// Cache of tenant database connection pools
    tenant_pools: RwLock<HashMap<String, Arc<PgPool>>>,
    /// PostgreSQL connection string template for tenant databases
    database_url_template: String,
}

impl PgManager {
    /// Create a new multi-tenant PostgreSQL manager
    ///
    /// # Arguments
    /// * `metadata_db_url` - Connection string for the metadata/registry database
    /// * `database_url_template` - Template for tenant DB URLs with `{database}` placeholder
    ///   Example: `postgres://user:pass@localhost/{database}`
    pub async fn new(metadata_db_url: &str, database_url_template: String) -> Result<Self> {
        // Connect to metadata database
        let metadata_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(metadata_db_url)
            .await
            .map_err(|e| {
                Error::Database(format!("Failed to connect to metadata database: {}", e))
            })?;

        // Warn if the application is running as a superuser or bypass-RLS role,
        // which would undermine RLS guarantees.
        if let Ok(row) = sqlx::query_as::<_, (String, bool, bool)>(
            "SELECT current_user, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(&metadata_pool)
        .await
        {
            let (user, is_super, bypass_rls) = row;
            if is_super || bypass_rls {
                tracing::warn!(
                    current_user = %user,
                    is_super,
                    bypass_rls,
                    "Database role has superuser or BYPASSRLS privilege; RLS is NOT enforced. \
                     Use a dedicated NOSUPERUSER NOBYPASSRLS role (e.g. billforge_app)."
                );
            }
        }

        Ok(Self {
            metadata_pool,
            tenant_pools: RwLock::new(HashMap::new()),
            database_url_template,
        })
    }

    /// Get the metadata database pool
    pub fn metadata(&self) -> &PgPool {
        &self.metadata_pool
    }

    /// Get or create a connection pool for a tenant database
    pub async fn tenant(&self, tenant_id: &TenantId) -> Result<Arc<PgPool>> {
        let tenant_key = tenant_id.as_str();
        let db_name = format!("tenant_{}", tenant_key.replace('-', "_"));

        // Check cache first
        {
            let cache = self.tenant_pools.read().await;
            if let Some(pool) = cache.get(&db_name) {
                return Ok(Arc::clone(pool));
            }
        }

        // Verify tenant exists in metadata
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1)")
            .bind(tenant_id.as_uuid())
            .fetch_one(&self.metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to check tenant existence: {}", e)))?;

        if !exists {
            return Err(Error::TenantNotFound(tenant_key));
        }

        // Check if tenant database exists, create if not
        self.ensure_tenant_database(&db_name).await?;

        // Create connection pool for tenant database with RLS tenant context
        let tenant_url = self.database_url_template.replace("{database}", &db_name);
        let tenant_uuid = *tenant_id.as_uuid();
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    let set_sql = format!("SET app.current_tenant_id = '{}'", tenant_uuid);
                    tracing::debug!(tenant_id = %tenant_uuid, "Setting RLS app.current_tenant_id on new pool connection");
                    sqlx::query(&set_sql)
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&tenant_url)
            .await
            .map_err(|e| {
                Error::Database(format!(
                    "Failed to connect to tenant database {}: {}",
                    db_name, e
                ))
            })?;

        let pool = Arc::new(pool);

        // Cache the pool
        {
            let mut cache = self.tenant_pools.write().await;
            cache.insert(db_name, Arc::clone(&pool));
        }

        Ok(pool)
    }

    /// Explicitly set `app.current_tenant_id` on a raw connection.
    ///
    /// Use this as an escape hatch when acquiring connections via `pool.acquire()`
    /// or beginning transactions on the metadata pool, to re-affirm the tenant
    /// binding inside a `BEGIN` block. Connections obtained through [`PgManager::tenant`]
    /// already have the variable set via the pool's `after_connect` hook.
    pub async fn set_tenant_context(conn: &mut PgConnection, tenant_id: &TenantId) -> Result<()> {
        let tenant_uuid = tenant_id.as_uuid();
        sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
            .bind(tenant_uuid.to_string())
            .execute(conn)
            .await
            .map_err(|e| {
                Error::Database(format!(
                    "Failed to set app.current_tenant_id to {}: {}",
                    tenant_uuid, e
                ))
            })?;
        tracing::debug!(tenant_id = %tenant_uuid, "set_tenant_context: RLS app.current_tenant_id bound");
        Ok(())
    }

    /// Create a new tenant with its dedicated database
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        let tenant_key = tenant_id.as_str();
        let db_name = format!("tenant_{}", tenant_key.replace('-', "_"));

        // Insert into metadata database
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3)")
            .bind(tenant_id.as_uuid())
            .bind(name)
            .bind(slugify(name))
            .execute(&self.metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to create tenant in metadata: {}", e)))?;

        // Create the tenant database
        self.ensure_tenant_database(&db_name).await?;

        Ok(())
    }

    /// Ensure a tenant database exists
    async fn ensure_tenant_database(&self, db_name: &str) -> Result<()> {
        // Check if database exists
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(db_name)
                .fetch_one(&self.metadata_pool)
                .await
                .map_err(|e| {
                    Error::Database(format!("Failed to check database existence: {}", e))
                })?;

        if !exists {
            // Create database (cannot use parameters in CREATE DATABASE)
            let create_sql = format!("CREATE DATABASE {}", db_name);
            sqlx::query(&create_sql)
                .execute(&self.metadata_pool)
                .await
                .map_err(|e| {
                    Error::Database(format!("Failed to create database {}: {}", db_name, e))
                })?;

            tracing::info!("Created tenant database: {}", db_name);
        }

        Ok(())
    }

    /// Delete a tenant and its database
    pub async fn delete_tenant(&self, tenant_id: &TenantId) -> Result<()> {
        let tenant_key = tenant_id.as_str();
        let db_name = format!("tenant_{}", tenant_key.replace('-', "_"));

        // Remove from cache
        {
            let mut cache = self.tenant_pools.write().await;
            cache.remove(&db_name);
        }

        // Drop the database
        let drop_sql = format!("DROP DATABASE IF EXISTS {}", db_name);
        sqlx::query(&drop_sql)
            .execute(&self.metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to drop database {}: {}", db_name, e)))?;

        // Delete from metadata
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id.as_uuid())
            .execute(&self.metadata_pool)
            .await
            .map_err(|e| {
                Error::Database(format!("Failed to delete tenant from metadata: {}", e))
            })?;

        tracing::info!("Deleted tenant database: {}", db_name);
        Ok(())
    }

    /// Run migrations on all tenant databases
    pub async fn migrate_all_tenants(&self) -> Result<()> {
        let tenants: Vec<String> = sqlx::query_scalar("SELECT id FROM tenants")
            .fetch_all(&self.metadata_pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list tenants: {}", e)))?;

        for tenant_id_str in tenants {
            let tenant_id: TenantId = tenant_id_str.parse().map_err(|e| {
                Error::Database(format!("Invalid tenant ID {}: {}", tenant_id_str, e))
            })?;
            let pool = self.tenant(&tenant_id).await?;
            self.run_tenant_migrations(&pool).await?;
        }

        Ok(())
    }

    /// Run migrations on a single tenant database
    pub async fn run_tenant_migrations(&self, pool: &PgPool) -> Result<()> {
        tracing::info!("Running tenant migrations...");
        let migration_runner = MigrationRunner::new();

        // Run all migrations in order
        // These are the same migrations used by the migrate binary
        let migrations = vec![
            (
                "001_create_tenants.sql",
                include_str!("../../../migrations/001_create_tenants.sql"),
            ),
            (
                "002_create_users.sql",
                include_str!("../../../migrations/002_create_users.sql"),
            ),
            (
                "003_create_vendors.sql",
                include_str!("../../../migrations/003_create_vendors.sql"),
            ),
            (
                "004_create_invoices.sql",
                include_str!("../../../migrations/004_create_invoices.sql"),
            ),
            (
                "005_create_workflow_tables.sql",
                include_str!("../../../migrations/005_create_workflow_tables.sql"),
            ),
            (
                "006_create_quickbooks_tables.sql",
                include_str!("../../../migrations/006_create_quickbooks_tables.sql"),
            ),
            (
                "007_create_vendor_documents.sql",
                include_str!("../../../migrations/007_create_vendor_documents.sql"),
            ),
            (
                "008_create_vendor_contacts.sql",
                include_str!("../../../migrations/008_create_vendor_contacts.sql"),
            ),
            (
                "009_create_email_notifications.sql",
                include_str!("../../../migrations/009_create_email_notifications.sql"),
            ),
        ];

        for (name, sql) in migrations {
            migration_runner.apply(pool, name, sql).await?;
        }

        // Run additional migration groups (idempotent, uses IF NOT EXISTS)
        crate::tenant_db::run_workflow_migrations(pool).await?;
        crate::tenant_db::run_purchase_order_migrations(pool).await?;
        crate::tenant_db::run_edi_outbound_migrations(pool).await?;
        crate::tenant_db::run_implementation_migrations(pool).await?;

        // Payment requests
        migration_runner
            .apply(
                pool,
                "069_create_payment_requests.sql",
                include_str!("../../../migrations/069_create_payment_requests.sql"),
            )
            .await?;

        // Intelligent routing tables (idempotent, uses IF NOT EXISTS)
        migration_runner
            .apply(
                pool,
                "051_add_intelligent_routing.sql",
                include_str!("../../../migrations/051_add_intelligent_routing.sql"),
            )
            .await?;

        // Integration webhook support (nonces table + webhook_secret columns)
        migration_runner
            .apply(
                pool,
                "070_add_integration_webhook_support.sql",
                include_str!("../../../migrations/070_add_integration_webhook_support.sql"),
            )
            .await?;

        // Core tenant FK constraints (users, vendors, invoices -> tenants)
        migration_runner
            .apply(
                pool,
                "071_add_core_tenant_fk_constraints.sql",
                include_str!("../../../migrations/071_add_core_tenant_fk_constraints.sql"),
            )
            .await?;

        // Feedback correction rules + confidence calibration
        migration_runner
            .apply(
                pool,
                "072_add_feedback_correction_rules.sql",
                include_str!("../../../migrations/072_add_feedback_correction_rules.sql"),
            )
            .await?;

        // Vendor columns for QuickBooks sync (email, phone, vendor_type, status)
        migration_runner
            .apply(
                pool,
                "074_add_vendor_sync_columns.sql",
                include_str!("../../../migrations/074_add_vendor_sync_columns.sql"),
            )
            .await?;

        // Row Level Security on core tenant tables (invoices, users, vendors)
        migration_runner
            .apply(
                pool,
                "080_enable_rls_core_tables.sql",
                include_str!("../../../migrations/080_enable_rls_core_tables.sql"),
            )
            .await?;

        // Harden RLS policies for unset/reset app.current_tenant_id values.
        migration_runner
            .apply(
                pool,
                "092_harden_core_rls_current_tenant_setting.sql",
                include_str!("../../../migrations/092_harden_core_rls_current_tenant_setting.sql"),
            )
            .await?;

        // Keep tenant DBs created by PgManager aligned with canonical tenant_id UUID types.
        migration_runner
            .apply(
                pool,
                "093_harden_workflow_tenant_id_uuid_types.sql",
                include_str!("../../../migrations/093_harden_workflow_tenant_id_uuid_types.sql"),
            )
            .await?;

        migration_runner
            .apply(
                pool,
                "094_add_queue_items_updated_at.sql",
                include_str!("../../../migrations/094_add_queue_items_updated_at.sql"),
            )
            .await?;

        // AI conversations and messages (scoped by tenant + user)
        crate::tenant_db::run_ai_conversation_migrations(pool).await?;

        // FORCE RLS on AI tables + create billforge_app role
        crate::tenant_db::run_ai_rls_migrations(pool).await?;

        tracing::info!("Tenant migrations completed successfully");
        Ok(())
    }

    /// Close all connections (for graceful shutdown)
    pub async fn close(&self) {
        let mut cache = self.tenant_pools.write().await;
        cache.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Acme Corporation"), "acme-corporation");
        assert_eq!(slugify("My Company LLC"), "my-company-llc");
        assert_eq!(slugify("Test@Company#123"), "test-company-123");
    }
}
