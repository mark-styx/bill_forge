//! Workflow repository implementation

use async_trait::async_trait;
use billforge_core::{
    domain::{
        WorkflowRule, WorkflowRuleId, CreateWorkflowRuleInput, WorkflowRuleType, RuleCondition, RuleAction,
        WorkQueue, WorkQueueId, CreateWorkQueueInput, QueueType, QueueItem, QueueSettings,
        AssignmentRule, AssignmentRuleId, CreateAssignmentRuleInput, AssignmentCondition, AssignmentTarget,
        ApprovalRequest, ApprovalStatus,
        InvoiceId,
    },
    traits::{WorkflowRuleRepository, WorkQueueRepository, ApprovalRepository, AssignmentRuleRepository},
    types::{Pagination, PaginatedResponse, PaginationMeta},
    UserId, TenantId, Error, Result,
};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct WorkflowRepositoryImpl {
    pool: Arc<PgPool>,
}

impl WorkflowRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WorkflowRuleRepository for WorkflowRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateWorkflowRuleInput,
    ) -> Result<WorkflowRule> {
        let id = WorkflowRuleId::new();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO workflow_rules (
                id, tenant_id, name, description, priority, is_active, rule_type,
                conditions, actions, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.priority)
        .bind(true)
        .bind(format!("{:?}", input.rule_type).to_lowercase())
        .bind(sqlx::types::Json(&input.conditions))
        .bind(sqlx::types::Json(&input.actions))
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
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

    async fn get_by_id(&self, tenant_id: &TenantId, id: &WorkflowRuleId) -> Result<Option<WorkflowRule>> {
        let result = sqlx::query_as::<_, WorkflowRuleRow>(
            "SELECT * FROM workflow_rules WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get workflow rule: {}", e)))?;

        Ok(result.map(|row| row.into_rule(tenant_id)))
    }

    async fn list(&self, tenant_id: &TenantId, rule_type: Option<WorkflowRuleType>) -> Result<Vec<WorkflowRule>> {
        let rows = if let Some(rt) = rule_type {
            sqlx::query_as::<_, WorkflowRuleRow>(
                "SELECT * FROM workflow_rules WHERE tenant_id = $1 AND rule_type = $2 ORDER BY priority DESC"
            )
            .bind(*tenant_id.as_uuid())
            .bind(format!("{:?}", rt).to_lowercase())
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list workflow rules: {}", e)))?
        } else {
            sqlx::query_as::<_, WorkflowRuleRow>(
                "SELECT * FROM workflow_rules WHERE tenant_id = $1 ORDER BY priority DESC"
            )
            .bind(*tenant_id.as_uuid())
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list workflow rules: {}", e)))?
        };

        Ok(rows.into_iter().map(|row| row.into_rule(tenant_id)).collect())
    }

    async fn update(&self, tenant_id: &TenantId, id: &WorkflowRuleId, input: CreateWorkflowRuleInput) -> Result<WorkflowRule> {
        let now = Utc::now();

        sqlx::query(
            r#"UPDATE workflow_rules SET
                name = $1, description = $2, priority = $3, rule_type = $4,
                conditions = $5, actions = $6, updated_at = $7
            WHERE id = $8 AND tenant_id = $9"#
        )
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.priority)
        .bind(format!("{:?}", input.rule_type).to_lowercase())
        .bind(sqlx::types::Json(&input.conditions))
        .bind(sqlx::types::Json(&input.actions))
        .bind(now)
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update workflow rule: {}", e)))?;

        WorkflowRuleRepository::get_by_id(self, tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "WorkflowRule".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &WorkflowRuleId) -> Result<()> {
        sqlx::query("DELETE FROM workflow_rules WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete workflow rule: {}", e)))?;

        Ok(())
    }

    async fn set_active(&self, tenant_id: &TenantId, id: &WorkflowRuleId, is_active: bool) -> Result<()> {
        sqlx::query("UPDATE workflow_rules SET is_active = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
            .bind(is_active)
            .bind(Utc::now())
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to set workflow rule active: {}", e)))?;

        Ok(())
    }

    async fn get_active_rules(&self, tenant_id: &TenantId, rule_type: WorkflowRuleType) -> Result<Vec<WorkflowRule>> {
        let rows = sqlx::query_as::<_, WorkflowRuleRow>(
            "SELECT * FROM workflow_rules WHERE tenant_id = $1 AND rule_type = $2 AND is_active = true ORDER BY priority DESC"
        )
        .bind(*tenant_id.as_uuid())
        .bind(format!("{:?}", rule_type).to_lowercase())
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get active workflow rules: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_rule(tenant_id)).collect())
    }
}

#[async_trait]
impl WorkQueueRepository for WorkflowRepositoryImpl {
    async fn create(&self, tenant_id: &TenantId, input: CreateWorkQueueInput) -> Result<WorkQueue> {
        let id = WorkQueueId::new();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO work_queues (
                id, tenant_id, name, description, queue_type, is_default, is_active, settings, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .bind(&input.name)
        .bind(&input.description)
        .bind(format!("{:?}", input.queue_type).to_lowercase())
        .bind(false) // is_default
        .bind(true)
        .bind(sqlx::types::Json(&serde_json::Value::Object(serde_json::Map::new())))
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create work queue: {}", e)))?;

        Ok(WorkQueue {
            id,
            tenant_id: tenant_id.clone(),
            name: input.name,
            description: input.description,
            queue_type: input.queue_type,
            assigned_users: vec![],
            assigned_roles: vec![],
            is_default: false,
            is_active: true,
            settings: QueueSettings {
                default_sort: "priority_desc".to_string(),
                sla_hours: None,
                escalation_hours: None,
                escalation_user_id: None,
            },
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: &WorkQueueId) -> Result<Option<WorkQueue>> {
        let result = sqlx::query_as::<_, WorkQueueRow>(
            "SELECT * FROM work_queues WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get work queue: {}", e)))?;

        Ok(result.map(|row| row.into_queue(tenant_id)))
    }

    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<WorkQueue>> {
        let rows = sqlx::query_as::<_, WorkQueueRow>(
            "SELECT * FROM work_queues WHERE tenant_id = $1 ORDER BY created_at"
        )
        .bind(*tenant_id.as_uuid())
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list work queues: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_queue(tenant_id)).collect())
    }

    async fn update(&self, tenant_id: &TenantId, id: &WorkQueueId, input: CreateWorkQueueInput) -> Result<WorkQueue> {
        let now = Utc::now();

        sqlx::query(
            r#"UPDATE work_queues SET
                name = $1, description = $2, queue_type = $3, is_default = $4, updated_at = $5
            WHERE id = $6 AND tenant_id = $7"#
        )
        .bind(&input.name)
        .bind(&input.description)
        .bind(format!("{:?}", input.queue_type).to_lowercase())
        .bind(false) // is_default
        .bind(now)
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update work queue: {}", e)))?;

        WorkQueueRepository::get_by_id(self, tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "WorkQueue".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &WorkQueueId) -> Result<()> {
        sqlx::query("DELETE FROM work_queues WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete work queue: {}", e)))?;

        Ok(())
    }

    async fn get_default(&self, tenant_id: &TenantId) -> Result<Option<WorkQueue>> {
        let result = sqlx::query_as::<_, WorkQueueRow>(
            "SELECT * FROM work_queues WHERE tenant_id = $1 AND is_default = true LIMIT 1"
        )
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get default work queue: {}", e)))?;

        Ok(result.map(|row| row.into_queue(tenant_id)))
    }

    async fn get_by_type(&self, tenant_id: &TenantId, queue_type: QueueType) -> Result<Option<WorkQueue>> {
        let result = sqlx::query_as::<_, WorkQueueRow>(
            "SELECT * FROM work_queues WHERE tenant_id = $1 AND queue_type = $2 LIMIT 1"
        )
        .bind(*tenant_id.as_uuid())
        .bind(format!("{:?}", queue_type).to_lowercase())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get work queue by type: {}", e)))?;

        Ok(result.map(|row| row.into_queue(tenant_id)))
    }

    async fn add_item(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, invoice_id: &InvoiceId, assigned_to: Option<&UserId>) -> Result<QueueItem> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO queue_items (id, tenant_id, queue_id, invoice_id, assigned_to, status, entered_at)
               VALUES ($1, $2, $3, $4, $5, 'pending', $6)"#
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .bind(queue_id.0)
        .bind(invoice_id.0)
        .bind(assigned_to.map(|u| u.0))
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to add queue item: {}", e)))?;

        Ok(QueueItem {
            id,
            tenant_id: tenant_id.clone(),
            queue_id: queue_id.clone(),
            invoice_id: invoice_id.clone(),
            assigned_to: assigned_to.cloned(),
            priority: 0,
            entered_at: now,
            due_at: None,
            claimed_at: None,
            completed_at: None,
        })
    }

    async fn get_items(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, pagination: &Pagination) -> Result<PaginatedResponse<QueueItem>> {
        let offset = ((pagination.page - 1) * pagination.per_page) as i32;

        let rows = sqlx::query_as::<_, QueueItemRow>(
            "SELECT * FROM queue_items WHERE tenant_id = $1 AND queue_id = $2 ORDER BY priority DESC, entered_at LIMIT $3 OFFSET $4"
        )
        .bind(*tenant_id.as_uuid())
        .bind(queue_id.0)
        .bind(pagination.per_page as i32)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get queue items: {}", e)))?;

        let items: Vec<QueueItem> = rows
            .into_iter()
            .map(|row| row.into_item())
            .collect();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue_items WHERE tenant_id = $1 AND queue_id = $2")
            .bind(*tenant_id.as_uuid())
            .bind(queue_id.0)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count queue items: {}", e)))?;

        Ok(PaginatedResponse {
            data: items,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn get_items_for_user(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, user_id: &UserId, pagination: &Pagination) -> Result<PaginatedResponse<QueueItem>> {
        let offset = ((pagination.page - 1) * pagination.per_page) as i32;

        let rows = sqlx::query_as::<_, QueueItemRow>(
            "SELECT * FROM queue_items WHERE tenant_id = $1 AND queue_id = $2 AND assigned_to = $3 ORDER BY priority DESC, entered_at LIMIT $4 OFFSET $5"
        )
        .bind(*tenant_id.as_uuid())
        .bind(queue_id.0)
        .bind(user_id.0)
        .bind(pagination.per_page as i32)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get queue items for user: {}", e)))?;

        let items: Vec<QueueItem> = rows
            .into_iter()
            .map(|row| row.into_item())
            .collect();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue_items WHERE tenant_id = $1 AND queue_id = $2 AND assigned_to = $3")
            .bind(*tenant_id.as_uuid())
            .bind(queue_id.0)
            .bind(user_id.0)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count queue items for user: {}", e)))?;

        Ok(PaginatedResponse {
            data: items,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn count_items(&self, tenant_id: &TenantId, queue_id: &WorkQueueId) -> Result<i64> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue_items WHERE tenant_id = $1 AND queue_id = $2")
            .bind(*tenant_id.as_uuid())
            .bind(queue_id.0)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count queue items: {}", e)))?;

        Ok(total)
    }

    async fn count_items_for_user(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, user_id: &UserId) -> Result<i64> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue_items WHERE tenant_id = $1 AND queue_id = $2 AND assigned_to = $3")
            .bind(*tenant_id.as_uuid())
            .bind(queue_id.0)
            .bind(user_id.0)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count queue items for user: {}", e)))?;

        Ok(total)
    }

    async fn move_item(&self, tenant_id: &TenantId, invoice_id: &InvoiceId, queue_id: &WorkQueueId, assigned_to: Option<&UserId>) -> Result<QueueItem> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO queue_items (id, tenant_id, queue_id, invoice_id, assigned_to, status, priority, entered_at)
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)"#
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .bind(queue_id.0)
        .bind(invoice_id.0)
        .bind(assigned_to.map(|u| u.0))
        .bind(0)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to move item to queue: {}", e)))?;

        Ok(QueueItem {
            id,
            tenant_id: tenant_id.clone(),
            queue_id: queue_id.clone(),
            invoice_id: invoice_id.clone(),
            assigned_to: assigned_to.cloned(),
            priority: 0,
            entered_at: now,
            due_at: None,
            claimed_at: None,
            completed_at: None,
        })
    }

    async fn claim_item(&self, tenant_id: &TenantId, item_id: Uuid, user_id: &UserId) -> Result<QueueItem> {
        let now = Utc::now();

        sqlx::query("UPDATE queue_items SET assigned_to = $1, claimed_at = $2 WHERE id = $3")
            .bind(user_id.0)
            .bind(now)
            .bind(item_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to claim item: {}", e)))?;

        let result = sqlx::query_as::<_, QueueItemRow>("SELECT * FROM queue_items WHERE id = $1")
            .bind(item_id)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get claimed item: {}", e)))?;

        Ok(result.into_item())
    }

    async fn complete_item(&self, tenant_id: &TenantId, item_id: Uuid, action: &str) -> Result<()> {
        let now = Utc::now();

        sqlx::query("UPDATE queue_items SET completed_at = $1, completion_action = $2 WHERE id = $3")
            .bind(now)
            .bind(action)
            .bind(item_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to complete item: {}", e)))?;

        Ok(())
    }

    async fn get_current_item_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<Option<QueueItem>> {
        let result = sqlx::query_as::<_, QueueItemRow>(
            r#"SELECT * FROM queue_items
               WHERE tenant_id = $1 AND invoice_id = $2 AND completed_at IS NULL
               ORDER BY entered_at DESC
               LIMIT 1"#
        )
        .bind(*tenant_id.as_uuid())
        .bind(invoice_id.0)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get current queue item: {}", e)))?;

        Ok(result.map(|row| row.into_item()))
    }

    async fn reassign_item(&self, tenant_id: &TenantId, item_id: Uuid, assigned_to: &UserId) -> Result<QueueItem> {
        let now = Utc::now();

        sqlx::query("UPDATE queue_items SET assigned_to = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
            .bind(assigned_to.0)
            .bind(now)
            .bind(item_id)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to reassign item: {}", e)))?;

        let result = sqlx::query_as::<_, QueueItemRow>("SELECT * FROM queue_items WHERE id = $1")
            .bind(item_id)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to get reassigned item: {}", e)))?;

        Ok(result.into_item())
    }
}

#[async_trait]
impl AssignmentRuleRepository for WorkflowRepositoryImpl {
    async fn create(&self, tenant_id: &TenantId, input: CreateAssignmentRuleInput) -> Result<AssignmentRule> {
        let id = AssignmentRuleId::new();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO assignment_rules (
                id, tenant_id, queue_id, name, description, priority, is_active, conditions, assign_to, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .bind(input.queue_id.0)
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.priority)
        .bind(true)
        .bind(sqlx::types::Json(&input.conditions))
        .bind(sqlx::types::Json(&input.assign_to))
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
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

    async fn get_by_id(&self, tenant_id: &TenantId, id: &AssignmentRuleId) -> Result<Option<AssignmentRule>> {
        let result = sqlx::query_as::<_, AssignmentRuleRow>(
            "SELECT * FROM assignment_rules WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get assignment rule: {}", e)))?;

        Ok(result.map(|row| row.into_rule(tenant_id)))
    }

    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<AssignmentRule>> {
        let rows = sqlx::query_as::<_, AssignmentRuleRow>(
            "SELECT * FROM assignment_rules WHERE tenant_id = $1 ORDER BY priority DESC"
        )
        .bind(*tenant_id.as_uuid())
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list assignment rules: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_rule(tenant_id)).collect())
    }

    async fn update(&self, tenant_id: &TenantId, id: &AssignmentRuleId, input: CreateAssignmentRuleInput) -> Result<AssignmentRule> {
        let now = Utc::now();

        sqlx::query(
            r#"UPDATE assignment_rules SET
                queue_id = $1, name = $2, description = $3, priority = $4,
                conditions = $5, assign_to = $6, updated_at = $7
            WHERE id = $8 AND tenant_id = $9"#
        )
        .bind(input.queue_id.0)
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.priority)
        .bind(sqlx::types::Json(&input.conditions))
        .bind(sqlx::types::Json(&input.assign_to))
        .bind(now)
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update assignment rule: {}", e)))?;

        AssignmentRuleRepository::get_by_id(self, tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "AssignmentRule".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &AssignmentRuleId) -> Result<()> {
        sqlx::query("DELETE FROM assignment_rules WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete assignment rule: {}", e)))?;

        Ok(())
    }

    async fn list_for_queue(&self, tenant_id: &TenantId, queue_id: &WorkQueueId) -> Result<Vec<AssignmentRule>> {
        let rows = sqlx::query_as::<_, AssignmentRuleRow>(
            "SELECT * FROM assignment_rules WHERE tenant_id = $1 AND queue_id = $2 ORDER BY priority DESC"
        )
        .bind(*tenant_id.as_uuid())
        .bind(queue_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list assignment rules for queue: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_rule(tenant_id)).collect())
    }

    async fn set_active(&self, tenant_id: &TenantId, id: &AssignmentRuleId, is_active: bool) -> Result<()> {
        sqlx::query("UPDATE assignment_rules SET is_active = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
            .bind(is_active)
            .bind(Utc::now())
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to set assignment rule active: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl ApprovalRepository for WorkflowRepositoryImpl {
    async fn create(&self, tenant_id: &TenantId, request: ApprovalRequest) -> Result<ApprovalRequest> {
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO approval_requests (
                id, tenant_id, invoice_id, rule_id, requested_from, status,
                expires_at, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(request.id)
        .bind(*tenant_id.as_uuid())
        .bind(request.invoice_id.0)
        .bind(request.rule_id.0)
        .bind(sqlx::types::Json(&request.requested_from))
        .bind(match request.status {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
            ApprovalStatus::Cancelled => "cancelled",
        })
        .bind(request.expires_at)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create approval request: {}", e)))?;

        Ok(request)
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let result = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT * FROM approval_requests WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get approval request: {}", e)))?;

        Ok(result.map(|row| row.into_approval_request(tenant_id)))
    }

    async fn list_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<Vec<ApprovalRequest>> {
        let rows = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT * FROM approval_requests WHERE tenant_id = $1 AND invoice_id = $2 ORDER BY created_at DESC"
        )
        .bind(*tenant_id.as_uuid())
        .bind(invoice_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list approval requests: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_approval_request(tenant_id)).collect())
    }

    async fn list_pending_for_user(&self, tenant_id: &TenantId, user_id: &UserId) -> Result<Vec<ApprovalRequest>> {
        let rows = sqlx::query_as::<_, ApprovalRequestRow>(
            r#"SELECT * FROM approval_requests
               WHERE tenant_id = $1
               AND requested_from->>'User' = $2
               AND status = 'pending'
               ORDER BY created_at DESC"#
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get pending approvals: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_approval_request(tenant_id)).collect())
    }

    async fn respond(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
        status: ApprovalStatus,
        comments: Option<String>,
        user_id: &UserId,
    ) -> Result<ApprovalRequest> {
        let now = Utc::now();

        sqlx::query(
            r#"UPDATE approval_requests
               SET status = $1, comments = $2, responded_by = $3, responded_at = $4, updated_at = $5
               WHERE id = $6 AND tenant_id = $7"#
        )
        .bind(match status {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
            ApprovalStatus::Cancelled => "cancelled",
        })
        .bind(comments)
        .bind(user_id.0)
        .bind(now)
        .bind(now)
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to respond to approval: {}", e)))?;

        ApprovalRepository::get_by_id(self, tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "ApprovalRequest".to_string(),
                id: id.to_string(),
            })
    }

    async fn cancel_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<()> {
        sqlx::query(
            "UPDATE approval_requests SET status = 'cancelled', updated_at = NOW() WHERE tenant_id = $1 AND invoice_id = $2 AND status = 'pending'"
        )
        .bind(*tenant_id.as_uuid())
        .bind(invoice_id.0)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to cancel approval requests: {}", e)))?;

        Ok(())
    }
}

// Helper structs for mapping database rows

#[derive(sqlx::FromRow)]
struct WorkflowRuleRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    description: Option<String>,
    priority: i32,
    is_active: bool,
    rule_type: String,
    conditions: sqlx::types::Json<Vec<RuleCondition>>,
    actions: sqlx::types::Json<Vec<RuleAction>>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl WorkflowRuleRow {
    fn into_rule(self, tenant_id: &TenantId) -> WorkflowRule {
        WorkflowRule {
            id: WorkflowRuleId(self.id),
            tenant_id: tenant_id.clone(),
            name: self.name,
            description: self.description,
            priority: self.priority,
            is_active: self.is_active,
            rule_type: match self.rule_type.as_str() {
                "approval" => WorkflowRuleType::Approval,
                "routing" => WorkflowRuleType::Routing,
                _ => WorkflowRuleType::AutoApproval,
            },
            conditions: self.conditions.0,
            actions: self.actions.0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorkQueueRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    description: Option<String>,
    queue_type: String,
    is_default: bool,
    is_active: bool,
    settings: sqlx::types::Json<serde_json::Value>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl WorkQueueRow {
    fn into_queue(self, tenant_id: &TenantId) -> WorkQueue {
        WorkQueue {
            id: WorkQueueId(self.id),
            tenant_id: tenant_id.clone(),
            name: self.name,
            description: self.description,
            queue_type: match self.queue_type.as_str() {
                "review" => QueueType::Review,
                "approval" => QueueType::Approval,
                "exception" => QueueType::Exception,
                "payment" => QueueType::Payment,
                "custom" => QueueType::Custom,
                _ => QueueType::Review,
            },
            assigned_users: vec![],
            assigned_roles: vec![],
            is_default: self.is_default,
            is_active: self.is_active,
            settings: serde_json::from_value(self.settings.0).unwrap_or(QueueSettings {
                default_sort: "priority_desc".to_string(),
                sla_hours: None,
                escalation_hours: None,
                escalation_user_id: None,
            }),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct QueueItemRow {
    id: Uuid,
    tenant_id: Uuid,
    queue_id: Uuid,
    invoice_id: Uuid,
    assigned_to: Option<Uuid>,
    priority: i32,
    entered_at: chrono::DateTime<Utc>,
    due_at: Option<chrono::DateTime<Utc>>,
    claimed_at: Option<chrono::DateTime<Utc>>,
    completed_at: Option<chrono::DateTime<Utc>>,
    completion_action: Option<String>,
    notes: Option<String>,
}

impl QueueItemRow {
    fn into_item(self) -> QueueItem {
        QueueItem {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            queue_id: WorkQueueId(self.queue_id),
            invoice_id: InvoiceId(self.invoice_id),
            assigned_to: self.assigned_to.map(UserId),
            priority: self.priority,
            entered_at: self.entered_at,
            due_at: self.due_at,
            claimed_at: self.claimed_at,
            completed_at: self.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AssignmentRuleRow {
    id: Uuid,
    tenant_id: Uuid,
    queue_id: Uuid,
    name: String,
    description: Option<String>,
    priority: i32,
    is_active: bool,
    conditions: sqlx::types::Json<Vec<AssignmentCondition>>,
    assign_to: sqlx::types::Json<AssignmentTarget>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl AssignmentRuleRow {
    fn into_rule(self, tenant_id: &TenantId) -> AssignmentRule {
        AssignmentRule {
            id: AssignmentRuleId(self.id),
            tenant_id: tenant_id.clone(),
            queue_id: WorkQueueId(self.queue_id),
            name: self.name,
            description: self.description,
            priority: self.priority,
            is_active: self.is_active,
            conditions: self.conditions.0,
            assign_to: self.assign_to.0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ApprovalRequestRow {
    id: Uuid,
    tenant_id: Uuid,
    invoice_id: Uuid,
    requested_from: Uuid,
    status: String,
    comments: Option<String>,
    responded_by: Option<Uuid>,
    responded_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
}

impl ApprovalRequestRow {
    fn into_approval_request(self, tenant_id: &TenantId) -> ApprovalRequest {
        ApprovalRequest {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            invoice_id: InvoiceId(self.invoice_id),
            rule_id: WorkflowRuleId(Uuid::nil()),
            requested_from: billforge_core::domain::ApprovalTarget::User(UserId(self.requested_from)),
            status: match self.status.as_str() {
                "approved" => ApprovalStatus::Approved,
                "rejected" => ApprovalStatus::Rejected,
                _ => ApprovalStatus::Pending,
            },
            expires_at: None,
            comments: self.comments,
            responded_by: self.responded_by.map(UserId),
            responded_at: self.responded_at,
            created_at: self.created_at,
        }
    }

    fn into_request(self) -> ApprovalRequest {
        ApprovalRequest {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            invoice_id: InvoiceId(self.invoice_id),
            rule_id: WorkflowRuleId(Uuid::nil()),
            requested_from: billforge_core::domain::ApprovalTarget::User(UserId(self.requested_from)),
            status: match self.status.as_str() {
                "approved" => ApprovalStatus::Approved,
                "rejected" => ApprovalStatus::Rejected,
                _ => ApprovalStatus::Pending,
            },
            comments: self.comments,
            responded_by: self.responded_by.map(UserId),
            responded_at: self.responded_at,
            expires_at: None,
            created_at: self.created_at,
        }
    }
}
