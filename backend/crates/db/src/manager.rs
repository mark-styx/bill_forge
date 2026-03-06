//! Database manager for multi-tenant isolation
//!
//! This module provides a transitional DatabaseManager that wraps
//! the new PgManager for backward compatibility.

use crate::metadata_db::MetadataDatabase;
use crate::pg_manager::PgManager;
use billforge_core::{Error, Result, TenantId};
use sqlx::PgPool;
use std::sync::Arc;

/// Database manager that provides multi-tenant database access
///
/// This is a transitional wrapper around PgManager
pub struct DatabaseManager {
    pg_manager: PgManager,
}

impl DatabaseManager {
    /// Create a new database manager (SQLite compatibility mode)
    pub async fn new(_metadata_db_url: &str, _tenant_db_path: &str) -> Result<Self> {
        // For now, use environment variables for PostgreSQL connection
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|e| Error::Database(format!("DATABASE_URL not set: {}", e)))?;

        let tenant_template = std::env::var("TENANT_DB_TEMPLATE")
            .unwrap_or_else(|_| database_url.clone());

        let pg_manager = PgManager::new(&database_url, tenant_template).await?;

        Ok(Self { pg_manager })
    }

    /// Get the metadata database (returns a pool for compatibility)
    pub fn metadata(&self) -> Arc<PgPool> {
        Arc::new(self.pg_manager.metadata().clone())
    }

    /// Get tenant connection pool
    pub async fn tenant(&self, tenant_id: &TenantId) -> Result<Arc<PgPool>> {
        self.pg_manager.tenant(tenant_id).await
    }

    /// Create a new tenant with its database
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        self.pg_manager.create_tenant(tenant_id, name).await
    }

    /// Delete a tenant and its database
    pub async fn delete_tenant(&self, tenant_id: &TenantId) -> Result<()> {
        self.pg_manager.delete_tenant(tenant_id).await
    }

    /// Close all connections (for graceful shutdown)
    pub async fn close(&self) {
        // PgManager doesn't have an explicit close method yet
    }
}
