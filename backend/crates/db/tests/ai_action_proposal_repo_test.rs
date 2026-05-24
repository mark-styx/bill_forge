//! Integration tests for AI action proposal persistence.
//!
//! Gated behind `#[cfg_attr(not(feature = "integration"), ignore)]` so
//! `cargo test` skips them by default; run with `--features integration`.

use billforge_core::{TenantId, UserId};
use billforge_db::repositories::{
    AiActionProposalRepositoryImpl, AiActionProposalRisk, AiActionProposalStatus,
    AiConversationRepositoryImpl, CreateAiActionProposalInput,
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
    manager.run_tenant_migrations(&pool).await.expect("migrate");

    (manager, tenant_id, pool)
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
    assert_eq!(first.status, AiActionProposalStatus::ApprovalRequired);
    assert!(first.created_at <= chrono::Utc::now());
    assert!(first.updated_at <= chrono::Utc::now());

    let loaded = proposal_repo
        .get_proposal(&tenant_id, &uid, first.id)
        .await
        .expect("get proposal")
        .expect("proposal should exist");

    assert_eq!(loaded.id, first.id);
    assert_eq!(loaded.risk, AiActionProposalRisk::Medium);
    assert_eq!(loaded.status, AiActionProposalStatus::ApprovalRequired);
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
            AiActionProposalStatus::Approved,
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
        .all(|proposal| proposal.status == AiActionProposalStatus::ApprovalRequired));

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

    assert_eq!(proposal.status, AiActionProposalStatus::ApprovalRequired);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let approved = proposal_repo
        .update_proposal_status(
            &tenant_id,
            &uid,
            proposal.id,
            AiActionProposalStatus::Approved,
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
            AiActionProposalStatus::Rejected,
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
