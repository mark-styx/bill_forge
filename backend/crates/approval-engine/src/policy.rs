//! Approval policy management
//!
//! CRUD operations for approval policies and their chain levels.
//! Uses runtime sqlx queries (not compile-time macros).

use crate::error::ApprovalError;
use crate::types::*;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Service for managing approval policies and their chain levels
pub struct PolicyService {
    pool: PgPool,
}

impl PolicyService {
    /// Create a new policy service
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a policy with its chain levels in a single transaction
    pub async fn create_policy(
        &self,
        tenant_id: &Uuid,
        input: CreatePolicyInput,
    ) -> Result<ApprovalPolicy, ApprovalError> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let policy_id = Uuid::new_v4();

        let match_criteria = input
            .match_criteria
            .unwrap_or_else(|| serde_json::json!({}));

        let row = sqlx::query(
            r#"
            INSERT INTO approval_policies (
                id, tenant_id, name, description, is_active, is_default,
                match_criteria, priority, require_sequential, require_all_levels,
                allow_self_approval, auto_approve_below_cents, escalation_enabled,
                escalation_timeout_hours, final_escalation_user_id,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, true, false,
                $5, $6, $7, $8,
                $9, $10, $11,
                $12, $13,
                $14, $15
            )
            RETURNING id, tenant_id, name, description, is_active, is_default,
                      match_criteria, priority, require_sequential, require_all_levels,
                      allow_self_approval, auto_approve_below_cents, escalation_enabled,
                      escalation_timeout_hours, final_escalation_user_id,
                      created_at, updated_at
            "#,
        )
        .bind(policy_id)
        .bind(tenant_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&match_criteria)
        .bind(input.priority.unwrap_or(0))
        .bind(input.require_sequential.unwrap_or(true))
        .bind(input.require_all_levels.unwrap_or(true))
        .bind(input.allow_self_approval.unwrap_or(false))
        .bind(input.auto_approve_below_cents)
        .bind(input.escalation_enabled.unwrap_or(false))
        .bind(input.escalation_timeout_hours)
        .bind(input.final_escalation_user_id)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        let policy = row_to_policy(&row);

        // Insert chain levels in order
        for (idx, level_input) in input.levels.iter().enumerate() {
            let level_id = Uuid::new_v4();
            let approver_user_ids =
                serde_json::to_value(level_input.approver_user_ids.as_deref().unwrap_or(&[]))
                    .unwrap_or_else(|_| serde_json::json!([]));

            sqlx::query(
                r#"
                INSERT INTO approval_chain_levels (
                    id, policy_id, tenant_id, level_order, name,
                    approver_type, approver_user_ids, approver_role,
                    min_amount_cents, max_amount_cents, required_approver_count,
                    timeout_hours, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
            )
            .bind(level_id)
            .bind(policy_id)
            .bind(tenant_id)
            .bind((idx as i32) + 1)
            .bind(&level_input.name)
            .bind(&level_input.approver_type)
            .bind(&approver_user_ids)
            .bind(&level_input.approver_role)
            .bind(level_input.min_amount_cents.unwrap_or(0))
            .bind(level_input.max_amount_cents)
            .bind(level_input.required_approver_count.unwrap_or(1))
            .bind(level_input.timeout_hours)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        tracing::info!(policy_id = %policy_id, name = %input.name, levels = input.levels.len(), "Approval policy created");

        Ok(policy)
    }

    /// Get a policy by ID
    pub async fn get_policy(
        &self,
        tenant_id: &Uuid,
        policy_id: &Uuid,
    ) -> Result<Option<ApprovalPolicy>, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, name, description, is_active, is_default,
                   match_criteria, priority, require_sequential, require_all_levels,
                   allow_self_approval, auto_approve_below_cents, escalation_enabled,
                   escalation_timeout_hours, final_escalation_user_id,
                   created_at, updated_at
            FROM approval_policies
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(policy_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_policy))
    }

    /// List all policies for a tenant, ordered by priority descending
    pub async fn list_policies(
        &self,
        tenant_id: &Uuid,
    ) -> Result<Vec<ApprovalPolicy>, ApprovalError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, name, description, is_active, is_default,
                   match_criteria, priority, require_sequential, require_all_levels,
                   allow_self_approval, auto_approve_below_cents, escalation_enabled,
                   escalation_timeout_hours, final_escalation_user_id,
                   created_at, updated_at
            FROM approval_policies
            WHERE tenant_id = $1
            ORDER BY priority DESC, name ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_policy).collect())
    }

    /// Update a policy with a partial JSON patch
    pub async fn update_policy(
        &self,
        tenant_id: &Uuid,
        policy_id: &Uuid,
        input: serde_json::Value,
    ) -> Result<ApprovalPolicy, ApprovalError> {
        // Verify policy exists
        self.get_policy(tenant_id, policy_id)
            .await?
            .ok_or_else(|| ApprovalError::PolicyNotFound(policy_id.to_string()))?;

        let now = Utc::now();

        // Build dynamic SET clause from provided fields
        let mut sets: Vec<String> = Vec::new();
        let mut param_idx: u32 = 3; // $1=policy_id, $2=tenant_id start at $3

        // We'll collect bind values as serde_json::Values and apply them dynamically.
        // For simplicity, update known fields individually.

        let obj = input.as_object().ok_or_else(|| {
            ApprovalError::Internal("Update input must be a JSON object".to_string())
        })?;

        // Build the update query dynamically
        let mut query_str = String::from("UPDATE approval_policies SET updated_at = $3");
        let mut binds: Vec<serde_json::Value> = vec![
            serde_json::json!(policy_id),
            serde_json::json!(tenant_id),
            serde_json::json!(now.to_rfc3339()),
        ];
        param_idx = 4;

        let updatable_text_fields = ["name", "description"];
        let updatable_bool_fields = [
            "is_active",
            "is_default",
            "require_sequential",
            "require_all_levels",
            "allow_self_approval",
            "escalation_enabled",
        ];
        let updatable_int_fields = ["priority", "escalation_timeout_hours"];

        for field in &updatable_text_fields {
            if let Some(val) = obj.get(*field) {
                sets.push(format!(", {} = ${}", field, param_idx));
                binds.push(val.clone());
                param_idx += 1;
            }
        }
        for field in &updatable_bool_fields {
            if let Some(val) = obj.get(*field) {
                sets.push(format!(", {} = ${}", field, param_idx));
                binds.push(val.clone());
                param_idx += 1;
            }
        }
        for field in &updatable_int_fields {
            if let Some(val) = obj.get(*field) {
                sets.push(format!(", {} = ${}", field, param_idx));
                binds.push(val.clone());
                param_idx += 1;
            }
        }
        if let Some(val) = obj.get("match_criteria") {
            sets.push(format!(", match_criteria = ${}", param_idx));
            binds.push(val.clone());
            param_idx += 1;
        }
        if let Some(val) = obj.get("auto_approve_below_cents") {
            sets.push(format!(", auto_approve_below_cents = ${}", param_idx));
            binds.push(val.clone());
            param_idx += 1;
        }
        if let Some(val) = obj.get("final_escalation_user_id") {
            sets.push(format!(", final_escalation_user_id = ${}", param_idx));
            binds.push(val.clone());
            let _ = param_idx; // last one
        }

        // Use a simpler approach: read-modify-write with known fields
        // This avoids the complexity of dynamic SQL with mixed types
        drop(query_str);
        drop(sets);
        drop(binds);

        let existing = self
            .get_policy(tenant_id, policy_id)
            .await?
            .ok_or_else(|| ApprovalError::PolicyNotFound(policy_id.to_string()))?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&existing.name);
        let description = obj
            .get("description")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or(existing.description.clone());
        let is_active = obj
            .get("is_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.is_active);
        let is_default = obj
            .get("is_default")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.is_default);
        let match_criteria = obj
            .get("match_criteria")
            .cloned()
            .unwrap_or(existing.match_criteria.clone());
        let priority = obj
            .get("priority")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(existing.priority);
        let require_sequential = obj
            .get("require_sequential")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.require_sequential);
        let require_all_levels = obj
            .get("require_all_levels")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.require_all_levels);
        let allow_self_approval = obj
            .get("allow_self_approval")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.allow_self_approval);
        let auto_approve_below_cents = if obj.contains_key("auto_approve_below_cents") {
            obj.get("auto_approve_below_cents").and_then(|v| v.as_i64())
        } else {
            existing.auto_approve_below_cents
        };
        let escalation_enabled = obj
            .get("escalation_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.escalation_enabled);
        let escalation_timeout_hours = if obj.contains_key("escalation_timeout_hours") {
            obj.get("escalation_timeout_hours")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
        } else {
            existing.escalation_timeout_hours
        };
        let final_escalation_user_id = if obj.contains_key("final_escalation_user_id") {
            obj.get("final_escalation_user_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        } else {
            existing.final_escalation_user_id
        };

        let row = sqlx::query(
            r#"
            UPDATE approval_policies SET
                name = $3, description = $4, is_active = $5, is_default = $6,
                match_criteria = $7, priority = $8, require_sequential = $9,
                require_all_levels = $10, allow_self_approval = $11,
                auto_approve_below_cents = $12, escalation_enabled = $13,
                escalation_timeout_hours = $14, final_escalation_user_id = $15,
                updated_at = $16
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, tenant_id, name, description, is_active, is_default,
                      match_criteria, priority, require_sequential, require_all_levels,
                      allow_self_approval, auto_approve_below_cents, escalation_enabled,
                      escalation_timeout_hours, final_escalation_user_id,
                      created_at, updated_at
            "#,
        )
        .bind(policy_id)
        .bind(tenant_id)
        .bind(name)
        .bind(&description)
        .bind(is_active)
        .bind(is_default)
        .bind(&match_criteria)
        .bind(priority)
        .bind(require_sequential)
        .bind(require_all_levels)
        .bind(allow_self_approval)
        .bind(auto_approve_below_cents)
        .bind(escalation_enabled)
        .bind(escalation_timeout_hours)
        .bind(final_escalation_user_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(row_to_policy(&row))
    }

    /// Delete a policy and its chain levels
    pub async fn delete_policy(
        &self,
        tenant_id: &Uuid,
        policy_id: &Uuid,
    ) -> Result<(), ApprovalError> {
        let mut tx = self.pool.begin().await?;

        // Delete chain levels first (FK constraint)
        sqlx::query("DELETE FROM approval_chain_levels WHERE policy_id = $1 AND tenant_id = $2")
            .bind(policy_id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM approval_policies WHERE id = $1 AND tenant_id = $2")
            .bind(policy_id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ApprovalError::PolicyNotFound(policy_id.to_string()));
        }

        tx.commit().await?;

        tracing::info!(policy_id = %policy_id, "Approval policy deleted");

        Ok(())
    }

    /// Get chain levels for a policy, ordered by level_order
    pub async fn get_levels(
        &self,
        policy_id: &Uuid,
    ) -> Result<Vec<ApprovalChainLevel>, ApprovalError> {
        let rows = sqlx::query(
            r#"
            SELECT id, policy_id, tenant_id, level_order, name,
                   approver_type, approver_user_ids, approver_role,
                   min_amount_cents, max_amount_cents, required_approver_count,
                   timeout_hours, created_at, updated_at
            FROM approval_chain_levels
            WHERE policy_id = $1
            ORDER BY level_order ASC
            "#,
        )
        .bind(policy_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_chain_level).collect())
    }

    /// Find the best matching policy for an invoice based on amount, department, vendor.
    ///
    /// Evaluates active policies in priority order (highest first).
    /// match_criteria JSON can contain:
    ///   - `min_amount_cents`: minimum invoice amount
    ///   - `max_amount_cents`: maximum invoice amount
    ///   - `departments`: array of department strings
    ///   - `vendor_ids`: array of vendor UUIDs
    ///
    /// An empty match_criteria `{}` matches all invoices (catch-all / default).
    pub async fn find_matching_policy(
        &self,
        tenant_id: &Uuid,
        amount_cents: i64,
        department: Option<&str>,
        vendor_id: Option<&Uuid>,
    ) -> Result<Option<ApprovalPolicy>, ApprovalError> {
        let policies = sqlx::query(
            r#"
            SELECT id, tenant_id, name, description, is_active, is_default,
                   match_criteria, priority, require_sequential, require_all_levels,
                   allow_self_approval, auto_approve_below_cents, escalation_enabled,
                   escalation_timeout_hours, final_escalation_user_id,
                   created_at, updated_at
            FROM approval_policies
            WHERE tenant_id = $1 AND is_active = true
            ORDER BY priority DESC, created_at ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        for row in &policies {
            let policy = row_to_policy(row);

            if matches_criteria(&policy.match_criteria, amount_cents, department, vendor_id) {
                return Ok(Some(policy));
            }
        }

        Ok(None)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Criteria Matching
// ══════════════════════════════════════════════════════════════════════

/// Check if an invoice matches a policy's criteria.
fn matches_criteria(
    criteria: &serde_json::Value,
    amount_cents: i64,
    department: Option<&str>,
    vendor_id: Option<&Uuid>,
) -> bool {
    let obj = match criteria.as_object() {
        Some(o) => o,
        None => return true, // non-object criteria matches everything
    };

    // Empty criteria = catch-all
    if obj.is_empty() {
        return true;
    }

    // Check min_amount_cents
    if let Some(min) = obj.get("min_amount_cents").and_then(|v| v.as_i64()) {
        if amount_cents < min {
            return false;
        }
    }

    // Check max_amount_cents
    if let Some(max) = obj.get("max_amount_cents").and_then(|v| v.as_i64()) {
        if amount_cents > max {
            return false;
        }
    }

    // Check departments
    if let Some(departments) = obj.get("departments").and_then(|v| v.as_array()) {
        if !departments.is_empty() {
            match department {
                Some(dept) => {
                    let dept_match = departments
                        .iter()
                        .any(|d| d.as_str().map_or(false, |s| s == dept));
                    if !dept_match {
                        return false;
                    }
                }
                None => return false,
            }
        }
    }

    // Check vendor_ids
    if let Some(vendor_ids) = obj.get("vendor_ids").and_then(|v| v.as_array()) {
        if !vendor_ids.is_empty() {
            match vendor_id {
                Some(vid) => {
                    let vid_str = vid.to_string();
                    let vendor_match = vendor_ids
                        .iter()
                        .any(|v| v.as_str().map_or(false, |s| s == vid_str));
                    if !vendor_match {
                        return false;
                    }
                }
                None => return false,
            }
        }
    }

    true
}

// ══════════════════════════════════════════════════════════════════════
// Row Mapping
// ══════════════════════════════════════════════════════════════════════

fn row_to_policy(row: &PgRow) -> ApprovalPolicy {
    ApprovalPolicy {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        name: row.get("name"),
        description: row.get("description"),
        is_active: row.get("is_active"),
        is_default: row.get("is_default"),
        match_criteria: row.get("match_criteria"),
        priority: row.get("priority"),
        require_sequential: row.get("require_sequential"),
        require_all_levels: row.get("require_all_levels"),
        allow_self_approval: row.get("allow_self_approval"),
        auto_approve_below_cents: row.get("auto_approve_below_cents"),
        escalation_enabled: row.get("escalation_enabled"),
        escalation_timeout_hours: row.get("escalation_timeout_hours"),
        final_escalation_user_id: row.get("final_escalation_user_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_chain_level(row: &PgRow) -> ApprovalChainLevel {
    ApprovalChainLevel {
        id: row.get("id"),
        policy_id: row.get("policy_id"),
        tenant_id: row.get("tenant_id"),
        level_order: row.get("level_order"),
        name: row.get("name"),
        approver_type: row.get("approver_type"),
        approver_user_ids: row.get("approver_user_ids"),
        approver_role: row.get("approver_role"),
        min_amount_cents: row.get("min_amount_cents"),
        max_amount_cents: row.get("max_amount_cents"),
        required_approver_count: row.get("required_approver_count"),
        timeout_hours: row.get("timeout_hours"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
