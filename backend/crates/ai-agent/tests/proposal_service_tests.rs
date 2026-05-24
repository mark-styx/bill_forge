//! Tests for Winston proposal creation service.
//!
//! The persistence-path test is gated behind `integration` because it requires
//! PostgreSQL. The disabled-module test uses a lazy pool to prove the service
//! returns before touching the repository.

use std::sync::Arc;

use billforge_ai_agent::{CreateWinstonProposalInput, WinstonProposalService};
use billforge_core::{
    Error, Module, Role, TenantContext, TenantFeatures, TenantId, TenantSettings, UserContext,
    UserId,
};
use billforge_db::repositories::{
    AiActionProposalRisk, AiActionProposalStatus, AiConversationRepositoryImpl,
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn tenant_context(enabled_modules: Vec<Module>) -> TenantContext {
    TenantContext {
        tenant_id: TenantId::new(),
        tenant_name: "Test Tenant".to_string(),
        enabled_modules,
        settings: TenantSettings {
            logo_url: None,
            primary_color: None,
            company_name: "Test Tenant".to_string(),
            timezone: "UTC".to_string(),
            default_currency: "USD".to_string(),
            features: TenantFeatures::default(),
        },
    }
}

fn user_context(tenant_id: TenantId) -> UserContext {
    UserContext {
        user_id: UserId::new(),
        tenant_id,
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        roles: vec![Role::TenantAdmin],
    }
}

fn proposal_input(conversation_id: Uuid) -> CreateWinstonProposalInput {
    CreateWinstonProposalInput {
        conversation_id,
        tool_name: "approve_invoice".to_string(),
        payload: serde_json::json!({ "invoice_id": "inv-001" }),
        risk: AiActionProposalRisk::Medium,
        permission: "invoice.approve".to_string(),
    }
}

#[tokio::test]
async fn proposal_service_disabled_tenant_returns_module_not_available_before_persistence() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/should_not_connect")
        .expect("lazy pool");
    let service = WinstonProposalService::new(Arc::new(pool));
    let tenant = tenant_context(vec![Module::InvoiceCapture]);
    let user = user_context(tenant.tenant_id.clone());

    let err = service
        .create_pending_proposal(&tenant, &user, proposal_input(Uuid::new_v4()))
        .await
        .expect_err("disabled tenant should be rejected before persistence");

    match err {
        Error::ModuleNotAvailable(module) => {
            assert_eq!(module, Module::AiAssistant.display_name());
        }
        other => panic!("expected ModuleNotAvailable, got {:?}", other),
    }
}

async fn setup_minimal_schema(pool: &sqlx::PgPool) {
    let migration_001 = include_str!("../../../migrations/001_create_tenants.sql");
    sqlx::raw_sql(migration_001)
        .execute(pool)
        .await
        .expect("create tenants table");

    let migration_002 = include_str!("../../../migrations/002_create_users.sql");
    sqlx::raw_sql(migration_002)
        .execute(pool)
        .await
        .expect("create users table");

    let migration_082 = include_str!("../../../migrations/082_create_ai_conversations.sql");
    sqlx::raw_sql(migration_082)
        .execute(pool)
        .await
        .expect("create ai_conversations table");

    let migration_086 = include_str!("../../../migrations/086_create_ai_action_proposals.sql");
    sqlx::raw_sql(migration_086)
        .execute(pool)
        .await
        .expect("create ai_action_proposals table");
}

async fn insert_tenant(pool: &sqlx::PgPool, tenant: &TenantContext) {
    sqlx::query(
        r#"INSERT INTO tenants (id, name, slug)
           VALUES ($1, $2, $3)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&tenant.tenant_name)
    .bind(format!("tenant-{}", tenant.tenant_id.as_uuid()))
    .execute(pool)
    .await
    .expect("insert tenant");
}

async fn insert_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user: &UserContext) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, 'hash', $4, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(*user.user_id.as_uuid())
    .bind(*tenant_id.as_uuid())
    .bind(&user.email)
    .bind(&user.name)
    .execute(pool)
    .await
    .expect("insert user");
}

#[sqlx::test]
#[cfg_attr(not(feature = "integration"), ignore)]
async fn proposal_service_enabled_tenant_creates_approval_required_proposal(pool: sqlx::PgPool) {
    setup_minimal_schema(&pool).await;

    let tenant = tenant_context(vec![Module::AiAssistant]);
    let user = user_context(tenant.tenant_id.clone());

    insert_tenant(&pool, &tenant).await;
    insert_user(&pool, &tenant.tenant_id, &user).await;

    let pool = Arc::new(pool);
    let conversation_repo = AiConversationRepositoryImpl::new(pool.clone());
    let conversation = conversation_repo
        .create_conversation(
            &tenant.tenant_id,
            &user.user_id,
            Some("Proposal service test"),
            serde_json::json!({ "source": "proposal_service" }),
        )
        .await
        .expect("create conversation");

    let service = WinstonProposalService::new(pool);
    let proposal = service
        .create_pending_proposal(&tenant, &user, proposal_input(conversation.id))
        .await
        .expect("create proposal");

    assert_eq!(proposal.tenant_id, *tenant.tenant_id.as_uuid());
    assert_eq!(proposal.user_id, *user.user_id.as_uuid());
    assert_eq!(proposal.conversation_id, conversation.id);
    assert_eq!(proposal.tool_name, "approve_invoice");
    assert_eq!(proposal.payload["invoice_id"], "inv-001");
    assert_eq!(proposal.risk, AiActionProposalRisk::Medium);
    assert_eq!(proposal.permission, "invoice.approve");
    assert_eq!(proposal.status, AiActionProposalStatus::ApprovalRequired);
}
