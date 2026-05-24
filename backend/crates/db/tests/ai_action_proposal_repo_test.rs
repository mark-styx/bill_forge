//! Integration tests for AI action proposal persistence.
//!
//! Gated behind `#[cfg_attr(not(feature = "integration"), ignore)]` so
//! `cargo test` skips them by default; run with `--features integration`.

use billforge_core::{TenantId, UserId};
use billforge_db::repositories::{
    AiActionProposalRepositoryImpl, AiActionProposalRisk, AiActionProposalStatus,
    AiConversationRepositoryImpl, CreateAiActionProposalInput, UpdateAiActionProposalStatusInput,
};
use billforge_db::PgManager;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Insert a minimal user row for the given tenant.
async fn seed_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind(format!("user-{}@test.com", user_id))
    .bind("hash")
    .bind("Test User")
    .execute(pool)
    .await
    .expect("seed user");
}

#[derive(Debug, sqlx::FromRow)]
struct AuditLogRow {
    tenant_id: Uuid,
    user_id: Uuid,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    changes: serde_json::Value,
}

async fn fetch_proposal_audit_row(
    pool: &sqlx::PgPool,
    proposal_id: Uuid,
    action: &str,
) -> AuditLogRow {
    sqlx::query_as::<_, AuditLogRow>(
        r#"SELECT tenant_id, user_id, action, resource_type, resource_id, changes
           FROM audit_log
           WHERE resource_id = $1 AND action = $2"#,
    )
    .bind(proposal_id.to_string())
    .bind(action)
    .fetch_one(pool)
    .await
    .expect("fetch proposal audit row")
}

async fn assert_single_proposal_audit_row(pool: &sqlx::PgPool, proposal_id: Uuid, action: &str) {
    let count = count_proposal_audit_rows(pool, proposal_id, action).await;

    assert_eq!(count, 1, "expected exactly one {action} audit row");
}

async fn count_proposal_audit_rows(pool: &sqlx::PgPool, proposal_id: Uuid, action: &str) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM audit_log
           WHERE resource_id = $1 AND action = $2"#,
    )
    .bind(proposal_id.to_string())
    .bind(action)
    .fetch_one(pool)
    .await
    .expect("count proposal audit rows")
}

fn assert_common_audit_fields(
    row: &AuditLogRow,
    tenant_id: &TenantId,
    user_id: Uuid,
    proposal_id: Uuid,
    action: &str,
) {
    assert_eq!(row.tenant_id, *tenant_id.as_uuid());
    assert_eq!(row.user_id, user_id);
    assert_eq!(row.action, action);
    assert_eq!(row.resource_type, "ai_action_proposal");
    let expected_resource_id = proposal_id.to_string();
    assert_eq!(
        row.resource_id.as_deref(),
        Some(expected_resource_id.as_str())
    );
}

// ---------------------------------------------------------------------------
// Shared setup helper
// ---------------------------------------------------------------------------

/// Create a fresh tenant database with migrations applied and return
/// `(manager, tenant_id, pool)`. The tag ensures test isolation.
async fn setup_single_tenant(tag: &str) -> (PgManager, TenantId, sqlx::PgPool) {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    let tenant_id: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("tag-{}", tag).as_bytes())
        .to_string()
        .parse()
        .unwrap();

    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(
            &tenant_id,
            &format!("AI Action Proposal Test Tenant {}", tag),
        )
        .await
        .expect("create tenant");

    let pool = (*manager.tenant(&tenant_id).await.expect("pool")).clone();
    run_proposal_test_schema(&pool).await;
    sqlx::query(
        "INSERT INTO tenants (id, name, slug)
         VALUES ($1, $2, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(*tenant_id.as_uuid())
    .bind(format!("AI Action Proposal Test Tenant {}", tag))
    .bind(format!("ai-action-proposal-test-tenant-{}", tag))
    .execute(&pool)
    .await
    .expect("seed tenant");

    (manager, tenant_id, pool)
}

async fn run_proposal_test_schema(pool: &sqlx::PgPool) {
    sqlx::raw_sql(include_str!("../../../migrations/001_create_tenants.sql"))
        .execute(pool)
        .await
        .expect("create tenants table");

    sqlx::raw_sql(include_str!("../../../migrations/002_create_users.sql"))
        .execute(pool)
        .await
        .expect("create users table");

    sqlx::raw_sql(include_str!(
        "../../../migrations/082_create_ai_conversations.sql"
    ))
    .execute(pool)
    .await
    .expect("create ai_conversations table");

    sqlx::raw_sql(include_str!(
        "../../../migrations/086_create_ai_action_proposals.sql"
    ))
    .execute(pool)
    .await
    .expect("create ai_action_proposals table");

    sqlx::raw_sql(include_str!(
        "../../../migrations/087_ai_action_proposal_status_failed_errors.sql"
    ))
    .execute(pool)
    .await
    .expect("update ai_action_proposals statuses");

    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS audit_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            user_id UUID NOT NULL REFERENCES users(id),
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            changes JSONB,
            ip_address INET,
            user_agent TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .expect("create audit_log table");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn create_and_load_list_action_proposals() {
    let (manager, tenant_id, pool) = setup_single_tenant("proposal-create-load").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    let conversation = conversation_repo
        .create_conversation(
            &tenant_id,
            &uid,
            Some("Proposal test"),
            serde_json::json!({"source": "test"}),
        )
        .await
        .expect("create conversation");

    let payload = serde_json::json!({
        "invoice_id": "inv-001",
        "action": "approve_invoice"
    });

    let first = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "approve_invoice".to_string(),
                payload: payload.clone(),
                risk: AiActionProposalRisk::Medium,
                permission: "invoice.approve".to_string(),
            },
        )
        .await
        .expect("create proposal");

    assert_eq!(first.tenant_id, *tenant_id.as_uuid());
    assert_eq!(first.user_id, user_id);
    assert_eq!(first.conversation_id, conversation.id);
    assert_eq!(first.tool_name, "approve_invoice");
    assert_eq!(first.payload, payload);
    assert_eq!(first.risk, AiActionProposalRisk::Medium);
    assert_eq!(first.permission, "invoice.approve");
    assert_eq!(first.status, AiActionProposalStatus::Pending);
    assert!(first.execution_error_code.is_none());
    assert!(first.execution_error_message.is_none());
    assert!(first.created_at <= chrono::Utc::now());
    assert!(first.updated_at <= chrono::Utc::now());

    let loaded = proposal_repo
        .get_proposal(&tenant_id, &uid, first.id)
        .await
        .expect("get proposal")
        .expect("proposal should exist");

    assert_eq!(loaded.id, first.id);
    assert_eq!(loaded.risk, AiActionProposalRisk::Medium);
    assert_eq!(loaded.status, AiActionProposalStatus::Pending);
    assert!(loaded.execution_error_code.is_none());
    assert!(loaded.execution_error_message.is_none());
    assert_eq!(loaded.payload["invoice_id"], "inv-001");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let second = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "create_payment".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-002"}),
                risk: AiActionProposalRisk::High,
                permission: "payment.create".to_string(),
            },
        )
        .await
        .expect("create second proposal");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let third = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "route_invoice".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-003"}),
                risk: AiActionProposalRisk::Low,
                permission: "invoice.route".to_string(),
            },
        )
        .await
        .expect("create third proposal");

    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            second.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("approve second proposal");

    let listed = proposal_repo
        .list_proposals_for_conversation(&tenant_id, &uid, conversation.id)
        .await
        .expect("list proposals");

    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].id, third.id);
    assert_eq!(listed[0].risk, AiActionProposalRisk::Low);
    assert_eq!(listed[1].id, second.id);
    assert_eq!(listed[2].id, first.id);

    let pending = proposal_repo
        .list_pending_proposals_for_conversation(&tenant_id, &uid, conversation.id)
        .await
        .expect("list pending proposals");

    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, third.id);
    assert_eq!(pending[1].id, first.id);
    assert!(pending
        .iter()
        .all(|proposal| proposal.status == AiActionProposalStatus::Pending));

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn update_status_and_enforce_scoped_access() {
    let (manager, tenant_id, pool) = setup_single_tenant("proposal-status-scope").await;
    let user_id = Uuid::new_v4();
    let wrong_user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;
    seed_user(pool.as_ref(), &tenant_id, wrong_user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);
    let wrong_uid = UserId(wrong_user_id);

    let conversation = conversation_repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let proposal = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "approve_invoice".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-003"}),
                risk: AiActionProposalRisk::Low,
                permission: "invoice.approve".to_string(),
            },
        )
        .await
        .expect("create proposal");

    assert_eq!(proposal.status, AiActionProposalStatus::Pending);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let approved = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("update status");

    assert_eq!(approved.id, proposal.id);
    assert_eq!(approved.status, AiActionProposalStatus::Approved);
    assert!(approved.updated_at > proposal.updated_at);

    let wrong_user_loaded = proposal_repo
        .get_proposal(&tenant_id, &wrong_uid, proposal.id)
        .await
        .expect("wrong user get should not error");

    assert!(wrong_user_loaded.is_none());

    let wrong_user_pending = proposal_repo
        .list_pending_proposals_for_conversation(&tenant_id, &wrong_uid, conversation.id)
        .await
        .expect("wrong user pending list should not error");

    assert!(wrong_user_pending.is_empty());

    let err = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &wrong_uid,
            proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Rejected,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect_err("wrong user should not update proposal");

    match err {
        billforge_core::Error::NotFound { resource_type, id } => {
            assert_eq!(resource_type, "ai_action_proposal");
            assert_eq!(id, proposal.id.to_string());
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn approve_pending_proposal_approves_pending_and_writes_audit() {
    let (manager, tenant_id, pool) = setup_single_tenant("approve-pending").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    let conversation = conversation_repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let proposal = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "approve_invoice".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-approve"}),
                risk: AiActionProposalRisk::Medium,
                permission: "invoice.approve".to_string(),
            },
        )
        .await
        .expect("create proposal");

    assert_eq!(proposal.status, AiActionProposalStatus::Pending);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let approved = proposal_repo
        .approve_pending_proposal(&tenant_id, &uid, proposal.id)
        .await
        .expect("approve pending proposal");

    assert_eq!(approved.id, proposal.id);
    assert_eq!(approved.status, AiActionProposalStatus::Approved);
    assert!(approved.updated_at > proposal.updated_at);
    assert!(approved.execution_error_code.is_none());
    assert!(approved.execution_error_message.is_none());

    let loaded = proposal_repo
        .get_proposal(&tenant_id, &uid, proposal.id)
        .await
        .expect("load approved proposal")
        .expect("proposal should exist");

    assert_eq!(loaded.status, AiActionProposalStatus::Approved);
    assert!(loaded.execution_error_code.is_none());
    assert!(loaded.execution_error_message.is_none());

    let audit =
        fetch_proposal_audit_row(pool.as_ref(), proposal.id, "ai_action_proposal_approved").await;
    assert_common_audit_fields(
        &audit,
        &tenant_id,
        user_id,
        proposal.id,
        "ai_action_proposal_approved",
    );
    assert_eq!(audit.changes["old_value"], serde_json::json!("pending"));
    assert_eq!(audit.changes["new_value"], serde_json::json!("approved"));
    assert_single_proposal_audit_row(pool.as_ref(), proposal.id, "ai_action_proposal_approved")
        .await;

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn approve_pending_proposal_wrong_user_returns_not_found_and_preserves_pending() {
    let (manager, tenant_id, pool) = setup_single_tenant("approve-pending-wrong-user").await;
    let user_id = Uuid::new_v4();
    let wrong_user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;
    seed_user(pool.as_ref(), &tenant_id, wrong_user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);
    let wrong_uid = UserId(wrong_user_id);

    let conversation = conversation_repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let proposal = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "approve_invoice".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-wrong-user"}),
                risk: AiActionProposalRisk::Low,
                permission: "invoice.approve".to_string(),
            },
        )
        .await
        .expect("create proposal");

    let err = proposal_repo
        .approve_pending_proposal(&tenant_id, &wrong_uid, proposal.id)
        .await
        .expect_err("wrong user should not approve proposal");

    match err {
        billforge_core::Error::NotFound { resource_type, id } => {
            assert_eq!(resource_type, "ai_action_proposal");
            assert_eq!(id, proposal.id.to_string());
        }
        other => panic!("expected NotFound, got {:?}", other),
    }

    let loaded = proposal_repo
        .get_proposal(&tenant_id, &uid, proposal.id)
        .await
        .expect("load proposal")
        .expect("proposal should exist");

    assert_eq!(loaded.status, AiActionProposalStatus::Pending);
    assert_eq!(
        count_proposal_audit_rows(pool.as_ref(), proposal.id, "ai_action_proposal_approved").await,
        0
    );

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn approve_pending_proposal_rejected_or_approved_returns_conflict() {
    let (manager, tenant_id, pool) = setup_single_tenant("approve-pending-conflict").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    let conversation = conversation_repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let create_proposal = |tool_name: &str| CreateAiActionProposalInput {
        conversation_id: conversation.id,
        tool_name: tool_name.to_string(),
        payload: serde_json::json!({"invoice_id": tool_name}),
        risk: AiActionProposalRisk::Medium,
        permission: "invoice.approve".to_string(),
    };

    let rejected_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("reject_invoice"))
        .await
        .expect("create rejected proposal");
    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            rejected_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Rejected,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("reject proposal");

    let approved_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("approve_invoice"))
        .await
        .expect("create approved proposal");
    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            approved_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("approve proposal");

    for (proposal_id, expected_status) in [
        (rejected_proposal.id, AiActionProposalStatus::Rejected),
        (approved_proposal.id, AiActionProposalStatus::Approved),
    ] {
        let err = proposal_repo
            .approve_pending_proposal(&tenant_id, &uid, proposal_id)
            .await
            .expect_err("non-pending proposal should not be approved");

        match err {
            billforge_core::Error::Conflict(message) => {
                assert!(message.contains(&proposal_id.to_string()));
                assert!(message.contains("not pending"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }

        let loaded = proposal_repo
            .get_proposal(&tenant_id, &uid, proposal_id)
            .await
            .expect("load proposal")
            .expect("proposal should exist");

        assert_eq!(loaded.status, expected_status);
    }

    assert_eq!(
        count_proposal_audit_rows(
            pool.as_ref(),
            rejected_proposal.id,
            "ai_action_proposal_approved"
        )
        .await,
        0
    );
    assert_single_proposal_audit_row(
        pool.as_ref(),
        approved_proposal.id,
        "ai_action_proposal_approved",
    )
    .await;

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn failed_status_persists_error_fields_and_non_failed_status_clears_them() {
    let (manager, tenant_id, pool) = setup_single_tenant("proposal-failed-errors").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    let conversation = conversation_repo
        .create_conversation(&tenant_id, &uid, None, serde_json::json!({}))
        .await
        .expect("create conversation");

    let proposal = proposal_repo
        .create_proposal(
            &tenant_id,
            &uid,
            CreateAiActionProposalInput {
                conversation_id: conversation.id,
                tool_name: "approve_invoice".to_string(),
                payload: serde_json::json!({"invoice_id": "inv-004"}),
                risk: AiActionProposalRisk::Low,
                permission: "invoice.approve".to_string(),
            },
        )
        .await
        .expect("create proposal");

    let failed = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Failed,
                execution_error_code: Some("execution_timeout".to_string()),
                execution_error_message: Some("Timed out executing proposal".to_string()),
            },
        )
        .await
        .expect("mark proposal failed");

    assert_eq!(failed.status, AiActionProposalStatus::Failed);
    assert_eq!(
        failed.execution_error_code.as_deref(),
        Some("execution_timeout")
    );
    assert_eq!(
        failed.execution_error_message.as_deref(),
        Some("Timed out executing proposal")
    );

    let loaded_failed = proposal_repo
        .get_proposal(&tenant_id, &uid, proposal.id)
        .await
        .expect("load failed proposal")
        .expect("proposal should exist");

    assert_eq!(loaded_failed.status, AiActionProposalStatus::Failed);
    assert_eq!(
        loaded_failed.execution_error_code.as_deref(),
        Some("execution_timeout")
    );
    assert_eq!(
        loaded_failed.execution_error_message.as_deref(),
        Some("Timed out executing proposal")
    );

    let approved = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: Some("ignored_code".to_string()),
                execution_error_message: Some("ignored message".to_string()),
            },
        )
        .await
        .expect("move failed proposal to approved");

    assert_eq!(approved.status, AiActionProposalStatus::Approved);
    assert!(approved.execution_error_code.is_none());
    assert!(approved.execution_error_message.is_none());

    manager.delete_tenant(&tenant_id).await.ok();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn writes_audit_logs_for_proposal_lifecycle_events() {
    let (manager, tenant_id, pool) = setup_single_tenant("proposal-audit").await;
    let user_id = Uuid::new_v4();

    let pool = Arc::new(pool);
    seed_user(pool.as_ref(), &tenant_id, user_id).await;

    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let proposal_repo = AiActionProposalRepositoryImpl::new(pool.clone());
    let uid = UserId(user_id);

    let conversation = conversation_repo
        .create_conversation(
            &tenant_id,
            &uid,
            Some("Proposal audit test"),
            serde_json::json!({"source": "audit_test"}),
        )
        .await
        .expect("create conversation");

    let create_proposal = |tool_name: &str| CreateAiActionProposalInput {
        conversation_id: conversation.id,
        tool_name: tool_name.to_string(),
        payload: serde_json::json!({"invoice_id": tool_name}),
        risk: AiActionProposalRisk::Medium,
        permission: "invoice.approve".to_string(),
    };

    let approved_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("approve_invoice"))
        .await
        .expect("create approved proposal");

    let created_audit = fetch_proposal_audit_row(
        pool.as_ref(),
        approved_proposal.id,
        "ai_action_proposal_created",
    )
    .await;
    assert_common_audit_fields(
        &created_audit,
        &tenant_id,
        user_id,
        approved_proposal.id,
        "ai_action_proposal_created",
    );
    assert_eq!(
        created_audit.changes["metadata"]["conversation_id"],
        serde_json::json!(conversation.id)
    );
    assert_eq!(
        created_audit.changes["metadata"]["tool_name"],
        serde_json::json!("approve_invoice")
    );
    assert_eq!(
        created_audit.changes["metadata"]["risk"],
        serde_json::json!("medium")
    );
    assert_eq!(
        created_audit.changes["metadata"]["permission"],
        serde_json::json!("invoice.approve")
    );
    assert_eq!(
        created_audit.changes["metadata"]["status"],
        serde_json::json!("pending")
    );
    assert_single_proposal_audit_row(
        pool.as_ref(),
        approved_proposal.id,
        "ai_action_proposal_created",
    )
    .await;

    let approved = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            approved_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("approve proposal");
    assert_eq!(approved.status, AiActionProposalStatus::Approved);

    let rejected_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("reject_invoice"))
        .await
        .expect("create rejected proposal");
    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            rejected_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Rejected,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("reject proposal");

    let executed_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("execute_invoice_action"))
        .await
        .expect("create executed proposal");
    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            executed_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Executed,
                execution_error_code: None,
                execution_error_message: None,
            },
        )
        .await
        .expect("execute proposal");

    let failed_proposal = proposal_repo
        .create_proposal(&tenant_id, &uid, create_proposal("fail_invoice_action"))
        .await
        .expect("create failed proposal");
    proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            failed_proposal.id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Failed,
                execution_error_code: Some("execution_timeout".to_string()),
                execution_error_message: Some("Timed out executing proposal".to_string()),
            },
        )
        .await
        .expect("fail proposal");

    for (proposal_id, action, new_status) in [
        (
            approved_proposal.id,
            "ai_action_proposal_approved",
            "approved",
        ),
        (
            rejected_proposal.id,
            "ai_action_proposal_rejected",
            "rejected",
        ),
        (
            executed_proposal.id,
            "ai_action_proposal_executed",
            "executed",
        ),
        (failed_proposal.id, "ai_action_proposal_failed", "failed"),
    ] {
        let row = fetch_proposal_audit_row(pool.as_ref(), proposal_id, action).await;
        assert_common_audit_fields(&row, &tenant_id, user_id, proposal_id, action);
        assert_eq!(row.changes["old_value"], serde_json::json!("pending"));
        assert_eq!(row.changes["new_value"], serde_json::json!(new_status));
        assert_single_proposal_audit_row(pool.as_ref(), proposal_id, action).await;
    }

    let failed_audit = fetch_proposal_audit_row(
        pool.as_ref(),
        failed_proposal.id,
        "ai_action_proposal_failed",
    )
    .await;
    assert_eq!(
        failed_audit.changes["metadata"]["execution_error_code"],
        serde_json::json!("execution_timeout")
    );
    assert_eq!(
        failed_audit.changes["metadata"]["execution_error_message"],
        serde_json::json!("Timed out executing proposal")
    );

    manager.delete_tenant(&tenant_id).await.ok();
}
