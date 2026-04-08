//! Audit log repository implementation

use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    traits::{AuditFilters, AuditService},
    types::{Pagination, PaginatedResponse, PaginationMeta, TenantId},
    Error, Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// PostgreSQL implementation of the audit service
pub struct AuditRepositoryImpl {
    pool: Arc<PgPool>,
}

impl AuditRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Run migrations to create audit tables
    pub async fn run_migrations(&self, _tenant_id: &TenantId) -> Result<()> {
        // Audit tables are created in tenant_db.rs migrations
        Ok(())
    }
}

#[async_trait]
impl AuditService for AuditRepositoryImpl {
    async fn log(&self, entry: AuditEntry) -> Result<()> {
        let action_str = serde_json::to_string(&entry.action)
            .map_err(|e| Error::Database(format!("Failed to serialize action: {}", e)))?
            .trim_matches('"')
            .to_string();

        let resource_type_str = serde_json::to_string(&entry.resource_type)
            .map_err(|e| Error::Database(format!("Failed to serialize resource_type: {}", e)))?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            r#"INSERT INTO audit_log (
                id, tenant_id, user_id, action, resource_type, resource_id,
                changes, ip_address, user_agent, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(entry.id)
        .bind(*entry.tenant_id.as_uuid())
        .bind(entry.user_id.map(|u| u.0))
        .bind(&action_str)
        .bind(&resource_type_str)
        .bind(&entry.resource_id)
        .bind(&serde_json::json!({
            "description": entry.description,
            "old_value": entry.old_value,
            "new_value": entry.new_value,
            "user_email": entry.user_email,
            "metadata": entry.metadata,
        }))
        .bind(&entry.ip_address)
        .bind(&entry.user_agent)
        .bind(entry.created_at)
        .execute(&*self.pool)
        .await
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
        // Build dynamic query
        let mut query_str = String::from(
            "SELECT id, tenant_id, user_id, action, resource_type, resource_id, \
             changes, ip_address, user_agent, created_at FROM audit_log WHERE tenant_id = $1"
        );
        let mut param_count = 2;

        // Add filters
        if filters.user_id.is_some() {
            query_str.push_str(&format!(" AND user_id = ${}", param_count));
            param_count += 1;
        }
        if filters.resource_type.is_some() {
            query_str.push_str(&format!(" AND resource_type = ${}", param_count));
            param_count += 1;
        }
        if filters.resource_id.is_some() {
            query_str.push_str(&format!(" AND resource_id = ${}", param_count));
            param_count += 1;
        }
        if filters.action.is_some() {
            query_str.push_str(&format!(" AND action = ${}", param_count));
            param_count += 1;
        }
        if filters.from_date.is_some() {
            query_str.push_str(&format!(" AND created_at >= ${}", param_count));
            param_count += 1;
        }
        if filters.to_date.is_some() {
            query_str.push_str(&format!(" AND created_at <= ${}", param_count));
            param_count += 1;
        }

        // Add ordering and pagination
        query_str.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            param_count,
            param_count + 1
        ));

        // Build the query
        let mut query = sqlx::query_as::<_, AuditRow>(&query_str)
            .bind(*tenant_id.as_uuid());

        if let Some(ref user_id) = filters.user_id {
            query = query.bind(user_id.0);
        }
        if let Some(ref resource_type) = filters.resource_type {
            let rt_str = serde_json::to_string(resource_type).unwrap();
            query = query.bind(rt_str.trim_matches('"').to_string());
        }
        if let Some(ref resource_id) = filters.resource_id {
            query = query.bind(resource_id);
        }
        if let Some(ref action) = filters.action {
            let action_str = serde_json::to_string(action).unwrap();
            query = query.bind(action_str.trim_matches('"').to_string());
        }
        if let Some(from_date) = filters.from_date {
            query = query.bind(from_date);
        }
        if let Some(to_date) = filters.to_date {
            query = query.bind(to_date);
        }

        // Add pagination
        let offset = ((pagination.page - 1) * pagination.per_page) as i32;
        query = query.bind(pagination.per_page as i32).bind(offset);

        let rows = query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to query audit log: {}", e)))?;

        let entries: Vec<AuditEntry> = rows
            .into_iter()
            .map(|row| row.into_entry())
            .collect();

        // Get total count
        let count_query_str = query_str.replace(
            "SELECT id, tenant_id, user_id, action, resource_type, resource_id, changes, ip_address, user_agent, created_at",
            "SELECT COUNT(*)"
        );
        let count_str = count_query_str.split(" ORDER BY").next().unwrap();

        let mut count_query = sqlx::query_scalar::<_, i64>(count_str)
            .bind(*tenant_id.as_uuid());

        if let Some(ref user_id) = filters.user_id {
            count_query = count_query.bind(user_id.0);
        }
        if let Some(ref resource_type) = filters.resource_type {
            let rt_str = serde_json::to_string(resource_type).unwrap();
            count_query = count_query.bind(rt_str.trim_matches('"').to_string());
        }
        if let Some(ref resource_id) = filters.resource_id {
            count_query = count_query.bind(resource_id);
        }
        if let Some(ref action) = filters.action {
            let action_str = serde_json::to_string(action).unwrap();
            count_query = count_query.bind(action_str.trim_matches('"').to_string());
        }
        if let Some(from_date) = filters.from_date {
            count_query = count_query.bind(from_date);
        }
        if let Some(to_date) = filters.to_date {
            count_query = count_query.bind(to_date);
        }

        let total = count_query
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count audit entries: {}", e)))?;

        Ok(PaginatedResponse {
            data: entries,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }
}

/// Helper struct for mapping database rows
#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    action: String,
    resource_type: String,
    resource_id: String,
    changes: Option<serde_json::Value>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_at: DateTime<Utc>,
}

impl AuditRow {
    fn into_entry(self) -> AuditEntry {
        // Unpack the structured changes JSON written by log()
        let (description, old_value, new_value, user_email, metadata) =
            if let Some(ref changes) = self.changes {
                (
                    changes.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    changes.get("old_value").cloned().filter(|v| !v.is_null()),
                    changes.get("new_value").cloned().filter(|v| !v.is_null()),
                    changes.get("user_email").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    changes.get("metadata").cloned().filter(|v| !v.is_null()),
                )
            } else {
                (String::new(), None, None, None, None)
            };

        AuditEntry {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            user_id: self.user_id.map(billforge_core::UserId),
            user_email,
            action: serde_json::from_str(&format!("\"{}\"", self.action)).unwrap_or(AuditAction::Read),
            resource_type: serde_json::from_str(&format!("\"{}\"", self.resource_type)).unwrap_or(ResourceType::Invoice),
            resource_id: self.resource_id,
            description,
            old_value,
            new_value,
            metadata,
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            request_id: None,
            created_at: self.created_at,
        }
    }
}
