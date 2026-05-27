//! Billing service implementation

use async_trait::async_trait;
use billforge_core::{Error, Module, Result, TenantId};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::addons::effective_features;
use crate::plans::{Plan, PlanId};
use crate::stripe::{CreateCheckoutSessionParams, CreateCustomerParams, StripeClient};
use crate::subscription::{BillingCycle, Subscription, SubscriptionId, SubscriptionStatus};

/// Outcome of a checkout flow
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckoutOutcome {
    pub mode: String,
    pub url: String,
}

const SUBSCRIPTION_COLUMNS: &str = "id, tenant_id, plan_id, status, billing_cycle, add_on_modules, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at";

/// Billing configuration
#[derive(Debug, Clone)]
pub struct BillingConfig {
    /// Stripe API key
    pub stripe_api_key: Option<String>,
    /// Stripe webhook secret
    pub stripe_webhook_secret: Option<String>,
    /// Default trial days for paid plans
    pub default_trial_days: u32,
    /// Enable billing (false for development)
    pub enabled: bool,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            stripe_api_key: None,
            stripe_webhook_secret: None,
            default_trial_days: 14,
            enabled: false,
        }
    }
}

impl BillingConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            stripe_api_key: std::env::var("STRIPE_API_KEY").ok(),
            stripe_webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
            default_trial_days: std::env::var("DEFAULT_TRIAL_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(14),
            enabled: std::env::var("BILLING_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        }
    }
}

/// Billing service trait
#[async_trait]
pub trait BillingServiceTrait: Send + Sync {
    /// Get subscription for a tenant
    async fn get_subscription(&self, tenant_id: &TenantId) -> Result<Subscription>;

    /// Create a new subscription
    async fn create_subscription(
        &self,
        tenant_id: &TenantId,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
    ) -> Result<Subscription>;

    /// Upgrade/downgrade subscription
    async fn change_plan(&self, tenant_id: &TenantId, new_plan_id: PlanId) -> Result<Subscription>;

    /// Cancel subscription
    async fn cancel_subscription(&self, tenant_id: &TenantId) -> Result<Subscription>;

    /// Resume a canceled subscription
    async fn resume_subscription(&self, tenant_id: &TenantId) -> Result<Subscription>;

    /// Get all available plans
    fn get_plans(&self) -> Vec<Plan>;

    /// Check if a feature is available for a tenant
    async fn has_feature(&self, tenant_id: &TenantId, feature: &str) -> Result<bool>;
}

/// Billing service implementation
pub struct BillingService {
    config: BillingConfig,
    stripe: Option<Arc<StripeClient>>,
    pool: Arc<PgPool>,
}

impl BillingService {
    /// Create a new billing service
    pub fn new(config: BillingConfig, pool: Arc<PgPool>) -> Self {
        let stripe = if let Some(ref api_key) = config.stripe_api_key {
            Some(Arc::new(StripeClient::new(api_key.clone())))
        } else {
            warn!("Stripe API key not configured - billing will be in mock mode");
            None
        };

        Self {
            config,
            stripe,
            pool,
        }
    }

    /// Check if billing is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.stripe.is_some()
    }

    /// Get Stripe client
    pub fn stripe(&self) -> Option<Arc<StripeClient>> {
        self.stripe.clone()
    }

    /// Create a checkout session (or mock checkout) for a paid plan.
    pub async fn create_checkout(
        &self,
        tenant_id: &TenantId,
        email: &str,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
        base_url: &str,
    ) -> Result<CheckoutOutcome> {
        self.create_checkout_with_modules(tenant_id, email, plan_id, billing_cycle, &[], base_url)
            .await
    }

    /// Create a checkout session for a base plan plus optional module add-ons.
    pub async fn create_checkout_with_modules(
        &self,
        tenant_id: &TenantId,
        email: &str,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
        add_on_modules: &[Module],
        base_url: &str,
    ) -> Result<CheckoutOutcome> {
        if plan_id == PlanId::Free {
            let has_paid_addons = !add_on_modules.is_empty();
            if !has_paid_addons {
                return Err(Error::Validation(
                    "Free plan does not require checkout".to_string(),
                ));
            }
        }

        if !self.is_enabled() {
            // Mock mode: persist a trialing paid subscription and return a
            // dashboard redirect URL. Keeps the dev/demo flow working without
            // Stripe keys.
            self.create_subscription_with_modules(
                tenant_id,
                plan_id,
                billing_cycle,
                add_on_modules,
            )
            .await?;
            return Ok(CheckoutOutcome {
                mode: "mock".to_string(),
                url: format!("{}/dashboard?checkout=mock", base_url),
            });
        }

        // Stripe mode
        let stripe = self
            .stripe()
            .expect("stripe client must be present when billing is enabled");

        let plan = Plan::by_id(plan_id);
        let price_id = match billing_cycle {
            BillingCycle::Annual => plan.stripe_annual_price_id.as_deref(),
            BillingCycle::Monthly => plan.stripe_monthly_price_id.as_deref(),
        };
        let price_id = price_id
            .ok_or_else(|| Error::Validation(format!("No Stripe price ID for plan {}", plan_id)))?
            .to_string();

        let customer = stripe
            .create_customer(CreateCustomerParams {
                email: email.to_string(),
                name: None,
                metadata: HashMap::from([("tenant_id".to_string(), tenant_id.to_string())]),
            })
            .await?;

        let session = stripe
            .create_checkout_session(CreateCheckoutSessionParams {
                customer_id: customer.id.clone(),
                price_id,
                mode: "subscription".to_string(),
                success_url: format!("{}/dashboard?checkout=success", base_url),
                cancel_url: format!("{}/onboard?checkout=cancelled", base_url),
                metadata: HashMap::from([
                    ("tenant_id".to_string(), tenant_id.to_string()),
                    ("plan_id".to_string(), plan_id.to_string()),
                    (
                        "add_on_modules".to_string(),
                        add_on_modules
                            .iter()
                            .map(Module::as_str)
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ]),
            })
            .await?;

        Ok(CheckoutOutcome {
            mode: "stripe".to_string(),
            url: session.url,
        })
    }

    fn row_to_subscription(row: &sqlx::postgres::PgRow) -> Result<Subscription> {
        let plan_str: String = row
            .try_get("plan_id")
            .map_err(|e| Error::Database(e.to_string()))?;
        let status_str: String = row
            .try_get("status")
            .map_err(|e| Error::Database(e.to_string()))?;
        let cycle_str: String = row
            .try_get("billing_cycle")
            .map_err(|e| Error::Database(e.to_string()))?;
        let add_on_modules_json: sqlx::types::Json<serde_json::Value> = row
            .try_get("add_on_modules")
            .map_err(|e| Error::Database(e.to_string()))?;
        let add_on_modules: Vec<Module> =
            serde_json::from_value(add_on_modules_json.0).map_err(|e| {
                Error::Database(format!(
                    "Failed to parse subscription add-on modules: {}",
                    e
                ))
            })?;

        Ok(Subscription {
            id: SubscriptionId(
                row.try_get("id")
                    .map_err(|e| Error::Database(e.to_string()))?,
            ),
            tenant_id: TenantId::from_uuid(
                row.try_get("tenant_id")
                    .map_err(|e| Error::Database(e.to_string()))?,
            ),
            plan_id: plan_str.parse().map_err(Error::Database)?,
            status: SubscriptionStatus::from_str(&status_str).map_err(Error::Database)?,
            billing_cycle: BillingCycle::from_str(&cycle_str).map_err(Error::Database)?,
            add_on_modules,
            started_at: row
                .try_get("started_at")
                .map_err(|e| Error::Database(e.to_string()))?,
            current_period_start: row
                .try_get("current_period_start")
                .map_err(|e| Error::Database(e.to_string()))?,
            current_period_end: row
                .try_get("current_period_end")
                .map_err(|e| Error::Database(e.to_string()))?,
            canceled_at: row
                .try_get("canceled_at")
                .map_err(|e| Error::Database(e.to_string()))?,
            trial_end: row
                .try_get("trial_end")
                .map_err(|e| Error::Database(e.to_string()))?,
            stripe_subscription_id: row
                .try_get("stripe_subscription_id")
                .map_err(|e| Error::Database(e.to_string()))?,
            stripe_customer_id: row
                .try_get("stripe_customer_id")
                .map_err(|e| Error::Database(e.to_string()))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| Error::Database(e.to_string()))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| Error::Database(e.to_string()))?,
        })
    }

    pub async fn create_subscription_with_modules(
        &self,
        tenant_id: &TenantId,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
        add_on_modules: &[Module],
    ) -> Result<Subscription> {
        let mut subscription = self
            .build_subscription(tenant_id, plan_id, billing_cycle)
            .await?
            .with_add_on_modules(add_on_modules.to_vec());
        self.persist_subscription(&subscription).await?;
        self.sync_tenant_modules(tenant_id, plan_id, add_on_modules)
            .await?;
        subscription.add_on_modules = add_on_modules.to_vec();
        Ok(subscription)
    }

    async fn build_subscription(
        &self,
        tenant_id: &TenantId,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
    ) -> Result<Subscription> {
        let subscription = if plan_id == PlanId::Free {
            Subscription::new_free(tenant_id.clone())
        } else if self.is_enabled() {
            info!(
                tenant_id = %tenant_id,
                plan = %plan_id,
                cycle = ?billing_cycle,
                "Creating subscription via Stripe"
            );
            Subscription::new_trial(tenant_id.clone(), plan_id, self.config.default_trial_days)
        } else {
            info!(
                tenant_id = %tenant_id,
                plan = %plan_id,
                "Creating mock subscription (Stripe not enabled)"
            );
            Subscription::new_trial(tenant_id.clone(), plan_id, self.config.default_trial_days)
        };

        Ok(subscription.with_add_on_modules(vec![]))
    }

    async fn persist_subscription(&self, subscription: &Subscription) -> Result<()> {
        let add_on_modules_json = serde_json::to_value(&subscription.add_on_modules)
            .map_err(|e| Error::Database(e.to_string()))?;

        sqlx::query(
            r#"INSERT INTO tenant_subscriptions
                (id, tenant_id, plan_id, status, billing_cycle, add_on_modules, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
               ON CONFLICT (tenant_id) DO UPDATE SET
                plan_id = EXCLUDED.plan_id,
                status = EXCLUDED.status,
                billing_cycle = EXCLUDED.billing_cycle,
                add_on_modules = EXCLUDED.add_on_modules,
                started_at = EXCLUDED.started_at,
                current_period_start = EXCLUDED.current_period_start,
                current_period_end = EXCLUDED.current_period_end,
                canceled_at = EXCLUDED.canceled_at,
                trial_end = EXCLUDED.trial_end,
                stripe_subscription_id = EXCLUDED.stripe_subscription_id,
                stripe_customer_id = EXCLUDED.stripe_customer_id,
                updated_at = EXCLUDED.updated_at"#
        )
        .bind(subscription.id.0)
        .bind(subscription.tenant_id.as_uuid())
        .bind(subscription.plan_id.as_str())
        .bind(subscription.status.as_str())
        .bind(subscription.billing_cycle.as_str())
        .bind(&add_on_modules_json)
        .bind(subscription.started_at)
        .bind(subscription.current_period_start)
        .bind(subscription.current_period_end)
        .bind(subscription.canceled_at)
        .bind(subscription.trial_end)
        .bind(&subscription.stripe_subscription_id)
        .bind(&subscription.stripe_customer_id)
        .bind(subscription.created_at)
        .bind(subscription.updated_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    async fn sync_tenant_modules(
        &self,
        tenant_id: &TenantId,
        plan_id: PlanId,
        add_on_modules: &[Module],
    ) -> Result<()> {
        let modules_json =
            serde_json::to_value(effective_features(plan_id, add_on_modules).modules)
                .map_err(|e| Error::Database(e.to_string()))?;

        sqlx::query("UPDATE tenants SET enabled_modules = $1, updated_at = NOW() WHERE id = $2")
            .bind(&modules_json)
            .bind(tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl BillingServiceTrait for BillingService {
    async fn get_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let row = sqlx::query(&format!(
            "SELECT {} FROM tenant_subscriptions WHERE tenant_id = $1",
            SUBSCRIPTION_COLUMNS
        ))
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        match row {
            Some(row) => Self::row_to_subscription(&row),
            None => Ok(Subscription::new_free(tenant_id.clone())),
        }
    }

    async fn create_subscription(
        &self,
        tenant_id: &TenantId,
        plan_id: PlanId,
        billing_cycle: BillingCycle,
    ) -> Result<Subscription> {
        self.create_subscription_with_modules(tenant_id, plan_id, billing_cycle, &[])
            .await
    }

    async fn change_plan(&self, tenant_id: &TenantId, new_plan_id: PlanId) -> Result<Subscription> {
        let row = sqlx::query(&format!(
            r#"UPDATE tenant_subscriptions
               SET plan_id = $1, updated_at = NOW()
               WHERE tenant_id = $2
               RETURNING {}"#,
            SUBSCRIPTION_COLUMNS
        ))
        .bind(new_plan_id.as_str())
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Subscription".to_string(),
            id: tenant_id.to_string(),
        })?;

        let subscription = Self::row_to_subscription(&row)?;
        self.sync_tenant_modules(tenant_id, new_plan_id, &subscription.add_on_modules)
            .await?;

        info!(
            tenant_id = %tenant_id,
            new_plan = %new_plan_id,
            "Subscription plan changed"
        );

        Ok(subscription)
    }

    async fn cancel_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let row = sqlx::query(&format!(
            r#"UPDATE tenant_subscriptions
               SET status = 'canceled', canceled_at = NOW(), updated_at = NOW()
               WHERE tenant_id = $1
               RETURNING {}"#,
            SUBSCRIPTION_COLUMNS
        ))
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Subscription".to_string(),
            id: tenant_id.to_string(),
        })?;

        info!(tenant_id = %tenant_id, "Subscription canceled");

        Self::row_to_subscription(&row)
    }

    async fn resume_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        // First fetch to check precondition
        let existing = sqlx::query("SELECT status FROM tenant_subscriptions WHERE tenant_id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Subscription".to_string(),
                id: tenant_id.to_string(),
            })?;

        let status_str: String = existing
            .try_get("status")
            .map_err(|e| Error::Database(e.to_string()))?;
        let status = SubscriptionStatus::from_str(&status_str).map_err(Error::Database)?;
        if status != SubscriptionStatus::Canceled {
            return Err(Error::Validation(
                "Can only resume canceled subscriptions".to_string(),
            ));
        }

        let row = sqlx::query(&format!(
            r#"UPDATE tenant_subscriptions
               SET status = 'active', canceled_at = NULL, updated_at = NOW()
               WHERE tenant_id = $1
               RETURNING {}"#,
            SUBSCRIPTION_COLUMNS
        ))
        .bind(tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        info!(tenant_id = %tenant_id, "Subscription resumed");

        Self::row_to_subscription(&row)
    }

    fn get_plans(&self) -> Vec<Plan> {
        Plan::all_public()
    }

    async fn has_feature(&self, tenant_id: &TenantId, feature: &str) -> Result<bool> {
        let subscription = self.get_subscription(tenant_id).await?;
        let features = effective_features(subscription.plan_id, &subscription.add_on_modules);

        let has_feature = match feature {
            "advanced_ocr" => features.advanced_ocr,
            "api_access" => features.api_access,
            "custom_workflows" => features.custom_workflows,
            "priority_support" => features.priority_support,
            "sso" | "sso_enabled" => features.sso_enabled,
            "custom_branding" => features.custom_branding,
            module_name => module_name
                .parse::<Module>()
                .is_ok_and(|module| features.modules.contains(&module)),
        };

        Ok(has_feature && subscription.is_active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    struct TestDb {
        admin_pool: PgPool,
        pool: Arc<PgPool>,
        db_name: String,
    }

    impl TestDb {
        async fn new() -> Self {
            let admin_url = std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap_or_else(|_| "postgres://mark@localhost/postgres".to_string());
            let admin_pool = PgPoolOptions::new()
                .max_connections(1)
                .connect(&admin_url)
                .await
                .expect("connect to local postgres for billing tests");
            let db_name = format!("billforge_billing_test_{}", uuid::Uuid::new_v4().simple());

            sqlx::query(&format!(r#"CREATE DATABASE "{}""#, db_name))
                .execute(&admin_pool)
                .await
                .expect("create billing test database");

            let db_url = database_url_for(&admin_url, &db_name);
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .connect(&db_url)
                .await
                .expect("connect to billing test database");
            apply_test_schema(&pool).await;

            Self {
                admin_pool,
                pool: Arc::new(pool),
                db_name,
            }
        }

        async fn cleanup(self) {
            self.pool.close().await;
            sqlx::query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1",
            )
            .bind(&self.db_name)
            .execute(&self.admin_pool)
            .await
            .ok();
            sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{}""#, self.db_name))
                .execute(&self.admin_pool)
                .await
                .ok();
            self.admin_pool.close().await;
        }
    }

    fn database_url_for(admin_url: &str, db_name: &str) -> String {
        let (base, suffix) = admin_url
            .split_once('?')
            .map(|(base, query)| (base, format!("?{}", query)))
            .unwrap_or((admin_url, String::new()));
        let prefix = base
            .rsplit_once('/')
            .map(|(prefix, _)| prefix)
            .unwrap_or(base);
        format!("{}/{}{}", prefix, db_name, suffix)
    }

    async fn apply_test_schema(pool: &PgPool) {
        sqlx::query(
            r#"
            CREATE TABLE tenants (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT UNIQUE NOT NULL,
                settings JSONB NOT NULL DEFAULT '{}',
                enabled_modules JSONB NOT NULL DEFAULT '[]',
                is_active BOOLEAN NOT NULL DEFAULT true,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create billing test tenants table");

        sqlx::query(
            r#"
            CREATE TABLE tenant_subscriptions (
                id UUID PRIMARY KEY,
                tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE CASCADE,
                plan_id TEXT NOT NULL,
                status TEXT NOT NULL,
                billing_cycle TEXT NOT NULL,
                add_on_modules JSONB NOT NULL DEFAULT '[]',
                started_at TIMESTAMPTZ NOT NULL,
                current_period_start TIMESTAMPTZ NOT NULL,
                current_period_end TIMESTAMPTZ NOT NULL,
                canceled_at TIMESTAMPTZ NULL,
                trial_end TIMESTAMPTZ NULL,
                stripe_subscription_id TEXT NULL,
                stripe_customer_id TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create billing test subscriptions table");
    }

    async fn seed_tenant(pool: &PgPool, tenant_id: &TenantId) {
        // Ensure tenants table exists and insert a row for FK satisfaction.
        // The test pool already has all migrations applied, so just insert.
        sqlx::query("INSERT INTO tenants (id, name, slug, created_at, updated_at) VALUES ($1, 'test', $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING")
            .bind(tenant_id.as_uuid())
            .bind(format!("test-{}", tenant_id.as_uuid()))
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_get_default_subscription() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();

        let sub = service.get_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.plan_id, PlanId::Free);
        assert!(sub.is_active());
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_subscription() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        seed_tenant(&pool, &TenantId::new()).await; // warm up tenant table
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        let sub = service
            .create_subscription(&tenant_id, PlanId::Starter, BillingCycle::Monthly)
            .await
            .unwrap();

        assert_eq!(sub.plan_id, PlanId::Starter);
        assert_eq!(sub.status, SubscriptionStatus::Trialing);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_subscription_with_modules_persists_entitlements() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        let sub = service
            .create_subscription_with_modules(
                &tenant_id,
                PlanId::Starter,
                BillingCycle::Monthly,
                &[Module::InvoiceProcessing, Module::Reporting],
            )
            .await
            .unwrap();

        assert_eq!(
            sub.add_on_modules,
            vec![Module::InvoiceProcessing, Module::Reporting]
        );
        assert!(service
            .has_feature(&tenant_id, "invoice_processing")
            .await
            .unwrap());
        assert!(service.has_feature(&tenant_id, "reporting").await.unwrap());

        let modules: serde_json::Value =
            sqlx::query_scalar("SELECT enabled_modules FROM tenants WHERE id = $1")
                .bind(tenant_id.as_uuid())
                .fetch_one(&*pool)
                .await
                .unwrap();
        assert_eq!(
            modules,
            serde_json::json!([
                "invoice_capture",
                "vendor_management",
                "invoice_processing",
                "reporting"
            ])
        );

        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_change_plan() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        // Create initial subscription
        service
            .create_subscription(&tenant_id, PlanId::Starter, BillingCycle::Monthly)
            .await
            .unwrap();

        // Change to professional
        let sub = service
            .change_plan(&tenant_id, PlanId::Professional)
            .await
            .unwrap();

        assert_eq!(sub.plan_id, PlanId::Professional);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_cancel_subscription() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        service
            .create_subscription(&tenant_id, PlanId::Starter, BillingCycle::Monthly)
            .await
            .unwrap();

        let sub = service.cancel_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Canceled);
        assert!(sub.canceled_at.is_some());
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_has_feature() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        // Free plan - no advanced features
        let has_api = service.has_feature(&tenant_id, "api_access").await.unwrap();
        assert!(!has_api);

        // Upgrade to professional
        service
            .create_subscription(&tenant_id, PlanId::Professional, BillingCycle::Monthly)
            .await
            .unwrap();

        let has_api = service.has_feature(&tenant_id, "api_access").await.unwrap();
        assert!(has_api);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_subscription_persists_across_service_instances() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        let svc1 = BillingService::new(BillingConfig::default(), pool.clone());
        svc1.create_subscription(&tenant_id, PlanId::Professional, BillingCycle::Monthly)
            .await
            .unwrap();
        drop(svc1); // simulate server restart

        let svc2 = BillingService::new(BillingConfig::default(), pool.clone());
        let sub = svc2.get_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.plan_id, PlanId::Professional);
        assert_eq!(sub.status, SubscriptionStatus::Trialing);
        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_checkout_mock_mode() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let outcome = service
            .create_checkout(
                &tenant_id,
                "a@b.com",
                PlanId::Starter,
                BillingCycle::Monthly,
                "http://localhost:3000",
            )
            .await
            .unwrap();

        assert_eq!(outcome.mode, "mock");
        assert!(outcome.url.contains("/dashboard?checkout=mock"));

        // Verify the paid plan was persisted as trialing
        let sub = service.get_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.plan_id, PlanId::Starter);
        assert_eq!(sub.status, SubscriptionStatus::Trialing);

        db.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_checkout_rejects_free() {
        let db = TestDb::new().await;
        let pool = db.pool.clone();
        let tenant_id = TenantId::new();
        seed_tenant(&pool, &tenant_id).await;

        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let result = service
            .create_checkout(
                &tenant_id,
                "a@b.com",
                PlanId::Free,
                BillingCycle::Monthly,
                "http://localhost:3000",
            )
            .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Free plan does not require checkout"));

        db.cleanup().await;
    }
}
