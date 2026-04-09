//! Billing API routes - exposes plan definitions, subscription status, and usage metering

use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use billforge_billing::{BillingConfig, BillingService, BillingServiceTrait, Plan};

use crate::error::ApiResult;
use crate::extractors::AuthUser;
use crate::state::AppState;

/// Create billing sub-router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/plans", get(list_plans))
        .route("/subscription", get(get_subscription))
        .route("/usage", get(get_usage))
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

/// GET /billing/usage - return per-tenant invoice and vendor counts for the current billing period
#[utoipa::path(
    get,
    path = "/api/v1/billing/usage",
    tag = "Billing",
    responses(
        (status = 200, description = "Current billing period usage"),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn get_usage(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    let tenant_id = user.tenant_id;
    let pool = state.db.tenant(&tenant_id).await?;

    let service = BillingService::new(BillingConfig::default());
    let sub = service.get_subscription(&tenant_id).await.unwrap_or_else(|_| {
        billforge_billing::Subscription::new_free(tenant_id.clone())
    });

    let usage = billforge_billing::get_tenant_usage(
        &pool,
        &tenant_id,
        sub.current_period_start,
        sub.current_period_end,
    )
    .await?;

    Ok(Json(json!({
        "usage": usage,
        "plan_id": sub.plan_id,
    })))
}
