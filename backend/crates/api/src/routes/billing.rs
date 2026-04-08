//! Billing API routes - exposes plan definitions and subscription status

use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use billforge_billing::{BillingConfig, BillingService, BillingServiceTrait, Plan};

use crate::extractors::AuthUser;
use crate::state::AppState;

/// Create billing sub-router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/plans", get(list_plans))
        .route("/subscription", get(get_subscription))
}

/// GET /billing/plans - return all public plans
#[utoipa::path(
    get,
    path = "/api/v1/billing/plans",
    tag = "Billing",
    responses(
        (status = 200, description = "Available billing plans"),
    )
)]
async fn list_plans() -> Json<Value> {
    let plans = Plan::all_public();
    Json(json!({
        "plans": plans,
    }))
}

/// GET /billing/subscription - return current subscription (default free)
#[utoipa::path(
    get,
    path = "/api/v1/billing/subscription",
    tag = "Billing",
    responses(
        (status = 200, description = "Current subscription"),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn get_subscription(
    AuthUser(user): AuthUser,
    State(_state): State<AppState>,
) -> Json<Value> {
    let tenant_id = user.tenant_id;
    let service = BillingService::new(BillingConfig::default());
    let sub = service.get_subscription(&tenant_id).await.unwrap_or_else(|_| {
        billforge_billing::Subscription::new_free(tenant_id)
    });
    Json(json!({
        "subscription": sub,
    }))
}
