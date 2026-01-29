//! Billing service implementation

use async_trait::async_trait;
use billforge_core::{Error, Result, TenantId};
use std::sync::Arc;
use tracing::{info, warn};

use crate::plans::{Plan, PlanId};
use crate::subscription::{BillingCycle, Subscription, SubscriptionStatus};
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
    // In production, this would be backed by a database
    // For now, we use a simple in-memory store
    subscriptions: tokio::sync::RwLock<std::collections::HashMap<String, Subscription>>,
}

impl BillingService {
    /// Create a new billing service
    pub fn new(config: BillingConfig) -> Self {
        let stripe = if let Some(ref api_key) = config.stripe_api_key {
            Some(Arc::new(StripeClient::new(api_key.clone())))
        } else {
            warn!("Stripe API key not configured - billing will be in mock mode");
            None
        };

        Self {
            config,
            stripe,
            subscriptions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
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
}

#[async_trait]
impl BillingServiceTrait for BillingService {
    async fn get_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let subscriptions = self.subscriptions.read().await;

        if let Some(sub) = subscriptions.get(&tenant_id.to_string()) {
            Ok(sub.clone())
        } else {
            // Return a default free subscription if none exists
            Ok(Subscription::new_free(tenant_id.clone()))
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
            // In production, this would create a Stripe subscription
            info!(
                tenant_id = %tenant_id,
                plan = %plan_id,
                cycle = ?billing_cycle,
                "Creating subscription via Stripe"
            );

            // Create trial subscription
            Subscription::new_trial(tenant_id.clone(), plan_id, self.config.default_trial_days)
        } else {
            // Mock mode - create trial subscription without Stripe
            info!(
                tenant_id = %tenant_id,
                plan = %plan_id,
                "Creating mock subscription (Stripe not enabled)"
            );
            Subscription::new_trial(tenant_id.clone(), plan_id, self.config.default_trial_days)
        };

        // Store subscription
        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.insert(tenant_id.to_string(), subscription.clone());

        Ok(subscription)
    }

    async fn change_plan(
        &self,
        tenant_id: &TenantId,
        new_plan_id: PlanId,
    ) -> Result<Subscription> {
        let mut subscriptions = self.subscriptions.write().await;

        let subscription = subscriptions
            .get_mut(&tenant_id.to_string())
            .ok_or_else(|| Error::NotFound {
                resource_type: "Subscription".to_string(),
                id: tenant_id.to_string(),
            })?;

        let old_plan = subscription.plan_id;
        subscription.plan_id = new_plan_id;
        subscription.updated_at = chrono::Utc::now();

        info!(
            tenant_id = %tenant_id,
            old_plan = %old_plan,
            new_plan = %new_plan_id,
            "Subscription plan changed"
        );

        Ok(subscription.clone())
    }

    async fn cancel_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let mut subscriptions = self.subscriptions.write().await;

        let subscription = subscriptions
            .get_mut(&tenant_id.to_string())
            .ok_or_else(|| Error::NotFound {
                resource_type: "Subscription".to_string(),
                id: tenant_id.to_string(),
            })?;

        subscription.status = SubscriptionStatus::Canceled;
        subscription.canceled_at = Some(chrono::Utc::now());
        subscription.updated_at = chrono::Utc::now();

        info!(tenant_id = %tenant_id, "Subscription canceled");

        Ok(subscription.clone())
    }

    async fn resume_subscription(&self, tenant_id: &TenantId) -> Result<Subscription> {
        let mut subscriptions = self.subscriptions.write().await;

        let subscription = subscriptions
            .get_mut(&tenant_id.to_string())
            .ok_or_else(|| Error::NotFound {
                resource_type: "Subscription".to_string(),
                id: tenant_id.to_string(),
            })?;

        if subscription.status != SubscriptionStatus::Canceled {
            return Err(Error::Validation(
                "Can only resume canceled subscriptions".to_string(),
            ));
        }

        subscription.status = SubscriptionStatus::Active;
        subscription.canceled_at = None;
        subscription.updated_at = chrono::Utc::now();

        info!(tenant_id = %tenant_id, "Subscription resumed");

        Ok(subscription.clone())
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

    #[tokio::test]
    async fn test_get_default_subscription() {
        let service = BillingService::new(BillingConfig::default());
        let tenant_id = TenantId::new();

        let sub = service.get_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.plan_id, PlanId::Free);
        assert!(sub.is_active());
    }

    #[tokio::test]
    async fn test_create_subscription() {
        let service = BillingService::new(BillingConfig::default());
        let tenant_id = TenantId::new();

        let sub = service
            .create_subscription(&tenant_id, PlanId::Starter, BillingCycle::Monthly)
            .await
            .unwrap();

        assert_eq!(sub.plan_id, PlanId::Starter);
        assert_eq!(sub.status, SubscriptionStatus::Trialing);
    }

    #[tokio::test]
    async fn test_change_plan() {
        let service = BillingService::new(BillingConfig::default());
        let tenant_id = TenantId::new();

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

    #[tokio::test]
    async fn test_cancel_subscription() {
        let service = BillingService::new(BillingConfig::default());
        let tenant_id = TenantId::new();

        service
            .create_subscription(&tenant_id, PlanId::Starter, BillingCycle::Monthly)
            .await
            .unwrap();

        let sub = service.cancel_subscription(&tenant_id).await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Canceled);
        assert!(sub.canceled_at.is_some());
    }

    #[tokio::test]
    async fn test_has_feature() {
        let service = BillingService::new(BillingConfig::default());
        let tenant_id = TenantId::new();

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
}
