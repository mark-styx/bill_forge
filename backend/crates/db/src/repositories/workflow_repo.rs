//! Workflow repository implementation

use crate::manager::DatabaseManager;
use async_trait::async_trait;
use billforge_core::{
    domain::*,
    traits::{WorkflowRuleRepository, WorkQueueRepository, ApprovalRepository, AssignmentRuleRepository},
    types::*,
    Error, Result,
};
use chrono::Utc;
use rusqlite::params;
use std::sync::Arc;
use uuid::Uuid;

pub struct WorkflowRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl WorkflowRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }
}

#[async_trait]
impl WorkflowRuleRepository for WorkflowRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateWorkflowRuleInput,
    ) -> Result<WorkflowRule> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = WorkflowRuleId::new();
        let now = Utc::now();

        let conditions_json = serde_json::to_string(&input.conditions)
            .map_err(|e| Error::Database(format!("Failed to serialize conditions: {}", e)))?;
        let actions_json = serde_json::to_string(&input.actions)
            .map_err(|e| Error::Database(format!("Failed to serialize actions: {}", e)))?;

        conn.execute(
            r#"INSERT INTO workflow_rules (
                id, name, description, priority, is_active, rule_type,
                conditions, actions, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                id.0.to_string(),
                input.name,
                input.description,
                input.priority,
                true,
                format!("{:?}", input.rule_type).to_lowercase(),
                conditions_json,
                actions_json,
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to create workflow rule: {}", e)))?;

        Ok(WorkflowRule {
            id,
            tenant_id: tenant_id.clone(),
            name: input.name,
            description: input.description,
            priority: input.priority,
            is_active: true,
            rule_type: input.rule_type,
            conditions: input.conditions,
            actions: input.actions,
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(
        &self,
        tenant_id: &TenantId,
        id: &WorkflowRuleId,
    ) -> Result<Option<WorkflowRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, description, priority, is_active, rule_type,
                          conditions, actions, created_at, updated_at
                   FROM workflow_rules WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row(params![id.0.to_string()], |row| {
                Ok(self.map_rule_row(row, tenant_id.clone()))
            })
            .ok();

        match result {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        rule_type: Option<WorkflowRuleType>,
    ) -> Result<Vec<WorkflowRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let sql = if let Some(rt) = rule_type {
            format!(
                r#"SELECT id, name, description, priority, is_active, rule_type,
                          conditions, actions, created_at, updated_at
                   FROM workflow_rules WHERE rule_type = '{}'
                   ORDER BY priority DESC"#,
                format!("{:?}", rt).to_lowercase()
            )
        } else {
            r#"SELECT id, name, description, priority, is_active, rule_type,
                      conditions, actions, created_at, updated_at
               FROM workflow_rules ORDER BY priority DESC"#
                .to_string()
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let rules = stmt
            .query_map([], |row| Ok(self.map_rule_row(row, tenant_clone.clone())))
            .map_err(|e| Error::Database(format!("Failed to list rules: {}", e)))?;

        let mut results = Vec::new();
        for rule in rules {
            results.push(rule.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(results)
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &WorkflowRuleId,
        input: CreateWorkflowRuleInput,
    ) -> Result<WorkflowRule> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();
        let conditions_json = serde_json::to_string(&input.conditions)
            .map_err(|e| Error::Database(format!("Failed to serialize conditions: {}", e)))?;
        let actions_json = serde_json::to_string(&input.actions)
            .map_err(|e| Error::Database(format!("Failed to serialize actions: {}", e)))?;

        conn.execute(
            r#"UPDATE workflow_rules SET
                name = ?, description = ?, priority = ?, rule_type = ?,
                conditions = ?, actions = ?, updated_at = ?
               WHERE id = ?"#,
            params![
                input.name,
                input.description,
                input.priority,
                format!("{:?}", input.rule_type).to_lowercase(),
                conditions_json,
                actions_json,
                now.to_rfc3339(),
                id.0.to_string(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to update workflow rule: {}", e)))?;

        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "WorkflowRule".to_string(),
                id: id.0.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &WorkflowRuleId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "DELETE FROM workflow_rules WHERE id = ?",
            params![id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete workflow rule: {}", e)))?;

        Ok(())
    }

    async fn set_active(
        &self,
        tenant_id: &TenantId,
        id: &WorkflowRuleId,
        is_active: bool,
    ) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE workflow_rules SET is_active = ?, updated_at = ? WHERE id = ?",
            params![is_active, Utc::now().to_rfc3339(), id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to update rule status: {}", e)))?;

        Ok(())
    }

    async fn get_active_rules(
        &self,
        tenant_id: &TenantId,
        rule_type: WorkflowRuleType,
    ) -> Result<Vec<WorkflowRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let sql = format!(
            r#"SELECT id, name, description, priority, is_active, rule_type,
                      conditions, actions, created_at, updated_at
               FROM workflow_rules
               WHERE rule_type = '{}' AND is_active = true
               ORDER BY priority DESC"#,
            format!("{:?}", rule_type).to_lowercase()
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let rules = stmt
            .query_map([], |row| Ok(self.map_rule_row(row, tenant_clone.clone())))
            .map_err(|e| Error::Database(format!("Failed to list active rules: {}", e)))?;

        let mut results = Vec::new();
        for rule in rules {
            results.push(rule.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(results)
    }
}

impl WorkflowRepositoryImpl {
    fn map_rule_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<WorkflowRule> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
        let conditions_json: String = row.get(6).map_err(|e| Error::Database(e.to_string()))?;
        let actions_json: String = row.get(7).map_err(|e| Error::Database(e.to_string()))?;

        Ok(WorkflowRule {
            id: WorkflowRuleId(Uuid::parse_str(&id_str).unwrap()),
            tenant_id,
            name: row.get(1).map_err(|e| Error::Database(e.to_string()))?,
            description: row.get(2).map_err(|e| Error::Database(e.to_string()))?,
            priority: row.get(3).map_err(|e| Error::Database(e.to_string()))?,
            is_active: row.get(4).map_err(|e| Error::Database(e.to_string()))?,
            rule_type: WorkflowRuleType::Routing,
            conditions: serde_json::from_str(&conditions_json).unwrap_or_default(),
            actions: serde_json::from_str(&actions_json).unwrap_or_default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
}

// ============================================================================
// Work Queue Repository Implementation
// ============================================================================

pub struct WorkQueueRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl WorkQueueRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    fn map_queue_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<WorkQueue> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
        let queue_type_str: String = row.get(3).map_err(|e| Error::Database(e.to_string()))?;
        let assigned_users_json: Option<String> = row.get(4).ok();
        let assigned_roles_json: Option<String> = row.get(5).ok();
        let settings_json: Option<String> = row.get(8).ok();

        let queue_type = match queue_type_str.as_str() {
            "review" => QueueType::Review,
            "approval" => QueueType::Approval,
            "exception" => QueueType::Exception,
            "payment" => QueueType::Payment,
            _ => QueueType::Custom,
        };

        Ok(WorkQueue {
            id: WorkQueueId(Uuid::parse_str(&id_str).unwrap()),
            tenant_id,
            name: row.get(1).map_err(|e| Error::Database(e.to_string()))?,
            description: row.get(2).ok(),
            queue_type,
            assigned_users: assigned_users_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
            assigned_roles: assigned_roles_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
            is_default: row.get::<_, i32>(6).unwrap_or(0) == 1,
            is_active: row.get::<_, i32>(7).unwrap_or(1) == 1,
            settings: settings_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_else(|| QueueSettings {
                    default_sort: "entered_at".to_string(),
                    sla_hours: None,
                    escalation_hours: None,
                    escalation_user_id: None,
                }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    fn map_queue_item_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<QueueItem> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
        let queue_id_str: String = row.get(1).map_err(|e| Error::Database(e.to_string()))?;
        let invoice_id_str: String = row.get(2).map_err(|e| Error::Database(e.to_string()))?;
        let assigned_to_str: Option<String> = row.get(3).ok();

        Ok(QueueItem {
            id: Uuid::parse_str(&id_str).unwrap(),
            queue_id: WorkQueueId(Uuid::parse_str(&queue_id_str).unwrap()),
            invoice_id: InvoiceId(Uuid::parse_str(&invoice_id_str).unwrap()),
            tenant_id,
            assigned_to: assigned_to_str.and_then(|s| Uuid::parse_str(&s).ok().map(UserId)),
            priority: row.get(4).unwrap_or(0),
            entered_at: Utc::now(),
            due_at: None,
            claimed_at: None,
            completed_at: None,
        })
    }
}

#[async_trait]
impl WorkQueueRepository for WorkQueueRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateWorkQueueInput,
    ) -> Result<WorkQueue> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = WorkQueueId::new();
        let now = Utc::now();

        let queue_type_str = match input.queue_type {
            QueueType::Review => "review",
            QueueType::Approval => "approval",
            QueueType::Exception => "exception",
            QueueType::Payment => "payment",
            QueueType::Custom => "custom",
        };

        let assigned_users_json = serde_json::to_string(&input.assigned_users)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let assigned_roles_json = serde_json::to_string(&input.assigned_roles)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let settings_json = serde_json::to_string(&input.settings)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;

        conn.execute(
            r#"INSERT INTO work_queues (
                id, name, description, queue_type, assigned_users, assigned_roles,
                is_default, is_active, settings, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                id.0.to_string(),
                input.name,
                input.description,
                queue_type_str,
                assigned_users_json,
                assigned_roles_json,
                0,
                1,
                settings_json,
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to create queue: {}", e)))?;

        Ok(WorkQueue {
            id,
            tenant_id: tenant_id.clone(),
            name: input.name,
            description: input.description,
            queue_type: input.queue_type,
            assigned_users: input.assigned_users,
            assigned_roles: input.assigned_roles,
            is_default: false,
            is_active: true,
            settings: input.settings,
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(
        &self,
        tenant_id: &TenantId,
        id: &WorkQueueId,
    ) -> Result<Option<WorkQueue>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, description, queue_type, assigned_users, assigned_roles,
                          is_default, is_active, settings, created_at, updated_at
                   FROM work_queues WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row(params![id.0.to_string()], |row| {
                Ok(self.map_queue_row(row, tenant_id.clone()))
            })
            .ok();

        match result {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<WorkQueue>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, description, queue_type, assigned_users, assigned_roles,
                          is_default, is_active, settings, created_at, updated_at
                   FROM work_queues WHERE is_active = 1 ORDER BY name"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let queues = stmt
            .query_map([], |row| Ok(self.map_queue_row(row, tenant_clone.clone())))
            .map_err(|e| Error::Database(format!("Failed to list queues: {}", e)))?;

        let mut results = Vec::new();
        for queue in queues {
            results.push(queue.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(results)
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &WorkQueueId,
        input: CreateWorkQueueInput,
    ) -> Result<WorkQueue> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();
        let queue_type_str = match input.queue_type {
            QueueType::Review => "review",
            QueueType::Approval => "approval",
            QueueType::Exception => "exception",
            QueueType::Payment => "payment",
            QueueType::Custom => "custom",
        };

        let assigned_users_json = serde_json::to_string(&input.assigned_users)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let assigned_roles_json = serde_json::to_string(&input.assigned_roles)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let settings_json = serde_json::to_string(&input.settings)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;

        conn.execute(
            r#"UPDATE work_queues SET
                name = ?, description = ?, queue_type = ?, assigned_users = ?,
                assigned_roles = ?, settings = ?, updated_at = ?
               WHERE id = ?"#,
            params![
                input.name,
                input.description,
                queue_type_str,
                assigned_users_json,
                assigned_roles_json,
                settings_json,
                now.to_rfc3339(),
                id.0.to_string(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to update queue: {}", e)))?;

        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "WorkQueue".to_string(),
                id: id.0.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &WorkQueueId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE work_queues SET is_active = 0, updated_at = ? WHERE id = ?",
            params![Utc::now().to_rfc3339(), id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete queue: {}", e)))?;

        Ok(())
    }

    async fn get_default(&self, tenant_id: &TenantId) -> Result<Option<WorkQueue>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, description, queue_type, assigned_users, assigned_roles,
                          is_default, is_active, settings, created_at, updated_at
                   FROM work_queues WHERE is_default = 1 AND is_active = 1 LIMIT 1"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row([], |row| {
                Ok(self.map_queue_row(row, tenant_id.clone()))
            })
            .ok();

        match result {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    async fn get_by_type(&self, tenant_id: &TenantId, queue_type: QueueType) -> Result<Option<WorkQueue>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let queue_type_str = match queue_type {
            QueueType::Review => "review",
            QueueType::Approval => "approval",
            QueueType::Exception => "exception",
            QueueType::Payment => "payment",
            QueueType::Custom => "custom",
        };

        let mut stmt = conn
            .prepare(
                r#"SELECT id, name, description, queue_type, assigned_users, assigned_roles,
                          is_default, is_active, settings, created_at, updated_at
                   FROM work_queues WHERE queue_type = ? AND is_active = 1 LIMIT 1"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row(params![queue_type_str], |row| {
                Ok(self.map_queue_row(row, tenant_id.clone()))
            })
            .ok();

        match result {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    async fn add_item(
        &self,
        tenant_id: &TenantId,
        queue_id: &WorkQueueId,
        invoice_id: &InvoiceId,
        assigned_to: Option<&UserId>,
    ) -> Result<QueueItem> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = Uuid::new_v4();
        let now = Utc::now();

        conn.execute(
            r#"INSERT INTO queue_items (id, queue_id, invoice_id, assigned_to, priority, entered_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            params![
                id.to_string(),
                queue_id.0.to_string(),
                invoice_id.0.to_string(),
                assigned_to.map(|u| u.0.to_string()),
                0,
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to add queue item: {}", e)))?;

        // Also update the invoice's current_queue_id
        conn.execute(
            "UPDATE invoices SET current_queue_id = ?, assigned_to = ?, updated_at = ? WHERE id = ?",
            params![
                queue_id.0.to_string(),
                assigned_to.map(|u| u.0.to_string()),
                now.to_rfc3339(),
                invoice_id.0.to_string(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to update invoice queue: {}", e)))?;

        Ok(QueueItem {
            id,
            queue_id: queue_id.clone(),
            invoice_id: invoice_id.clone(),
            tenant_id: tenant_id.clone(),
            assigned_to: assigned_to.cloned(),
            priority: 0,
            entered_at: now,
            due_at: None,
            claimed_at: None,
            completed_at: None,
        })
    }

    async fn get_items(
        &self,
        tenant_id: &TenantId,
        queue_id: &WorkQueueId,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<QueueItem>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let offset = (pagination.page - 1) * pagination.per_page;

        // Get total count
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM queue_items WHERE queue_id = ? AND completed_at IS NULL",
                params![queue_id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("Failed to count items: {}", e)))?;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, invoice_id, assigned_to, priority, entered_at, due_at, claimed_at, completed_at
                   FROM queue_items
                   WHERE queue_id = ? AND completed_at IS NULL
                   ORDER BY priority DESC, entered_at ASC
                   LIMIT ? OFFSET ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let items = stmt
            .query_map(params![queue_id.0.to_string(), pagination.per_page as i64, offset as i64], |row| {
                Ok(self.map_queue_item_row(row, tenant_clone.clone()))
            })
            .map_err(|e| Error::Database(format!("Failed to list items: {}", e)))?;

        let mut data = Vec::new();
        for item in items {
            data.push(item.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(PaginatedResponse {
            data,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn get_items_for_user(
        &self,
        tenant_id: &TenantId,
        queue_id: &WorkQueueId,
        user_id: &UserId,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<QueueItem>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let offset = (pagination.page - 1) * pagination.per_page;

        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM queue_items WHERE queue_id = ? AND assigned_to = ? AND completed_at IS NULL",
                params![queue_id.0.to_string(), user_id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("Failed to count items: {}", e)))?;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, invoice_id, assigned_to, priority, entered_at, due_at, claimed_at, completed_at
                   FROM queue_items
                   WHERE queue_id = ? AND assigned_to = ? AND completed_at IS NULL
                   ORDER BY priority DESC, entered_at ASC
                   LIMIT ? OFFSET ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let items = stmt
            .query_map(
                params![queue_id.0.to_string(), user_id.0.to_string(), pagination.per_page as i64, offset as i64],
                |row| Ok(self.map_queue_item_row(row, tenant_clone.clone())),
            )
            .map_err(|e| Error::Database(format!("Failed to list items: {}", e)))?;

        let mut data = Vec::new();
        for item in items {
            data.push(item.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(PaginatedResponse {
            data,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn claim_item(&self, tenant_id: &TenantId, item_id: Uuid, user_id: &UserId) -> Result<QueueItem> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();

        conn.execute(
            "UPDATE queue_items SET claimed_at = ?, assigned_to = ? WHERE id = ?",
            params![now.to_rfc3339(), user_id.0.to_string(), item_id.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to claim item: {}", e)))?;

        // Get the updated item
        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, invoice_id, assigned_to, priority, entered_at, due_at, claimed_at, completed_at
                   FROM queue_items WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let item = stmt
            .query_row(params![item_id.to_string()], |row| {
                Ok(self.map_queue_item_row(row, tenant_id.clone()))
            })
            .map_err(|e| Error::Database(format!("Item not found: {}", e)))??;

        Ok(item)
    }

    async fn complete_item(&self, tenant_id: &TenantId, item_id: Uuid, action: &str) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();

        conn.execute(
            "UPDATE queue_items SET completed_at = ?, completion_action = ? WHERE id = ?",
            params![now.to_rfc3339(), action, item_id.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to complete item: {}", e)))?;

        Ok(())
    }

    async fn move_item(
        &self,
        tenant_id: &TenantId,
        invoice_id: &InvoiceId,
        to_queue_id: &WorkQueueId,
        assigned_to: Option<&UserId>,
    ) -> Result<QueueItem> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();

        // Mark current queue item as completed (moved)
        conn.execute(
            "UPDATE queue_items SET completed_at = ?, completion_action = 'moved' WHERE invoice_id = ? AND completed_at IS NULL",
            params![now.to_rfc3339(), invoice_id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to complete old item: {}", e)))?;

        // Create new queue item
        let id = Uuid::new_v4();
        conn.execute(
            r#"INSERT INTO queue_items (id, queue_id, invoice_id, assigned_to, priority, entered_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            params![
                id.to_string(),
                to_queue_id.0.to_string(),
                invoice_id.0.to_string(),
                assigned_to.map(|u| u.0.to_string()),
                0,
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to add queue item: {}", e)))?;

        // Update invoice queue tracking
        conn.execute(
            "UPDATE invoices SET current_queue_id = ?, assigned_to = ?, updated_at = ? WHERE id = ?",
            params![
                to_queue_id.0.to_string(),
                assigned_to.map(|u| u.0.to_string()),
                now.to_rfc3339(),
                invoice_id.0.to_string(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to update invoice queue: {}", e)))?;

        Ok(QueueItem {
            id,
            queue_id: to_queue_id.clone(),
            invoice_id: invoice_id.clone(),
            tenant_id: tenant_id.clone(),
            assigned_to: assigned_to.cloned(),
            priority: 0,
            entered_at: now,
            due_at: None,
            claimed_at: None,
            completed_at: None,
        })
    }

    async fn count_items(&self, tenant_id: &TenantId, queue_id: &WorkQueueId) -> Result<i64> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM queue_items WHERE queue_id = ? AND completed_at IS NULL",
                params![queue_id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("Failed to count items: {}", e)))?;

        Ok(count)
    }

    async fn count_items_for_user(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, user_id: &UserId) -> Result<i64> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM queue_items WHERE queue_id = ? AND assigned_to = ? AND completed_at IS NULL",
                params![queue_id.0.to_string(), user_id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("Failed to count items: {}", e)))?;

        Ok(count)
    }
}

// ============================================================================
// Assignment Rule Repository Implementation
// ============================================================================

pub struct AssignmentRuleRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl AssignmentRuleRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    fn map_rule_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<AssignmentRule> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
        let queue_id_str: String = row.get(1).map_err(|e| Error::Database(e.to_string()))?;
        let conditions_json: String = row.get(5).map_err(|e| Error::Database(e.to_string()))?;
        let assign_to_json: String = row.get(6).map_err(|e| Error::Database(e.to_string()))?;

        Ok(AssignmentRule {
            id: AssignmentRuleId(Uuid::parse_str(&id_str).unwrap()),
            tenant_id,
            queue_id: WorkQueueId(Uuid::parse_str(&queue_id_str).unwrap()),
            name: row.get(2).map_err(|e| Error::Database(e.to_string()))?,
            description: row.get(3).ok(),
            priority: row.get(4).unwrap_or(0),
            is_active: row.get::<_, i32>(7).unwrap_or(1) == 1,
            conditions: serde_json::from_str(&conditions_json).unwrap_or_default(),
            assign_to: serde_json::from_str(&assign_to_json)
                .unwrap_or(AssignmentTarget::Role("ap_user".to_string())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
}

#[async_trait]
impl AssignmentRuleRepository for AssignmentRuleRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateAssignmentRuleInput,
    ) -> Result<AssignmentRule> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = AssignmentRuleId::new();
        let now = Utc::now();

        let conditions_json = serde_json::to_string(&input.conditions)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let assign_to_json = serde_json::to_string(&input.assign_to)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;

        conn.execute(
            r#"INSERT INTO assignment_rules (
                id, queue_id, name, description, priority, conditions, assign_to, is_active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            params![
                id.0.to_string(),
                input.queue_id.0.to_string(),
                input.name,
                input.description,
                input.priority,
                conditions_json,
                assign_to_json,
                1,
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to create assignment rule: {}", e)))?;

        Ok(AssignmentRule {
            id,
            tenant_id: tenant_id.clone(),
            queue_id: input.queue_id,
            name: input.name,
            description: input.description,
            priority: input.priority,
            is_active: true,
            conditions: input.conditions,
            assign_to: input.assign_to,
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(
        &self,
        tenant_id: &TenantId,
        id: &AssignmentRuleId,
    ) -> Result<Option<AssignmentRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, name, description, priority, conditions, assign_to, is_active
                   FROM assignment_rules WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row(params![id.0.to_string()], |row| {
                Ok(self.map_rule_row(row, tenant_id.clone()))
            })
            .ok();

        match result {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    async fn list_for_queue(
        &self,
        tenant_id: &TenantId,
        queue_id: &WorkQueueId,
    ) -> Result<Vec<AssignmentRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, name, description, priority, conditions, assign_to, is_active
                   FROM assignment_rules WHERE queue_id = ? AND is_active = 1
                   ORDER BY priority DESC"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let rules = stmt
            .query_map(params![queue_id.0.to_string()], |row| {
                Ok(self.map_rule_row(row, tenant_clone.clone()))
            })
            .map_err(|e| Error::Database(format!("Failed to list rules: {}", e)))?;

        let mut results = Vec::new();
        for rule in rules {
            results.push(rule.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(results)
    }

    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<AssignmentRule>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let mut stmt = conn
            .prepare(
                r#"SELECT id, queue_id, name, description, priority, conditions, assign_to, is_active
                   FROM assignment_rules ORDER BY queue_id, priority DESC"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let rules = stmt
            .query_map([], |row| Ok(self.map_rule_row(row, tenant_clone.clone())))
            .map_err(|e| Error::Database(format!("Failed to list rules: {}", e)))?;

        let mut results = Vec::new();
        for rule in rules {
            results.push(rule.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(results)
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &AssignmentRuleId,
        input: CreateAssignmentRuleInput,
    ) -> Result<AssignmentRule> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let now = Utc::now();
        let conditions_json = serde_json::to_string(&input.conditions)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;
        let assign_to_json = serde_json::to_string(&input.assign_to)
            .map_err(|e| Error::Database(format!("Failed to serialize: {}", e)))?;

        conn.execute(
            r#"UPDATE assignment_rules SET
                queue_id = ?, name = ?, description = ?, priority = ?,
                conditions = ?, assign_to = ?, updated_at = ?
               WHERE id = ?"#,
            params![
                input.queue_id.0.to_string(),
                input.name,
                input.description,
                input.priority,
                conditions_json,
                assign_to_json,
                now.to_rfc3339(),
                id.0.to_string(),
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to update rule: {}", e)))?;

        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "AssignmentRule".to_string(),
                id: id.0.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &AssignmentRuleId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "DELETE FROM assignment_rules WHERE id = ?",
            params![id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete rule: {}", e)))?;

        Ok(())
    }

    async fn set_active(&self, tenant_id: &TenantId, id: &AssignmentRuleId, is_active: bool) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE assignment_rules SET is_active = ?, updated_at = ? WHERE id = ?",
            params![if is_active { 1 } else { 0 }, Utc::now().to_rfc3339(), id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to update rule status: {}", e)))?;

        Ok(())
    }
}
