//! Billing service implementation

use async_trait::async_trait;
use billforge_core::{Error, Result, TenantId};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{info, warn};

use crate::plans::{Plan, PlanId};
use crate::subscription::{BillingCycle, Subscription, SubscriptionId, SubscriptionStatus};
use crate::stripe::StripeClient;

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
    async fn change_plan(
        &self,
        tenant_id: &TenantId,
        new_plan_id: PlanId,
    ) -> Result<Subscription>;

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

    fn row_to_subscription(row: &sqlx::postgres::PgRow) -> Result<Subscription> {
        let plan_str: String = row.try_get("plan_id").map_err(|e| Error::Database(e.to_string()))?;
        let status_str: String = row.try_get("status").map_err(|e| Error::Database(e.to_string()))?;
        let cycle_str: String = row.try_get("billing_cycle").map_err(|e| Error::Database(e.to_string()))?;

        Ok(Subscription {
            id: SubscriptionId(row.try_get("id").map_err(|e| Error::Database(e.to_string()))?),
            tenant_id: TenantId::from_uuid(row.try_get("tenant_id").map_err(|e| Error::Database(e.to_string()))?),
            plan_id: plan_str.parse().map_err(|e| Error::Database(e))?,
            status: SubscriptionStatus::from_str(&status_str).map_err(|e| Error::Database(e))?,
            billing_cycle: BillingCycle::from_str(&cycle_str).map_err(|e| Error::Database(e))?,
            started_at: row.try_get("started_at").map_err(|e| Error::Database(e.to_string()))?,
            current_period_start: row.try_get("current_period_start").map_err(|e| Error::Database(e.to_string()))?,
            current_period_end: row.try_get("current_period_end").map_err(|e| Error::Database(e.to_string()))?,
            canceled_at: row.try_get("canceled_at").map_err(|e| Error::Database(e.to_string()))?,
            trial_end: row.try_get("trial_end").map_err(|e| Error::Database(e.to_string()))?,
            stripe_subscription_id: row.try_get("stripe_subscription_id").map_err(|e| Error::Database(e.to_string()))?,
            stripe_customer_id: row.try_get("stripe_customer_id").map_err(|e| Error::Database(e.to_string()))?,
            created_at: row.try_get("created_at").map_err(|e| Error::Database(e.to_string()))?,
            updated_at: row.try_get("updated_at").map_err(|e| Error::Database(e.to_string()))?,
        })
    }
}

#[async_trait]
impl BillingServiceTrait for BillingService {
    async fn get_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let row = sqlx::query(
            "SELECT id, tenant_id, plan_id, status, billing_cycle, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at FROM tenant_subscriptions WHERE tenant_id = $1"
        )
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

        sqlx::query(
            r#"INSERT INTO tenant_subscriptions
                (id, tenant_id, plan_id, status, billing_cycle, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
               ON CONFLICT (tenant_id) DO UPDATE SET
                plan_id = EXCLUDED.plan_id,
                status = EXCLUDED.status,
                billing_cycle = EXCLUDED.billing_cycle,
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
        .bind(tenant_id.as_uuid())
        .bind(plan_id.as_str())
        .bind(subscription.status.as_str())
        .bind(billing_cycle.as_str())
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

        Ok(subscription)
    }

    async fn change_plan(
        &self,
        tenant_id: &TenantId,
        new_plan_id: PlanId,
    ) -> Result<Subscription> {
        let row = sqlx::query(
            r#"UPDATE tenant_subscriptions
               SET plan_id = $1, updated_at = NOW()
               WHERE tenant_id = $2
               RETURNING id, tenant_id, plan_id, status, billing_cycle, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at"#
        )
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

        info!(
            tenant_id = %tenant_id,
            new_plan = %new_plan_id,
            "Subscription plan changed"
        );

        Ok(subscription)
    }

    async fn cancel_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let row = sqlx::query(
            r#"UPDATE tenant_subscriptions
               SET status = 'canceled', canceled_at = NOW(), updated_at = NOW()
               WHERE tenant_id = $1
               RETURNING id, tenant_id, plan_id, status, billing_cycle, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at"#
        )
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
        let existing = sqlx::query(
            "SELECT status FROM tenant_subscriptions WHERE tenant_id = $1"
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Subscription".to_string(),
            id: tenant_id.to_string(),
        })?;

        let status_str: String = existing.try_get("status").map_err(|e| Error::Database(e.to_string()))?;
        let status = SubscriptionStatus::from_str(&status_str).map_err(|e| Error::Database(e))?;
        if status != SubscriptionStatus::Canceled {
            return Err(Error::Validation(
                "Can only resume canceled subscriptions".to_string(),
            ));
        }

        let row = sqlx::query(
            r#"UPDATE tenant_subscriptions
               SET status = 'active', canceled_at = NULL, updated_at = NOW()
               WHERE tenant_id = $1
               RETURNING id, tenant_id, plan_id, status, billing_cycle, started_at, current_period_start, current_period_end, canceled_at, trial_end, stripe_subscription_id, stripe_customer_id, created_at, updated_at"#
        )
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
        let plan = Plan::by_id(subscription.plan_id);

        let has_feature = match feature {
            "advanced_ocr" => plan.features.advanced_ocr,
            "api_access" => plan.features.api_access,
            "custom_workflows" => plan.features.custom_workflows,
            "priority_support" => plan.features.priority_support,
            "sso" | "sso_enabled" => plan.features.sso_enabled,
            "custom_branding" => plan.features.custom_branding,
            _ => false,
        };

        Ok(has_feature && subscription.is_active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

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

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_default_subscription(pool: PgPool) {
        let pool = Arc::new(pool);
        let service = BillingService::new(BillingConfig::default(), pool.clone());
        let tenant_id = TenantId::new();

        let sub = service.get_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.plan_id, PlanId::Free);
        assert!(sub.is_active());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_create_subscription(pool: PgPool) {
        let pool = Arc::new(pool);
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
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_change_plan(pool: PgPool) {
        let pool = Arc::new(pool);
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
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_cancel_subscription(pool: PgPool) {
        let pool = Arc::new(pool);
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
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_has_feature(pool: PgPool) {
        let pool = Arc::new(pool);
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
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_subscription_persists_across_service_instances(pool: PgPool) {
        let pool = Arc::new(pool);
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
    }
}
