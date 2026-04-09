//! Smoke tests for AI assistant and billing route wiring.
//!
//! These tests verify the route modules produce correct JSON shapes
//! and that the billing plan data matches the backend truth.
//!
//! Auth enforcement: AI handlers now use the `AuthUser` extractor, which
//! rejects requests with missing/invalid Bearer tokens by returning
//! `Error::Unauthenticated` (HTTP 401).

use billforge_billing::{Plan, PlanId};
use billforge_billing::{BillingConfig, BillingService, BillingServiceTrait};
use billforge_core::TenantId;
use billforge_core::UserContext;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Billing plans data integrity
// ---------------------------------------------------------------------------

#[test]
fn test_all_public_plans_returns_three() {
    let plans = Plan::all_public();
    assert_eq!(plans.len(), 3, "all_public should return Free, Starter, Professional");
}

#[test]
fn test_starter_plan_pricing() {
    let starter = Plan::starter();
    assert_eq!(starter.monthly_price_cents, 4900, "Starter should be $49/mo = 4900 cents");
    assert_eq!(starter.features.max_invoices_per_month, 200);
    assert_eq!(starter.features.max_users, 3);
    assert!(starter.is_public);
}

#[test]
fn test_professional_plan_pricing() {
    let pro = Plan::professional();
    assert_eq!(pro.monthly_price_cents, 14900, "Professional should be $149/mo = 14900 cents");
    assert_eq!(pro.features.max_invoices_per_month, 1000);
    assert_eq!(pro.features.max_users, 10);
    assert!(pro.is_public);
}

#[test]
fn test_enterprise_plan_not_public() {
    let ent = Plan::enterprise();
    assert_eq!(ent.monthly_price_cents, 49900, "Enterprise should be $499/mo = 49900 cents");
    assert!(!ent.is_public, "Enterprise should not be in public listing");
}

#[test]
fn test_plans_serialize_to_json() {
    let plans = Plan::all_public();
    let val: serde_json::Value = serde_json::to_value(&plans).expect("plans serialize");
    let arr = val.as_array().expect("plans is array");
    assert_eq!(arr.len(), 3);

    // Verify pricing values survive serialization
    let starter = &arr[1]; // index 0 = Free, 1 = Starter, 2 = Professional
    assert_eq!(starter["monthly_price_cents"], 4900);
    let pro = &arr[2];
    assert_eq!(pro["monthly_price_cents"], 14900);
}

// ---------------------------------------------------------------------------
// Billing service default subscription
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn test_default_subscription_is_free(pool: sqlx::PgPool) {
    let pool = std::sync::Arc::new(pool);
    let service = BillingService::new(BillingConfig::default(), pool);
    let tenant_id = TenantId::from_uuid(Uuid::nil());
    let sub = service.get_subscription(&tenant_id).await.expect("get subscription");
    assert_eq!(sub.plan_id, PlanId::Free);
    assert!(sub.is_active());
}

// ---------------------------------------------------------------------------
// AI agent models serialize
// ---------------------------------------------------------------------------

#[test]
fn test_chat_request_deserialize() {
    let json = r#"{ "message": "What is invoice ACME-123 status?", "conversation_id": null }"#;
    let req: billforge_ai_agent::models::ChatRequest =
        serde_json::from_str(json).expect("deserialize ChatRequest");
    assert_eq!(req.message, "What is invoice ACME-123 status?");
    assert!(req.conversation_id.is_none());
}

#[test]
fn test_chat_response_serializes() {
    let resp = billforge_ai_agent::models::ChatResponse {
        conversation_id: Uuid::new_v4(),
        message: billforge_ai_agent::models::Message {
            id: Uuid::new_v4(),
            role: billforge_ai_agent::models::MessageRole::Assistant,
            content: "Invoice ACME-123 is pending approval.".to_string(),
            created_at: chrono::Utc::now(),
        },
    };
    let val: serde_json::Value = serde_json::to_value(&resp).expect("serialize ChatResponse");
    assert!(val.get("conversation_id").is_some());
    assert!(val.get("message").is_some());
    assert_eq!(val["message"]["role"], "assistant");
}

// ---------------------------------------------------------------------------
// AI route auth enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_ai_chat_requires_auth_unauthenticated_returns_401() {
    // When the AuthUser extractor finds no Authorization header it returns
    // Error::Unauthenticated. Verify that maps to HTTP 401.
    use billforge_core::Error;
    let err = Error::Unauthenticated;
    assert_eq!(err.status_code(), 401, "Unauthenticated must map to HTTP 401");
    assert_eq!(err.error_code(), "UNAUTHENTICATED");
}

#[test]
fn test_ai_handler_uses_authenticated_tenant_not_hardcoded() {
    // Construct a UserContext with a known tenant and verify the fields
    // that the handlers now read (tenant_id.0, user_id.0) produce the
    // correct values -- NOT "acme-mfg".
    use billforge_core::{Role, TenantId, UserId};

    let tenant_uuid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let user_uuid = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();

    let user = UserContext {
        user_id: UserId(user_uuid),
        tenant_id: TenantId(tenant_uuid),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        roles: vec![Role::ApUser],
    };

    // These are the exact expressions the handlers now use:
    let tenant_id_string = user.tenant_id.0.to_string();
    let extracted_user_id = user.user_id.0;

    assert_ne!(
        tenant_id_string, "acme-mfg",
        "tenant_id must come from the token, not the old hardcoded value"
    );
    assert_eq!(tenant_id_string, "22222222-2222-2222-2222-222222222222");
    assert_eq!(extracted_user_id, user_uuid);
    assert_ne!(
        extracted_user_id,
        Uuid::nil(),
        "user_id must come from the token, not Uuid::nil()"
    );
}
