//! Database manager for multi-tenant isolation

use crate::metadata_db::MetadataDatabase;
use crate::tenant_db::TenantDatabase;
use billforge_core::{Error, Result, TenantId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages all database connections with tenant isolation
pub struct DatabaseManager {
    /// Path where tenant databases are stored
    tenant_db_path: PathBuf,
    /// Metadata/auth database
    metadata_db: Arc<MetadataDatabase>,
    /// Cache of tenant database connections
    tenant_dbs: RwLock<HashMap<String, Arc<TenantDatabase>>>,
}

impl DatabaseManager {
    /// Create a new database manager
    pub async fn new(metadata_db_url: &str, tenant_db_path: &str) -> Result<Self> {
        let tenant_path = PathBuf::from(tenant_db_path);
        
        // Create tenant directory if it doesn't exist
        tokio::fs::create_dir_all(&tenant_path)
            .await
            .map_err(|e| Error::Database(format!("Failed to create tenant db path: {}", e)))?;

        let metadata_db = MetadataDatabase::new(metadata_db_url).await?;
        
        Ok(Self {
            tenant_db_path: tenant_path,
            metadata_db: Arc::new(metadata_db),
            tenant_dbs: RwLock::new(HashMap::new()),
        })
    }

    /// Get the metadata database
    pub fn metadata(&self) -> Arc<MetadataDatabase> {
        Arc::clone(&self.metadata_db)
    }

    /// Get or create a tenant database connection
    pub async fn tenant(&self, tenant_id: &TenantId) -> Result<Arc<TenantDatabase>> {
        let tenant_key = tenant_id.as_str();
        
        // Check cache first
        {
            let cache = self.tenant_dbs.read().await;
            if let Some(db) = cache.get(&tenant_key) {
                return Ok(Arc::clone(db));
            }
        }

        // Verify tenant exists in metadata
        if !self.metadata_db.tenant_exists(tenant_id).await? {
            return Err(Error::TenantNotFound(tenant_key.clone()));
        }

        // Create new connection
        let db_path = self.tenant_db_path.join(format!("{}.db", tenant_key));
        let tenant_db = TenantDatabase::new(db_path.to_str().unwrap(), tenant_id.clone()).await?;
        let tenant_db = Arc::new(tenant_db);

        // Cache it
        {
            let mut cache = self.tenant_dbs.write().await;
            cache.insert(tenant_key, Arc::clone(&tenant_db));
        }

        Ok(tenant_db)
    }

    /// Create a new tenant with its database
    pub async fn create_tenant(&self, tenant_id: &TenantId, name: &str) -> Result<()> {
        // Create tenant in metadata
        self.metadata_db.create_tenant(tenant_id, name).await?;
        
        // Create and initialize tenant database
        let db_path = self.tenant_db_path.join(format!("{}.db", tenant_id.as_str()));
        let tenant_db = TenantDatabase::new(db_path.to_str().unwrap(), tenant_id.clone()).await?;
        
        // Run migrations
        tenant_db.run_migrations().await?;

        Ok(())
    }

    /// Delete a tenant and its database
    pub async fn delete_tenant(&self, tenant_id: &TenantId) -> Result<()> {
        let tenant_key = tenant_id.as_str();
        
        // Remove from cache
        {
            let mut cache = self.tenant_dbs.write().await;
            cache.remove(&tenant_key);
        }

        // Delete from metadata
        self.metadata_db.delete_tenant(tenant_id).await?;

        // Delete database file
        let db_path = self.tenant_db_path.join(format!("{}.db", tenant_key));
        if db_path.exists() {
            tokio::fs::remove_file(&db_path)
                .await
                .map_err(|e| Error::Database(format!("Failed to delete tenant db: {}", e)))?;
        }

        Ok(())
    }

    /// Run migrations on all tenant databases
    pub async fn migrate_all_tenants(&self) -> Result<()> {
        let tenants = self.metadata_db.list_all_tenants().await?;
        
        for tenant_id in tenants {
            let db = self.tenant(&tenant_id).await?;
            db.run_migrations().await?;
        }

        Ok(())
    }

    /// Close all connections (for graceful shutdown)
    pub async fn close(&self) {
        let mut cache = self.tenant_dbs.write().await;
        cache.clear();
    }
}
