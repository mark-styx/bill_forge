//! Public self-serve signup and sandbox promotion routes.
//!
//! POST /api/public/signup  - creates a sandbox tenant with seeded data, returns auth credentials.
//! POST /api/public/promote - flips a sandbox tenant to a paid plan (authenticated).

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use billforge_auth::{AuthService, PasswordService, ProvisionInput};
use billforge_billing::Plan;
use billforge_core::{Module, Role, TenantId};
use billforge_db::metadata_db::CreateUserInput;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Public routes mounted outside the authenticated middleware stack.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/signup", post(signup))
        .route("/plans", get(list_public_plans))
}

/// Authenticated route for promoting a sandbox tenant to paid.
pub fn promote_route() -> Router<AppState> {
    Router::new().route("/promote", post(promote))
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub company_name: String,
    pub password: String,
    pub name: Option<String>,
    pub plan_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub tenant_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub is_sandbox: bool,
}

#[derive(Debug, Deserialize)]
pub struct PromoteRequest {
    pub plan_id: String,
}

#[derive(Debug, Serialize)]
pub struct PromoteResponse {
    pub tenant_id: String,
    pub plan_id: String,
    pub is_sandbox: bool,
}

#[derive(Debug, Serialize)]
pub struct PublicPlan {
    pub id: String,
    pub name: String,
    pub description: String,
    pub monthly_price_cents: u64,
    pub annual_price_cents: u64,
    pub metered_invoice_unit_price_cents: u64,
    pub features: PlanFeaturesBrief,
}

#[derive(Debug, Serialize)]
pub struct PlanFeaturesBrief {
    pub max_users: u32,
    pub max_invoices_per_month: u32,
    pub max_vendors: u32,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/public/signup
///
/// Creates a sandbox tenant (is_sandbox=true), seeds sample data, creates an
/// admin user, and returns auto-login credentials.
async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> ApiResult<Json<SignupResponse>> {
    // Basic validation
    if req.email.is_empty() || req.company_name.is_empty() || req.password.is_empty() {
        return Err(ApiError(billforge_core::Error::Validation(
            "email, company_name, and password are required".to_string(),
        )));
    }

    if req.password.len() < 8 {
        return Err(ApiError(billforge_core::Error::Validation(
            "Password must be at least 8 characters".to_string(),
        )));
    }

    // Use the auth provision flow (creates tenant, user, tokens).
    let admin_name = req.name.unwrap_or_else(|| req.email.clone());
    let response = state
        .auth
        .provision(ProvisionInput {
            company_name: req.company_name.clone(),
            admin_email: req.email.clone(),
            admin_password: req.password.clone(),
            admin_name: admin_name.clone(),
            timezone: None,
            default_currency: None,
            ocr_provider: None,
            local_ocr_required: None,
        })
        .await
        .map_err(|e| match &e {
            billforge_core::Error::AlreadyExists { .. } => {
                ApiError(billforge_core::Error::Validation(
                    "An account with this email already exists".to_string(),
                ))
            }
            billforge_core::Error::Database(_) => {
                tracing::error!("Signup database error: {}", e);
                ApiError(billforge_core::Error::Validation(
                    "Unable to create account. Please try again.".to_string(),
                ))
            }
            _ => ApiError(e),
        })?;

    let tenant_id = response.tenant.id.clone();

    // Mark tenant as sandbox
    let database_url = std::env::var("DATABASE_URL_MIGRATIONS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("DATABASE_URL").unwrap_or_default());

    let metadata_db = billforge_db::MetadataDatabase::new(&database_url)
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to create metadata database: {}",
                e
            )))
        })?;

    metadata_db
        .set_tenant_sandbox(&tenant_id, true)
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to set sandbox flag: {}",
                e
            )))
        })?;

    // Create tenant database, run migrations, seed sample data
    match state.db.tenant(&tenant_id).await {
        Ok(pool) => {
            if let Err(e) = state.db.run_tenant_migrations(&pool).await {
                tracing::warn!(
                    "Failed to run tenant migrations for {}: {}",
                    tenant_id.as_str(),
                    e
                );
            }

            // Seed sandbox sample data
            if let Err(e) = seed_sandbox_tenant(&pool, &tenant_id).await {
                tracing::warn!(
                    "Failed to seed sandbox tenant {}: {}",
                    tenant_id.as_str(),
                    e
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create tenant database for {}: {}",
                tenant_id.as_str(),
                e
            );
        }
    }

    tracing::info!(
        tenant_id = %tenant_id.as_str(),
        "SANDBOX_SIGNUP completed"
    );

    Ok(Json(SignupResponse {
        tenant_id: tenant_id.to_string(),
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        is_sandbox: true,
    }))
}

/// POST /api/public/promote
///
/// Sets is_sandbox=false and updates the tenant plan. Requires authentication.
async fn promote(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(req): Json<PromoteRequest>,
) -> ApiResult<Json<PromoteResponse>> {
    let plan_id: billforge_billing::PlanId = req
        .plan_id
        .parse()
        .map_err(|e: String| ApiError(billforge_core::Error::Validation(e)))?;

    let tenant_id = user.tenant_id.clone();

    let database_url = std::env::var("DATABASE_URL_MIGRATIONS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("DATABASE_URL").unwrap_or_default());

    let metadata_db = billforge_db::MetadataDatabase::new(&database_url)
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to create metadata database: {}",
                e
            )))
        })?;

    // Verify tenant is currently a sandbox
    let is_sandbox = metadata_db
        .is_tenant_sandbox(&tenant_id)
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to check sandbox status: {}",
                e
            )))
        })?;

    if !is_sandbox {
        return Err(ApiError(billforge_core::Error::Validation(
            "Tenant is not a sandbox tenant".to_string(),
        )));
    }

    // Flip sandbox flag
    metadata_db
        .set_tenant_sandbox(&tenant_id, false)
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to clear sandbox flag: {}",
                e
            )))
        })?;

    // Update plan
    metadata_db
        .update_tenant_plan(&tenant_id, plan_id.as_str())
        .await
        .map_err(|e| {
            ApiError(billforge_core::Error::Database(format!(
                "Failed to update plan: {}",
                e
            )))
        })?;

    tracing::info!(
        tenant_id = %tenant_id.as_str(),
        plan = %plan_id.as_str(),
        "SANDBOX_PROMOTED"
    );

    Ok(Json(PromoteResponse {
        tenant_id: tenant_id.to_string(),
        plan_id: plan_id.as_str().to_string(),
        is_sandbox: false,
    }))
}

/// GET /api/public/plans - return public plan definitions for the pricing calculator.
async fn list_public_plans() -> ApiResult<Json<Vec<PublicPlan>>> {
    let plans = Plan::all_public()
        .into_iter()
        .map(|p| PublicPlan {
            id: p.id.as_str().to_string(),
            name: p.name,
            description: p.description,
            monthly_price_cents: p.monthly_price_cents,
            annual_price_cents: p.annual_price_cents,
            metered_invoice_unit_price_cents: p.metered_invoice_unit_price_cents,
            features: PlanFeaturesBrief {
                max_users: p.features.max_users,
                max_invoices_per_month: p.features.max_invoices_per_month,
                max_vendors: p.features.max_vendors,
            },
        })
        .collect();

    Ok(Json(plans))
}

// ---------------------------------------------------------------------------
// Sandbox seeding
// ---------------------------------------------------------------------------

/// Seed a small fixed set of sample data for self-serve sandbox tenants.
/// Runs inside the tenant database with RLS context already established.
async fn seed_sandbox_tenant(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
) -> Result<(), anyhow::Error> {
    let tenant_uuid = *tenant_id.as_uuid();

    // Seed 5 sample vendors
    let vendors = vec![
        (
            "Acme Corp",
            "business",
            "billing@acme.com",
            "+1-555-0100",
            "123 Industrial Way, Chicago, IL 60601",
            "active",
        ),
        (
            "TechSupplies Inc",
            "business",
            "ap@techsupplies.com",
            "+1-555-0101",
            "456 Tech Park Dr, San Jose, CA 95110",
            "active",
        ),
        (
            "Office Supplies Co",
            "business",
            "invoices@officesupplies.com",
            "+1-555-0102",
            "789 Commerce St, Dallas, TX 75201",
            "active",
        ),
        (
            "Cloud Services LLC",
            "business",
            "billing@cloudservices.io",
            "+1-555-0103",
            "321 Server Lane, Seattle, WA 98101",
            "active",
        ),
        (
            "Marketing Agency Pro",
            "contractor",
            "invoices@marketingpro.co",
            "+1-555-0104",
            "555 Creative Blvd, Austin, TX 78701",
            "active",
        ),
    ];

    for (name, vtype, email, phone, address, status) in &vendors {
        sqlx::query(
            "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, address_line1, status, created_at, updated_at)
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, NOW(), NOW())"
        )
        .bind(tenant_uuid)
        .bind(name)
        .bind(vtype)
        .bind(email)
        .bind(phone)
        .bind(address)
        .bind(status)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed vendor {}: {}", name, e))?;
    }

    // Get admin user id for created_by field
    let admin_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 LIMIT 1")
            .bind(tenant_uuid)
            .fetch_optional(pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get admin user: {}", e))?;

    let admin_uuid = admin_id.unwrap_or_else(uuid::Uuid::new_v4);

    // Seed 10 invoices across statuses
    let invoices: Vec<(&str, &str, i64, &str, &str, &str)> = vec![
        (
            "INV-001",
            "Acme Corp",
            125000,
            "ready_for_review",
            "submitted",
            "Office equipment order",
        ),
        (
            "INV-002",
            "TechSupplies Inc",
            45600,
            "ready_for_review",
            "submitted",
            "Network hardware",
        ),
        (
            "INV-003",
            "Cloud Services LLC",
            234500,
            "reviewed",
            "pending_approval",
            "Monthly cloud services",
        ),
        (
            "INV-004",
            "Marketing Agency Pro",
            89000,
            "reviewed",
            "pending_approval",
            "Q1 campaign work",
        ),
        (
            "INV-005",
            "Office Supplies Co",
            12500,
            "reviewed",
            "approved",
            "Office restock",
        ),
        (
            "INV-006",
            "Acme Corp",
            67800,
            "reviewed",
            "approved",
            "Widget delivery",
        ),
        (
            "INV-007",
            "Cloud Services LLC",
            156000,
            "reviewed",
            "paid",
            "December infrastructure",
        ),
        (
            "INV-008",
            "TechSupplies Inc",
            34000,
            "reviewed",
            "paid",
            "Laptop accessories",
        ),
        (
            "INV-009",
            "Acme Corp",
            210000,
            "reviewed",
            "on_hold",
            "Disputed delivery",
        ),
        (
            "INV-010",
            "Office Supplies Co",
            8900,
            "ready_for_review",
            "submitted",
            "Stationery order",
        ),
    ];

    for (inv_num, vendor_name, amount, capture_status, processing_status, notes) in &invoices {
        let doc_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, currency, invoice_date, due_date, capture_status, processing_status, notes, document_id, created_by, created_at, updated_at)
             VALUES (gen_random_uuid(), $1, $2, $3, $4, 'USD', CURRENT_DATE - INTERVAL '5 days', CURRENT_DATE + INTERVAL '25 days', $5, $6, $7, $8, $9, NOW(), NOW())"
        )
        .bind(tenant_uuid)
        .bind(vendor_name)
        .bind(inv_num)
        .bind(*amount)
        .bind(capture_status)
        .bind(processing_status)
        .bind(notes)
        .bind(doc_id)
        .bind(admin_uuid)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed invoice {}: {}", inv_num, e))?;
    }

    // Seed 1 default approval policy
    sqlx::query(
        "INSERT INTO approval_policies (id, tenant_id, name, description, rules, is_active, created_at, updated_at)
         VALUES (gen_random_uuid(), $1, 'Default Approval Policy', 'Standard two-tier approval based on invoice amount',
                 '[{\"field\":\"amount\",\"operator\":\"greater_than\",\"value\":100000,\"approver\":{\"type\":\"role\",\"value\":\"tenant_admin\"}}]'::jsonb,
                 true, NOW(), NOW())
         ON CONFLICT DO NOTHING"
    )
    .bind(tenant_uuid)
    .execute(pool)
    .await
    .map_err(|e| anyhow::anyhow!("Failed to seed approval policy: {}", e))?;

    tracing::info!(
        tenant_id = %tenant_id.as_str(),
        "Seeded sandbox tenant with sample data"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_public_plans_includes_expected_tiers() {
        let plans = Plan::all_public();
        assert!(
            plans.len() >= 3,
            "Should have at least Free, Starter, Professional"
        );

        let free = plans
            .iter()
            .find(|p| p.id == billforge_billing::PlanId::Free);
        assert!(free.is_some(), "Free plan should be public");

        let starter = plans
            .iter()
            .find(|p| p.id == billforge_billing::PlanId::Starter);
        assert!(starter.is_some(), "Starter plan should be public");

        let pro = plans
            .iter()
            .find(|p| p.id == billforge_billing::PlanId::Professional);
        assert!(pro.is_some(), "Professional plan should be public");
    }

    #[test]
    fn test_plan_price_calculation() {
        // Starter: $49/mo + $1.50/invoice
        let starter = Plan::starter();
        assert_eq!(starter.monthly_price_cents, 4900);
        assert_eq!(starter.metered_invoice_unit_price_cents, 150);

        // Professional: $149/mo + $1.00/invoice
        let pro = Plan::professional();
        assert_eq!(pro.monthly_price_cents, 14900);
        assert_eq!(pro.metered_invoice_unit_price_cents, 100);

        // Verify estimate for 200 invoices/mo on Starter: $49 + 200 * $1.50 = $349
        let estimate = starter.monthly_price_cents + 200 * starter.metered_invoice_unit_price_cents;
        assert_eq!(estimate, 34900); // $349.00
    }

    #[test]
    fn test_recommend_plan_free() {
        // Under 25 invoices, 1 seat -> Free
        let plan = recommend_plan(10, 1);
        assert_eq!(plan.id, billforge_billing::PlanId::Free);
    }

    #[test]
    fn test_recommend_plan_starter() {
        // 50 invoices, 2 seats -> Starter
        let plan = recommend_plan(50, 2);
        assert_eq!(plan.id, billforge_billing::PlanId::Starter);
    }

    #[test]
    fn test_recommend_plan_professional() {
        // 500 invoices, 5 seats -> Professional
        let plan = recommend_plan(500, 5);
        assert_eq!(plan.id, billforge_billing::PlanId::Professional);
    }

    #[test]
    fn test_recommend_plan_enterprise() {
        // 2000 invoices or 15 seats -> Enterprise
        let plan = recommend_plan(2000, 5);
        assert_eq!(plan.id, billforge_billing::PlanId::Enterprise);

        let plan = recommend_plan(100, 15);
        assert_eq!(plan.id, billforge_billing::PlanId::Enterprise);
    }

    /// Pure function that maps invoice volume and seat count to a recommended plan.
    /// Mirrors the PlanId tier boundaries defined in plans.rs.
    fn recommend_plan(monthly_invoices: u32, seats: u32) -> billforge_billing::Plan {
        if seats > 10 || monthly_invoices > 1000 {
            Plan::enterprise()
        } else if seats > 3 || monthly_invoices > 100 {
            Plan::professional()
        } else if monthly_invoices > 25 || seats > 1 {
            Plan::starter()
        } else {
            Plan::free()
        }
    }
}
