//! Core approval engine
//!
//! Orchestrates multi-level approval chains:
//! - Submit invoices for approval (policy matching → chain creation → step assignment)
//! - Approve/reject steps (advance chain or terminate)
//! - Delegate steps to other users
//! - Escalate overdue steps
//! - Recall/cancel chains
//!
//! Uses runtime sqlx queries (not compile-time macros).

use crate::error::ApprovalError;
use crate::policy::PolicyService;
use crate::types::*;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// The core approval engine
pub struct ApprovalEngine {
    pool: PgPool,
    policy_service: PolicyService,
}

impl ApprovalEngine {
    /// Create a new approval engine
    pub fn new(pool: PgPool) -> Self {
        let policy_service = PolicyService::new(pool.clone());
        Self {
            pool,
            policy_service,
        }
    }

    /// Access the underlying policy service
    pub fn policy_service(&self) -> &PolicyService {
        &self.policy_service
    }

    // ══════════════════════════════════════════════════════════════════════
    // Submit for Approval
    // ══════════════════════════════════════════════════════════════════════

    /// Submit an invoice for approval.
    ///
    /// 1. Finds the best matching policy by priority
    /// 2. Checks auto-approve threshold
    /// 3. Filters applicable chain levels by amount
    /// 4. Creates the active approval chain
    /// 5. Creates steps for the first level (or all levels if not sequential)
    /// 6. Logs the submission activity
    pub async fn submit_for_approval(
        &self,
        tenant_id: &Uuid,
        invoice_id: &Uuid,
        amount_cents: i64,
        department: Option<&str>,
        vendor_id: Option<&Uuid>,
        initiated_by: &Uuid,
    ) -> Result<ActiveApprovalChain, ApprovalError> {
        // 1. Find matching policy
        let policy = self
            .policy_service
            .find_matching_policy(tenant_id, amount_cents, department, vendor_id)
            .await?
            .ok_or_else(|| {
                ApprovalError::PolicyNotFound("No matching approval policy found".to_string())
            })?;

        // 2. Check auto-approve threshold
        if let Some(threshold) = policy.auto_approve_below_cents {
            if amount_cents < threshold {
                return self
                    .create_auto_approved_chain(tenant_id, invoice_id, &policy, initiated_by)
                    .await;
            }
        }

        // 3. Get applicable levels filtered by amount thresholds
        let all_levels = self.policy_service.get_levels(&policy.id).await?;
        let applicable_levels: Vec<_> = all_levels
            .into_iter()
            .filter(|l| {
                amount_cents >= l.min_amount_cents
                    && l.max_amount_cents
                        .map_or(true, |max| amount_cents <= max)
            })
            .collect();

        // No applicable levels = auto-approve
        if applicable_levels.is_empty() {
            return self
                .create_auto_approved_chain(tenant_id, invoice_id, &policy, initiated_by)
                .await;
        }

        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let chain_id = Uuid::new_v4();
        let total_levels = applicable_levels.len() as i32;

        // 4. Create the active approval chain
        let chain_row = sqlx::query(
            r#"
            INSERT INTO active_approval_chains (
                id, tenant_id, invoice_id, policy_id, status,
                current_level, total_levels, escalation_count,
                initiated_by, initiated_at,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id, tenant_id, invoice_id, policy_id, status,
                      current_level, total_levels, final_decision,
                      final_decided_by, final_decided_at, escalation_count,
                      last_escalated_at, initiated_by, initiated_at,
                      completed_at, created_at, updated_at
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .bind(invoice_id)
        .bind(policy.id)
        .bind("in_progress")
        .bind(1i32) // start at level 1
        .bind(total_levels)
        .bind(0i32) // escalation_count
        .bind(initiated_by)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        // 5. Create steps — for sequential: only first level; for parallel: all levels
        if policy.require_sequential {
            self.create_steps_for_level(&mut tx, tenant_id, chain_id, &applicable_levels[0], &policy, initiated_by)
                .await?;
        } else {
            for level in &applicable_levels {
                self.create_steps_for_level(&mut tx, tenant_id, chain_id, level, &policy, initiated_by)
                    .await?;
            }
        }

        // 6. Log activity
        self.log_activity_tx(
            &mut tx,
            tenant_id,
            &chain_id,
            None,
            invoice_id,
            "submitted",
            initiated_by,
            None,
            Some(serde_json::json!({
                "policy_id": policy.id,
                "policy_name": policy.name,
                "amount_cents": amount_cents,
                "total_levels": total_levels,
            })),
        )
        .await?;

        tx.commit().await?;

        let chain = row_to_chain(&chain_row);
        tracing::info!(
            chain_id = %chain_id,
            invoice_id = %invoice_id,
            policy = %policy.name,
            levels = total_levels,
            "Invoice submitted for approval"
        );

        Ok(chain)
    }

    // ══════════════════════════════════════════════════════════════════════
    // Approve / Reject
    // ══════════════════════════════════════════════════════════════════════

    /// Approve a step. If all required approvers at the current level have approved,
    /// advances to the next level or completes the chain.
    pub async fn approve_step(
        &self,
        tenant_id: &Uuid,
        step_id: &Uuid,
        approver_id: &Uuid,
        comments: Option<&str>,
    ) -> Result<ApprovalChainStep, ApprovalError> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        // 1. Get the step
        let step = self
            .get_step_tx(&mut tx, tenant_id, step_id)
            .await?
            .ok_or_else(|| ApprovalError::StepNotFound(step_id.to_string()))?;

        // Verify step is pending
        if step.status != "pending" {
            return Err(ApprovalError::InvalidTransition {
                current: step.status.clone(),
                requested: "approved".to_string(),
            });
        }

        // Verify the approver is assigned (or delegated)
        if step.assigned_to != *approver_id {
            return Err(ApprovalError::NotAuthorized(format!(
                "User {} is not assigned to step {}",
                approver_id, step_id
            )));
        }

        // 2. Get the chain to check self-approval policy
        let chain = self
            .get_chain_tx(&mut tx, tenant_id, &step.chain_id)
            .await?
            .ok_or_else(|| ApprovalError::ChainNotFound(step.chain_id.to_string()))?;

        if chain.status != "in_progress" {
            return Err(ApprovalError::AlreadyCompleted);
        }

        let policy = self
            .policy_service
            .get_policy(tenant_id, &chain.policy_id)
            .await?
            .ok_or_else(|| ApprovalError::PolicyNotFound(chain.policy_id.to_string()))?;

        // Check self-approval
        if !policy.allow_self_approval && chain.initiated_by == *approver_id {
            return Err(ApprovalError::SelfApprovalNotAllowed);
        }

        // 3. Update step
        let step_row = sqlx::query(
            r#"
            UPDATE approval_chain_steps
            SET status = 'approved', decision = 'approved', comments = $3,
                responded_at = $4, updated_at = $4
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, chain_id, tenant_id, level_id, level_order,
                      assigned_to, status, decision, comments,
                      delegated_to, delegated_at, delegation_reason,
                      assigned_at, due_at, responded_at,
                      escalated_at, escalated_to, escalation_reason,
                      created_at, updated_at
            "#,
        )
        .bind(step_id)
        .bind(tenant_id)
        .bind(comments)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        let updated_step = row_to_step(&step_row);

        // 4. Log activity
        self.log_activity_tx(
            &mut tx,
            tenant_id,
            &chain.id,
            Some(step_id),
            &chain.invoice_id,
            "approved",
            approver_id,
            comments,
            None,
        )
        .await?;

        // 5. Check if all required approvers at this level have approved
        let level = self.get_level_for_step(&step).await?;
        let approved_count = self
            .count_approved_at_level(&mut tx, &chain.id, step.level_order)
            .await?;

        if approved_count >= level.required_approver_count as i64 {
            // This level is complete — advance or finish
            if chain.current_level >= chain.total_levels {
                // Final level done — approve the chain
                self.complete_chain(&mut tx, tenant_id, &chain.id, "approved", approver_id)
                    .await?;
            } else if policy.require_sequential {
                // Advance to the next level
                self.advance_to_next_level_tx(&mut tx, tenant_id, &chain, &policy)
                    .await?;
            } else {
                // Parallel mode: check if all levels are done
                let all_done = self
                    .all_levels_completed(&mut tx, &chain.id, chain.total_levels)
                    .await?;
                if all_done {
                    self.complete_chain(&mut tx, tenant_id, &chain.id, "approved", approver_id)
                        .await?;
                }
            }
        }

        tx.commit().await?;

        tracing::info!(
            step_id = %step_id,
            chain_id = %chain.id,
            approver = %approver_id,
            "Approval step approved"
        );

        Ok(updated_step)
    }

    /// Reject a step — immediately rejects the entire chain.
    pub async fn reject_step(
        &self,
        tenant_id: &Uuid,
        step_id: &Uuid,
        rejector_id: &Uuid,
        comments: Option<&str>,
    ) -> Result<ApprovalChainStep, ApprovalError> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        // 1. Get and validate the step
        let step = self
            .get_step_tx(&mut tx, tenant_id, step_id)
            .await?
            .ok_or_else(|| ApprovalError::StepNotFound(step_id.to_string()))?;

        if step.status != "pending" {
            return Err(ApprovalError::InvalidTransition {
                current: step.status.clone(),
                requested: "rejected".to_string(),
            });
        }

        if step.assigned_to != *rejector_id {
            return Err(ApprovalError::NotAuthorized(format!(
                "User {} is not assigned to step {}",
                rejector_id, step_id
            )));
        }

        let chain = self
            .get_chain_tx(&mut tx, tenant_id, &step.chain_id)
            .await?
            .ok_or_else(|| ApprovalError::ChainNotFound(step.chain_id.to_string()))?;

        if chain.status != "in_progress" {
            return Err(ApprovalError::AlreadyCompleted);
        }

        // 2. Reject the step
        let step_row = sqlx::query(
            r#"
            UPDATE approval_chain_steps
            SET status = 'rejected', decision = 'rejected', comments = $3,
                responded_at = $4, updated_at = $4
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, chain_id, tenant_id, level_id, level_order,
                      assigned_to, status, decision, comments,
                      delegated_to, delegated_at, delegation_reason,
                      assigned_at, due_at, responded_at,
                      escalated_at, escalated_to, escalation_reason,
                      created_at, updated_at
            "#,
        )
        .bind(step_id)
        .bind(tenant_id)
        .bind(comments)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        let updated_step = row_to_step(&step_row);

        // 3. Reject the entire chain
        self.complete_chain(&mut tx, tenant_id, &chain.id, "rejected", rejector_id)
            .await?;

        // 4. Cancel remaining pending steps
        self.cancel_pending_steps(&mut tx, &chain.id).await?;

        // 5. Log activity
        self.log_activity_tx(
            &mut tx,
            tenant_id,
            &chain.id,
            Some(step_id),
            &chain.invoice_id,
            "rejected",
            rejector_id,
            comments,
            None,
        )
        .await?;

        tx.commit().await?;

        tracing::info!(
            step_id = %step_id,
            chain_id = %chain.id,
            rejector = %rejector_id,
            "Approval step rejected — chain rejected"
        );

        Ok(updated_step)
    }

    // ══════════════════════════════════════════════════════════════════════
    // Delegation
    // ══════════════════════════════════════════════════════════════════════

    /// Delegate a step to another user.
    /// Marks the original step as "delegated" and creates a new pending step
    /// for the delegate.
    pub async fn delegate_step(
        &self,
        tenant_id: &Uuid,
        step_id: &Uuid,
        delegator_id: &Uuid,
        delegate_to: &Uuid,
        reason: Option<&str>,
    ) -> Result<ApprovalChainStep, ApprovalError> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let step = self
            .get_step_tx(&mut tx, tenant_id, step_id)
            .await?
            .ok_or_else(|| ApprovalError::StepNotFound(step_id.to_string()))?;

        if step.status != "pending" {
            return Err(ApprovalError::InvalidTransition {
                current: step.status.clone(),
                requested: "delegated".to_string(),
            });
        }

        if step.assigned_to != *delegator_id {
            return Err(ApprovalError::NotAuthorized(format!(
                "User {} is not assigned to step {}",
                delegator_id, step_id
            )));
        }

        // Mark original step as delegated
        sqlx::query(
            r#"
            UPDATE approval_chain_steps
            SET status = 'delegated', delegated_to = $3, delegated_at = $4,
                delegation_reason = $5, updated_at = $4
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(step_id)
        .bind(tenant_id)
        .bind(delegate_to)
        .bind(now)
        .bind(reason)
        .execute(&mut *tx)
        .await?;

        // Create new step for the delegate
        let new_step_id = Uuid::new_v4();
        let due_at = step.due_at; // carry over the original deadline

        let new_step_row = sqlx::query(
            r#"
            INSERT INTO approval_chain_steps (
                id, chain_id, tenant_id, level_id, level_order,
                assigned_to, status, assigned_at, due_at,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9, $10)
            RETURNING id, chain_id, tenant_id, level_id, level_order,
                      assigned_to, status, decision, comments,
                      delegated_to, delegated_at, delegation_reason,
                      assigned_at, due_at, responded_at,
                      escalated_at, escalated_to, escalation_reason,
                      created_at, updated_at
            "#,
        )
        .bind(new_step_id)
        .bind(step.chain_id)
        .bind(tenant_id)
        .bind(step.level_id)
        .bind(step.level_order)
        .bind(delegate_to)
        .bind(now)
        .bind(due_at)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        let new_step = row_to_step(&new_step_row);

        // Get invoice ID before logging (avoid double mutable borrow)
        let invoice_id = self.get_invoice_id_for_chain(&mut tx, &step.chain_id).await?;

        // Log activity
        self.log_activity_tx(
            &mut tx,
            tenant_id,
            &step.chain_id,
            Some(step_id),
            &invoice_id,
            "delegated",
            delegator_id,
            reason,
            Some(serde_json::json!({
                "delegated_to": delegate_to,
                "new_step_id": new_step_id,
            })),
        )
        .await?;

        tx.commit().await?;

        tracing::info!(
            step_id = %step_id,
            delegator = %delegator_id,
            delegate = %delegate_to,
            "Approval step delegated"
        );

        Ok(new_step)
    }

    // ══════════════════════════════════════════════════════════════════════
    // Escalation
    // ══════════════════════════════════════════════════════════════════════

    /// Escalate overdue steps.
    /// Finds all pending steps past their due_at, marks them as escalated,
    /// and creates new steps for escalation targets.
    pub async fn escalate_overdue(
        &self,
        tenant_id: &Uuid,
    ) -> Result<Vec<ApprovalChainStep>, ApprovalError> {
        let now = Utc::now();

        // Find overdue pending steps
        let overdue_rows = sqlx::query(
            r#"
            SELECT s.id, s.chain_id, s.tenant_id, s.level_id, s.level_order,
                   s.assigned_to, s.status, s.decision, s.comments,
                   s.delegated_to, s.delegated_at, s.delegation_reason,
                   s.assigned_at, s.due_at, s.responded_at,
                   s.escalated_at, s.escalated_to, s.escalation_reason,
                   s.created_at, s.updated_at
            FROM approval_chain_steps s
            JOIN active_approval_chains c ON c.id = s.chain_id
            WHERE s.tenant_id = $1
              AND s.status = 'pending'
              AND s.due_at IS NOT NULL
              AND s.due_at < $2
              AND c.status = 'in_progress'
            "#,
        )
        .bind(tenant_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        let mut escalated_steps = Vec::new();

        for row in &overdue_rows {
            let step = row_to_step(row);

            let mut tx = self.pool.begin().await?;

            // Get the chain to find the policy's escalation target
            let chain = match self.get_chain_tx(&mut tx, tenant_id, &step.chain_id).await? {
                Some(c) => c,
                None => continue,
            };

            let policy = match self
                .policy_service
                .get_policy(tenant_id, &chain.policy_id)
                .await?
            {
                Some(p) => p,
                None => continue,
            };

            if !policy.escalation_enabled {
                continue;
            }

            // Determine escalation target
            let escalation_target = if let Some(final_user) = policy.final_escalation_user_id {
                final_user
            } else {
                // No escalation target configured — skip
                continue;
            };

            // Mark step as escalated
            sqlx::query(
                r#"
                UPDATE approval_chain_steps
                SET status = 'escalated', escalated_at = $3, escalated_to = $4,
                    escalation_reason = 'Overdue - exceeded deadline', updated_at = $3
                WHERE id = $1 AND tenant_id = $2
                "#,
            )
            .bind(step.id)
            .bind(tenant_id)
            .bind(now)
            .bind(escalation_target)
            .execute(&mut *tx)
            .await?;

            // Create new step for escalation target
            let new_step_id = Uuid::new_v4();
            let new_due = policy
                .escalation_timeout_hours
                .map(|h| now + chrono::Duration::hours(h as i64));

            let new_step_row = sqlx::query(
                r#"
                INSERT INTO approval_chain_steps (
                    id, chain_id, tenant_id, level_id, level_order,
                    assigned_to, status, assigned_at, due_at,
                    created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9, $10)
                RETURNING id, chain_id, tenant_id, level_id, level_order,
                          assigned_to, status, decision, comments,
                          delegated_to, delegated_at, delegation_reason,
                          assigned_at, due_at, responded_at,
                          escalated_at, escalated_to, escalation_reason,
                          created_at, updated_at
                "#,
            )
            .bind(new_step_id)
            .bind(step.chain_id)
            .bind(tenant_id)
            .bind(step.level_id)
            .bind(step.level_order)
            .bind(escalation_target)
            .bind(now)
            .bind(new_due)
            .bind(now)
            .bind(now)
            .fetch_one(&mut *tx)
            .await?;

            // Increment chain escalation count
            sqlx::query(
                r#"
                UPDATE active_approval_chains
                SET escalation_count = escalation_count + 1,
                    last_escalated_at = $2, updated_at = $2
                WHERE id = $1
                "#,
            )
            .bind(step.chain_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Log activity
            self.log_activity_tx(
                &mut tx,
                tenant_id,
                &step.chain_id,
                Some(&step.id),
                &chain.invoice_id,
                "escalated",
                &escalation_target,
                Some("Overdue - exceeded deadline"),
                Some(serde_json::json!({
                    "original_assignee": step.assigned_to,
                    "escalated_to": escalation_target,
                    "new_step_id": new_step_id,
                })),
            )
            .await?;

            tx.commit().await?;

            escalated_steps.push(row_to_step(&new_step_row));
        }

        if !escalated_steps.is_empty() {
            tracing::info!(
                count = escalated_steps.len(),
                "Escalated overdue approval steps"
            );
        }

        Ok(escalated_steps)
    }

    // ══════════════════════════════════════════════════════════════════════
    // Queries
    // ══════════════════════════════════════════════════════════════════════

    /// Get pending approvals for a user with invoice/policy context
    pub async fn get_pending_for_user(
        &self,
        tenant_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<Vec<PendingApprovalSummary>, ApprovalError> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.id as step_id, s.chain_id, s.tenant_id as step_tenant_id,
                s.level_id, s.level_order, s.assigned_to, s.status as step_status,
                s.decision, s.comments as step_comments,
                s.delegated_to, s.delegated_at, s.delegation_reason,
                s.assigned_at, s.due_at, s.responded_at,
                s.escalated_at, s.escalated_to, s.escalation_reason,
                s.created_at as step_created_at, s.updated_at as step_updated_at,
                c.id as chain_id_2, c.tenant_id as chain_tenant_id,
                c.invoice_id, c.policy_id, c.status as chain_status,
                c.current_level, c.total_levels,
                c.final_decision, c.final_decided_by, c.final_decided_at,
                c.escalation_count, c.last_escalated_at,
                c.initiated_by, c.initiated_at,
                c.completed_at as chain_completed_at,
                c.created_at as chain_created_at, c.updated_at as chain_updated_at,
                i.invoice_number, i.vendor_name,
                i.total_amount_cents,
                p.name as policy_name,
                l.name as level_name
            FROM approval_chain_steps s
            JOIN active_approval_chains c ON c.id = s.chain_id AND c.tenant_id = s.tenant_id
            JOIN invoices i ON i.id = c.invoice_id AND i.tenant_id = c.tenant_id
            JOIN approval_policies p ON p.id = c.policy_id
            JOIN approval_chain_levels l ON l.id = s.level_id
            WHERE s.tenant_id = $1
              AND s.assigned_to = $2
              AND s.status = 'pending'
              AND c.status = 'in_progress'
            ORDER BY s.due_at ASC NULLS LAST, s.created_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let summaries = rows
            .iter()
            .map(|row| {
                let step = ApprovalChainStep {
                    id: row.get("step_id"),
                    chain_id: row.get("chain_id"),
                    tenant_id: row.get("step_tenant_id"),
                    level_id: row.get("level_id"),
                    level_order: row.get("level_order"),
                    assigned_to: row.get("assigned_to"),
                    status: row.get("step_status"),
                    decision: row.get("decision"),
                    comments: row.get("step_comments"),
                    delegated_to: row.get("delegated_to"),
                    delegated_at: row.get("delegated_at"),
                    delegation_reason: row.get("delegation_reason"),
                    assigned_at: row.get("assigned_at"),
                    due_at: row.get("due_at"),
                    responded_at: row.get("responded_at"),
                    escalated_at: row.get("escalated_at"),
                    escalated_to: row.get("escalated_to"),
                    escalation_reason: row.get("escalation_reason"),
                    created_at: row.get("step_created_at"),
                    updated_at: row.get("step_updated_at"),
                };

                let chain = ActiveApprovalChain {
                    id: row.get("chain_id_2"),
                    tenant_id: row.get("chain_tenant_id"),
                    invoice_id: row.get("invoice_id"),
                    policy_id: row.get("policy_id"),
                    status: row.get("chain_status"),
                    current_level: row.get("current_level"),
                    total_levels: row.get("total_levels"),
                    final_decision: row.get("final_decision"),
                    final_decided_by: row.get("final_decided_by"),
                    final_decided_at: row.get("final_decided_at"),
                    escalation_count: row.get("escalation_count"),
                    last_escalated_at: row.get("last_escalated_at"),
                    initiated_by: row.get("initiated_by"),
                    initiated_at: row.get("initiated_at"),
                    completed_at: row.get("chain_completed_at"),
                    created_at: row.get("chain_created_at"),
                    updated_at: row.get("chain_updated_at"),
                };

                PendingApprovalSummary {
                    invoice_id: row.get("invoice_id"),
                    invoice_number: row.get("invoice_number"),
                    vendor_name: row.get("vendor_name"),
                    total_amount_cents: row.get("total_amount_cents"),
                    policy_name: row.get("policy_name"),
                    level_name: row.get("level_name"),
                    step,
                    chain,
                }
            })
            .collect();

        Ok(summaries)
    }

    /// Get full chain detail (chain + policy + steps + activity)
    pub async fn get_chain_detail(
        &self,
        tenant_id: &Uuid,
        chain_id: &Uuid,
    ) -> Result<Option<ApprovalChainDetail>, ApprovalError> {
        let chain_row = sqlx::query(
            r#"
            SELECT id, tenant_id, invoice_id, policy_id, status,
                   current_level, total_levels, final_decision,
                   final_decided_by, final_decided_at, escalation_count,
                   last_escalated_at, initiated_by, initiated_at,
                   completed_at, created_at, updated_at
            FROM active_approval_chains
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let chain_row = match chain_row {
            Some(r) => r,
            None => return Ok(None),
        };

        let chain = row_to_chain(&chain_row);

        let policy = self
            .policy_service
            .get_policy(tenant_id, &chain.policy_id)
            .await?
            .ok_or_else(|| ApprovalError::PolicyNotFound(chain.policy_id.to_string()))?;

        let step_rows = sqlx::query(
            r#"
            SELECT id, chain_id, tenant_id, level_id, level_order,
                   assigned_to, status, decision, comments,
                   delegated_to, delegated_at, delegation_reason,
                   assigned_at, due_at, responded_at,
                   escalated_at, escalated_to, escalation_reason,
                   created_at, updated_at
            FROM approval_chain_steps
            WHERE chain_id = $1 AND tenant_id = $2
            ORDER BY level_order ASC, created_at ASC
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let steps: Vec<ApprovalChainStep> = step_rows.iter().map(row_to_step).collect();

        let activity_rows = sqlx::query(
            r#"
            SELECT id, tenant_id, chain_id, step_id, invoice_id,
                   action, actor_id, actor_role, comments, metadata,
                   ip_address, created_at
            FROM approval_activity_log
            WHERE chain_id = $1 AND tenant_id = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let activity: Vec<ApprovalActivity> = activity_rows.iter().map(row_to_activity).collect();

        Ok(Some(ApprovalChainDetail {
            chain,
            policy,
            steps,
            activity,
        }))
    }

    /// Get the active chain for a specific invoice
    pub async fn get_chain_for_invoice(
        &self,
        tenant_id: &Uuid,
        invoice_id: &Uuid,
    ) -> Result<Option<ActiveApprovalChain>, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, invoice_id, policy_id, status,
                   current_level, total_levels, final_decision,
                   final_decided_by, final_decided_at, escalation_count,
                   last_escalated_at, initiated_by, initiated_at,
                   completed_at, created_at, updated_at
            FROM active_approval_chains
            WHERE invoice_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(invoice_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_chain))
    }

    /// Recall (cancel) a chain. Only the initiator can recall.
    pub async fn recall_chain(
        &self,
        tenant_id: &Uuid,
        chain_id: &Uuid,
        recalled_by: &Uuid,
    ) -> Result<ActiveApprovalChain, ApprovalError> {
        let mut tx = self.pool.begin().await?;

        let chain = self
            .get_chain_tx(&mut tx, tenant_id, chain_id)
            .await?
            .ok_or_else(|| ApprovalError::ChainNotFound(chain_id.to_string()))?;

        if chain.status != "in_progress" && chain.status != "pending" {
            return Err(ApprovalError::AlreadyCompleted);
        }

        if chain.initiated_by != *recalled_by {
            return Err(ApprovalError::NotAuthorized(
                "Only the initiator can recall an approval chain".to_string(),
            ));
        }

        let now = Utc::now();

        // Cancel the chain
        let chain_row = sqlx::query(
            r#"
            UPDATE active_approval_chains
            SET status = 'cancelled', completed_at = $3, updated_at = $3
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, tenant_id, invoice_id, policy_id, status,
                      current_level, total_levels, final_decision,
                      final_decided_by, final_decided_at, escalation_count,
                      last_escalated_at, initiated_by, initiated_at,
                      completed_at, created_at, updated_at
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        // Cancel all pending steps
        self.cancel_pending_steps(&mut tx, chain_id).await?;

        // Log activity
        self.log_activity_tx(
            &mut tx,
            tenant_id,
            chain_id,
            None,
            &chain.invoice_id,
            "recalled",
            recalled_by,
            None,
            None,
        )
        .await?;

        tx.commit().await?;

        tracing::info!(chain_id = %chain_id, recalled_by = %recalled_by, "Approval chain recalled");

        Ok(row_to_chain(&chain_row))
    }

    // ══════════════════════════════════════════════════════════════════════
    // Private Helpers
    // ══════════════════════════════════════════════════════════════════════

    /// Create an auto-approved chain (for invoices below the threshold)
    async fn create_auto_approved_chain(
        &self,
        tenant_id: &Uuid,
        invoice_id: &Uuid,
        policy: &ApprovalPolicy,
        initiated_by: &Uuid,
    ) -> Result<ActiveApprovalChain, ApprovalError> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let chain_id = Uuid::new_v4();

        let chain_row = sqlx::query(
            r#"
            INSERT INTO active_approval_chains (
                id, tenant_id, invoice_id, policy_id, status,
                current_level, total_levels, final_decision,
                final_decided_by, final_decided_at, escalation_count,
                initiated_by, initiated_at, completed_at,
                created_at, updated_at
            ) VALUES ($1, $2, $3, $4, 'approved', 0, 0, 'approved', $5, $6, 0, $5, $6, $6, $6, $6)
            RETURNING id, tenant_id, invoice_id, policy_id, status,
                      current_level, total_levels, final_decision,
                      final_decided_by, final_decided_at, escalation_count,
                      last_escalated_at, initiated_by, initiated_at,
                      completed_at, created_at, updated_at
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .bind(invoice_id)
        .bind(policy.id)
        .bind(initiated_by)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        self.log_activity_tx(
            &mut tx,
            tenant_id,
            &chain_id,
            None,
            invoice_id,
            "auto_approved",
            initiated_by,
            Some("Below auto-approve threshold"),
            Some(serde_json::json!({
                "policy_id": policy.id,
                "policy_name": policy.name,
                "threshold_cents": policy.auto_approve_below_cents,
            })),
        )
        .await?;

        tx.commit().await?;

        tracing::info!(
            chain_id = %chain_id,
            invoice_id = %invoice_id,
            "Invoice auto-approved (below threshold)"
        );

        Ok(row_to_chain(&chain_row))
    }

    /// Create approval steps for a given chain level
    async fn create_steps_for_level(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        chain_id: Uuid,
        level: &ApprovalChainLevel,
        policy: &ApprovalPolicy,
        initiated_by: &Uuid,
    ) -> Result<(), ApprovalError> {
        let now = Utc::now();

        // Determine due_at from level timeout or policy default
        let timeout_hours = level
            .timeout_hours
            .or(policy.escalation_timeout_hours);
        let due_at = timeout_hours.map(|h| now + chrono::Duration::hours(h as i64));

        // Get approver user IDs from the level configuration
        let approver_ids: Vec<Uuid> = serde_json::from_value(level.approver_user_ids.clone())
            .unwrap_or_default();

        if approver_ids.is_empty() {
            // If no explicit users, we still create a step — the API layer
            // should resolve role-based or department_head approvers.
            // For now, log a warning.
            tracing::warn!(
                level_id = %level.id,
                approver_type = %level.approver_type,
                "No explicit approver user IDs for level — role-based resolution needed"
            );
            return Ok(());
        }

        // Filter out self-approval if not allowed
        let approver_ids: Vec<Uuid> = if !policy.allow_self_approval {
            approver_ids
                .into_iter()
                .filter(|id| id != initiated_by)
                .collect()
        } else {
            approver_ids
        };

        for approver_id in &approver_ids {
            let step_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO approval_chain_steps (
                    id, chain_id, tenant_id, level_id, level_order,
                    assigned_to, status, assigned_at, due_at,
                    created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9, $10)
                "#,
            )
            .bind(step_id)
            .bind(chain_id)
            .bind(tenant_id)
            .bind(level.id)
            .bind(level.level_order)
            .bind(approver_id)
            .bind(now)
            .bind(due_at)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    /// Advance the chain to the next level (for sequential policies)
    async fn advance_to_next_level_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        chain: &ActiveApprovalChain,
        policy: &ApprovalPolicy,
    ) -> Result<(), ApprovalError> {
        let next_level = chain.current_level + 1;
        let now = Utc::now();

        // Update chain's current level
        sqlx::query(
            r#"
            UPDATE active_approval_chains
            SET current_level = $3, updated_at = $4
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(chain.id)
        .bind(tenant_id)
        .bind(next_level)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        // Get the next level's definition
        let level_row = sqlx::query(
            r#"
            SELECT id, policy_id, tenant_id, level_order, name,
                   approver_type, approver_user_ids, approver_role,
                   min_amount_cents, max_amount_cents, required_approver_count,
                   timeout_hours, created_at, updated_at
            FROM approval_chain_levels
            WHERE policy_id = $1 AND level_order = $2
            "#,
        )
        .bind(chain.policy_id)
        .bind(next_level)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(row) = level_row {
            let level = row_to_chain_level(&row);
            self.create_steps_for_level(tx, tenant_id, chain.id, &level, policy, &chain.initiated_by)
                .await?;
        }

        Ok(())
    }

    /// Complete (approve or reject) a chain
    async fn complete_chain(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        chain_id: &Uuid,
        decision: &str,
        decided_by: &Uuid,
    ) -> Result<(), ApprovalError> {
        let now = Utc::now();
        let status = decision; // "approved" or "rejected"

        sqlx::query(
            r#"
            UPDATE active_approval_chains
            SET status = $3, final_decision = $3, final_decided_by = $4,
                final_decided_at = $5, completed_at = $5, updated_at = $5
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .bind(status)
        .bind(decided_by)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Cancel all pending steps in a chain
    async fn cancel_pending_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        chain_id: &Uuid,
    ) -> Result<(), ApprovalError> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE approval_chain_steps
            SET status = 'cancelled', updated_at = $2
            WHERE chain_id = $1 AND status = 'pending'
            "#,
        )
        .bind(chain_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Count approved steps at a specific level in a chain
    async fn count_approved_at_level(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        chain_id: &Uuid,
        level_order: i32,
    ) -> Result<i64, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as cnt
            FROM approval_chain_steps
            WHERE chain_id = $1 AND level_order = $2 AND status = 'approved'
            "#,
        )
        .bind(chain_id)
        .bind(level_order)
        .fetch_one(&mut **tx)
        .await?;

        Ok(row.get::<i64, _>("cnt"))
    }

    /// Check if all levels in a parallel chain are completed
    async fn all_levels_completed(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        chain_id: &Uuid,
        total_levels: i32,
    ) -> Result<bool, ApprovalError> {
        // Count distinct levels that still have pending steps
        let row = sqlx::query(
            r#"
            SELECT COUNT(DISTINCT level_order) as pending_levels
            FROM approval_chain_steps
            WHERE chain_id = $1 AND status = 'pending'
            "#,
        )
        .bind(chain_id)
        .fetch_one(&mut **tx)
        .await?;

        let pending_levels = row.get::<i64, _>("pending_levels");
        let _ = total_levels; // used conceptually, but we check if any pending remain
        Ok(pending_levels == 0)
    }

    /// Get the chain level definition for a step
    async fn get_level_for_step(
        &self,
        step: &ApprovalChainStep,
    ) -> Result<ApprovalChainLevel, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT id, policy_id, tenant_id, level_order, name,
                   approver_type, approver_user_ids, approver_role,
                   min_amount_cents, max_amount_cents, required_approver_count,
                   timeout_hours, created_at, updated_at
            FROM approval_chain_levels
            WHERE id = $1
            "#,
        )
        .bind(step.level_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            ApprovalError::Internal(format!("Chain level {} not found", step.level_id))
        })?;

        Ok(row_to_chain_level(&row))
    }

    /// Get a step within a transaction
    async fn get_step_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        step_id: &Uuid,
    ) -> Result<Option<ApprovalChainStep>, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT id, chain_id, tenant_id, level_id, level_order,
                   assigned_to, status, decision, comments,
                   delegated_to, delegated_at, delegation_reason,
                   assigned_at, due_at, responded_at,
                   escalated_at, escalated_to, escalation_reason,
                   created_at, updated_at
            FROM approval_chain_steps
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(step_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(row.as_ref().map(row_to_step))
    }

    /// Get a chain within a transaction
    async fn get_chain_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        chain_id: &Uuid,
    ) -> Result<Option<ActiveApprovalChain>, ApprovalError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, invoice_id, policy_id, status,
                   current_level, total_levels, final_decision,
                   final_decided_by, final_decided_at, escalation_count,
                   last_escalated_at, initiated_by, initiated_at,
                   completed_at, created_at, updated_at
            FROM active_approval_chains
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(chain_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(row.as_ref().map(row_to_chain))
    }

    /// Get the invoice_id for a chain (helper for delegation logging)
    async fn get_invoice_id_for_chain(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        chain_id: &Uuid,
    ) -> Result<Uuid, ApprovalError> {
        let row = sqlx::query(
            "SELECT invoice_id FROM active_approval_chains WHERE id = $1",
        )
        .bind(chain_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(row.get::<Uuid, _>("invoice_id"))
    }

    /// Log an activity entry within a transaction
    async fn log_activity_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid,
        chain_id: &Uuid,
        step_id: Option<&Uuid>,
        invoice_id: &Uuid,
        action: &str,
        actor_id: &Uuid,
        comments: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), ApprovalError> {
        let now = Utc::now();
        let meta = metadata.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query(
            r#"
            INSERT INTO approval_activity_log (
                id, tenant_id, chain_id, step_id, invoice_id,
                action, actor_id, comments, metadata, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(chain_id)
        .bind(step_id)
        .bind(invoice_id)
        .bind(action)
        .bind(actor_id)
        .bind(comments)
        .bind(&meta)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════
// Row Mapping
// ══════════════════════════════════════════════════════════════════════

fn row_to_chain(row: &PgRow) -> ActiveApprovalChain {
    ActiveApprovalChain {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        invoice_id: row.get("invoice_id"),
        policy_id: row.get("policy_id"),
        status: row.get("status"),
        current_level: row.get("current_level"),
        total_levels: row.get("total_levels"),
        final_decision: row.get("final_decision"),
        final_decided_by: row.get("final_decided_by"),
        final_decided_at: row.get("final_decided_at"),
        escalation_count: row.get("escalation_count"),
        last_escalated_at: row.get("last_escalated_at"),
        initiated_by: row.get("initiated_by"),
        initiated_at: row.get("initiated_at"),
        completed_at: row.get("completed_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_step(row: &PgRow) -> ApprovalChainStep {
    ApprovalChainStep {
        id: row.get("id"),
        chain_id: row.get("chain_id"),
        tenant_id: row.get("tenant_id"),
        level_id: row.get("level_id"),
        level_order: row.get("level_order"),
        assigned_to: row.get("assigned_to"),
        status: row.get("status"),
        decision: row.get("decision"),
        comments: row.get("comments"),
        delegated_to: row.get("delegated_to"),
        delegated_at: row.get("delegated_at"),
        delegation_reason: row.get("delegation_reason"),
        assigned_at: row.get("assigned_at"),
        due_at: row.get("due_at"),
        responded_at: row.get("responded_at"),
        escalated_at: row.get("escalated_at"),
        escalated_to: row.get("escalated_to"),
        escalation_reason: row.get("escalation_reason"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_activity(row: &PgRow) -> ApprovalActivity {
    ApprovalActivity {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        chain_id: row.get("chain_id"),
        step_id: row.get("step_id"),
        invoice_id: row.get("invoice_id"),
        action: row.get("action"),
        actor_id: row.get("actor_id"),
        actor_role: row.get("actor_role"),
        comments: row.get("comments"),
        metadata: row.get("metadata"),
        ip_address: row.get("ip_address"),
        created_at: row.get("created_at"),
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
