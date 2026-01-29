//! Audit log repository implementation

use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    traits::{AuditFilters, AuditService},
    types::{Pagination, PaginatedResponse, TenantId},
    Error, Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::params;
use std::sync::Arc;
use uuid::Uuid;

use crate::DatabaseManager;

/// SQLite implementation of the audit service
pub struct AuditRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl AuditRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    /// Run migrations to create audit tables
    pub async fn run_migrations(&self, tenant_id: &TenantId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute_batch(
            r#"
            -- Audit log table
            CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                user_id TEXT,
                user_email TEXT,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                description TEXT NOT NULL,
                old_value TEXT,
                new_value TEXT,
                metadata TEXT,
                ip_address TEXT,
                user_agent TEXT,
                request_id TEXT,
                created_at TEXT NOT NULL
            );

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_audit_created_at ON audit_log(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_log(user_id);
            CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action);
            CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_log(resource_type, resource_id);
            "#,
        )
        .map_err(|e| Error::Migration(format!("Failed to create audit tables: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl AuditService for AuditRepositoryImpl {
    async fn log(&self, entry: AuditEntry) -> Result<()> {
        let db = self.db_manager.tenant(&entry.tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let action_str = serde_json::to_string(&entry.action)
            .map_err(|e| Error::Database(format!("Failed to serialize action: {}", e)))?
            .trim_matches('"')
            .to_string();

        let resource_type_str = serde_json::to_string(&entry.resource_type)
            .map_err(|e| Error::Database(format!("Failed to serialize resource_type: {}", e)))?
            .trim_matches('"')
            .to_string();

        let old_value = entry
            .old_value
            .map(|v| serde_json::to_string(&v))
            .transpose()
            .map_err(|e| Error::Database(format!("Failed to serialize old_value: {}", e)))?;

        let new_value = entry
            .new_value
            .map(|v| serde_json::to_string(&v))
            .transpose()
            .map_err(|e| Error::Database(format!("Failed to serialize new_value: {}", e)))?;

        let metadata = entry
            .metadata
            .map(|v| serde_json::to_string(&v))
            .transpose()
            .map_err(|e| Error::Database(format!("Failed to serialize metadata: {}", e)))?;

        conn.execute(
            r#"INSERT INTO audit_log (
                id, user_id, user_email, action, resource_type, resource_id,
                description, old_value, new_value, metadata,
                ip_address, user_agent, request_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                entry.id.to_string(),
                entry.user_id.map(|u| u.0.to_string()),
                entry.user_email,
                action_str,
                resource_type_str,
                entry.resource_id,
                entry.description,
                old_value,
                new_value,
                metadata,
                entry.ip_address,
                entry.user_agent,
                entry.request_id,
                entry.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to insert audit entry: {}", e)))?;

        tracing::debug!(
            action = %action_str,
            resource_type = %resource_type_str,
            resource_id = %entry.resource_id,
            "Audit entry logged"
        );

        Ok(())
    }

    async fn query(
        &self,
        tenant_id: &TenantId,
        filters: AuditFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<AuditEntry>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Build WHERE clause
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref user_id) = filters.user_id {
            conditions.push("user_id = ?");
            params_vec.push(Box::new(user_id.0.to_string()));
        }

        if let Some(ref action) = filters.action {
            conditions.push("action = ?");
            params_vec.push(Box::new(action.clone()));
        }

        if let Some(ref resource_type) = filters.resource_type {
            conditions.push("resource_type = ?");
            params_vec.push(Box::new(resource_type.clone()));
        }

        if let Some(ref resource_id) = filters.resource_id {
            conditions.push("resource_id = ?");
            params_vec.push(Box::new(resource_id.clone()));
        }

        if let Some(ref from_date) = filters.from_date {
            conditions.push("created_at >= ?");
            params_vec.push(Box::new(from_date.to_rfc3339()));
        }

        if let Some(ref to_date) = filters.to_date {
            conditions.push("created_at <= ?");
            params_vec.push(Box::new(to_date.to_rfc3339()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Count total
        let count_sql = format!("SELECT COUNT(*) FROM audit_log {}", where_clause);
        let total: i64 = {
            let mut stmt = conn
                .prepare(&count_sql)
                .map_err(|e| Error::Database(format!("Failed to prepare count query: {}", e)))?;

            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();

            stmt.query_row(params_refs.as_slice(), |row| row.get(0))
                .map_err(|e| Error::Database(format!("Failed to count audit entries: {}", e)))?
        };

        // Fetch entries
        let query_sql = format!(
            r#"SELECT id, user_id, user_email, action, resource_type, resource_id,
                      description, old_value, new_value, metadata,
                      ip_address, user_agent, request_id, created_at
               FROM audit_log {}
               ORDER BY created_at DESC
               LIMIT ? OFFSET ?"#,
            where_clause
        );

        let tenant_clone = tenant_id.clone();
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        // Add pagination params
        params_vec.push(Box::new(pagination.per_page as i64));
        params_vec.push(Box::new(offset as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let user_id_str: Option<String> = row.get(1)?;
                let action_str: String = row.get(3)?;
                let resource_type_str: String = row.get(4)?;
                let old_value_str: Option<String> = row.get(7)?;
                let new_value_str: Option<String> = row.get(8)?;
                let metadata_str: Option<String> = row.get(9)?;
                let created_at_str: String = row.get(13)?;

                Ok(AuditEntry {
                    id: Uuid::parse_str(&id_str).unwrap_or_default(),
                    tenant_id: tenant_clone.clone(),
                    user_id: user_id_str.and_then(|s| {
                        Uuid::parse_str(&s)
                            .ok()
                            .map(billforge_core::types::UserId::from_uuid)
                    }),
                    user_email: row.get(2)?,
                    action: serde_json::from_str(&format!("\"{}\"", action_str))
                        .unwrap_or(AuditAction::Read),
                    resource_type: serde_json::from_str(&format!("\"{}\"", resource_type_str))
                        .unwrap_or(ResourceType::Invoice),
                    resource_id: row.get(5)?,
                    description: row.get(6)?,
                    old_value: old_value_str.and_then(|s| serde_json::from_str(&s).ok()),
                    new_value: new_value_str.and_then(|s| serde_json::from_str(&s).ok()),
                    metadata: metadata_str.and_then(|s| serde_json::from_str(&s).ok()),
                    ip_address: row.get(10)?,
                    user_agent: row.get(11)?,
                    request_id: row.get(12)?,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .map_err(|e| Error::Database(format!("Failed to query audit entries: {}", e)))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }

        let total_pages = ((total as u32) + pagination.per_page - 1) / pagination.per_page;

        Ok(PaginatedResponse {
            data: items,
            pagination: billforge_core::types::PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages,
            },
        })
    }
}
