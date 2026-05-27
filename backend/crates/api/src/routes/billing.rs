//! Billing API routes - exposes plan definitions, subscription status, and usage metering

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::ToSchema;

use billforge_billing::{
    quote_subscription, BillingConfig, BillingService, BillingServiceTrait, ModuleAddOn, Plan,
};
use billforge_core::Module;

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckoutRequest {
    plan_id: String,
    billing_cycle: Option<String>,
    #[serde(default)]
    add_on_modules: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct QuoteRequest {
    plan_id: String,
    #[serde(default)]
    add_on_modules: Vec<String>,
}

/// Create billing sub-router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/plans", get(list_plans))
        .route("/module-addons", get(list_module_addons))
        .route("/quote", post(quote_billing))
        .route("/subscription", get(get_subscription))
        .route("/usage", get(get_usage))
        .route("/checkout", post(create_checkout))
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

/// GET /billing/module-addons - return purchasable module add-on catalog
#[utoipa::path(
    get,
    path = "/api/v1/billing/module-addons",
    tag = "Billing",
    responses(
        (status = 200, description = "Available module add-ons"),
    )
)]
pub async fn list_module_addons() -> Json<Value> {
    Json(json!({
        "module_addons": ModuleAddOn::catalog(),
    }))
}

/// POST /billing/quote - price a base plan plus selected module add-ons
#[utoipa::path(
    post,
    path = "/api/v1/billing/quote",
    tag = "Billing",
    request_body = QuoteRequest,
    responses(
        (status = 200, description = "Subscription quote"),
        (status = 400, description = "Validation error"),
    )
)]
pub async fn quote_billing(Json(req): Json<QuoteRequest>) -> ApiResult<Json<Value>> {
    use billforge_billing::PlanId;

    let plan_id: PlanId = req
        .plan_id
        .parse()
        .map_err(|e| ApiError(billforge_core::Error::Validation(e)))?;
    let add_on_modules = parse_modules(&req.add_on_modules)?;
    let quote = quote_subscription(plan_id, &add_on_modules);

    Ok(Json(json!({
        "quote": quote,
    })))
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
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    let tenant_id = user.tenant_id;
    let pool = state.db.metadata();
    let service = BillingService::new(BillingConfig::default(), pool);
    let sub = service
        .get_subscription(&tenant_id)
        .await
        .unwrap_or_else(|_| billforge_billing::Subscription::new_free(tenant_id));
    Ok(Json(json!({
        "subscription": sub,
    })))
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

    let service = BillingService::new(BillingConfig::default(), state.db.metadata());
    let sub = service
        .get_subscription(&tenant_id)
        .await
        .unwrap_or_else(|_| billforge_billing::Subscription::new_free(tenant_id.clone()));

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

/// POST /billing/checkout - create a Stripe checkout session (or mock checkout)
#[utoipa::path(
    post,
    path = "/api/v1/billing/checkout",
    tag = "Billing",
    request_body = CheckoutRequest,
    responses(
        (status = 200, description = "Checkout session created"),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn create_checkout(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CheckoutRequest>,
) -> ApiResult<Json<Value>> {
    use billforge_billing::{BillingCycle, PlanId};

    let plan_id: PlanId = req
        .plan_id
        .parse()
        .map_err(|e| ApiError(billforge_core::Error::Validation(e)))?;

    let cycle = BillingCycle::from_str(req.billing_cycle.as_deref().unwrap_or("monthly"))
        .map_err(|e| ApiError(billforge_core::Error::Validation(e)))?;
    let add_on_modules = parse_modules(&req.add_on_modules)?;

    let service = BillingService::new(BillingConfig::from_env(), state.db.metadata());
    let base =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let outcome = service
        .create_checkout_with_modules(
            &user.tenant_id,
            &user.email,
            plan_id,
            cycle,
            &add_on_modules,
            &base,
        )
        .await?;

    Ok(Json(json!({
        "mode": outcome.mode,
        "url": outcome.url,
    })))
}

fn parse_modules(module_names: &[String]) -> ApiResult<Vec<Module>> {
    module_names
        .iter()
        .map(|module_name| {
            module_name
                .parse::<Module>()
                .map_err(|e| ApiError(billforge_core::Error::Validation(e)))
        })
        .collect()
}
