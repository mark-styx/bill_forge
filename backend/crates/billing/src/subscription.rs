//! Subscription management

use billforge_core::TenantId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::plans::PlanId;

/// Subscription ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriptionId(pub Uuid);

impl SubscriptionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Billing cycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingCycle {
    Monthly,
    Annual,
}

impl BillingCycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingCycle::Monthly => "monthly",
            BillingCycle::Annual => "annual",
        }
    }
}

/// Subscription status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// Active subscription
    Active,
    /// Subscription is in trial period
    Trialing,
    /// Payment failed, subscription at risk
    PastDue,
    /// Subscription canceled but still active until period end
    Canceled,
    /// Subscription has expired
    Expired,
    /// Subscription is paused (by user or admin)
    Paused,
}

impl SubscriptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubscriptionStatus::Active => "active",
            SubscriptionStatus::Trialing => "trialing",
            SubscriptionStatus::PastDue => "past_due",
            SubscriptionStatus::Canceled => "canceled",
            SubscriptionStatus::Expired => "expired",
            SubscriptionStatus::Paused => "paused",
        }
    }

    /// Check if the subscription allows access to features
    pub fn allows_access(&self) -> bool {
        matches!(
            self,
            SubscriptionStatus::Active | SubscriptionStatus::Trialing | SubscriptionStatus::PastDue
        )
    }
}

/// Tenant subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Subscription ID
    pub id: SubscriptionId,
    /// Tenant this subscription belongs to
    pub tenant_id: TenantId,
    /// Current plan
    pub plan_id: PlanId,
    /// Subscription status
    pub status: SubscriptionStatus,
    /// Billing cycle
    pub billing_cycle: BillingCycle,
    /// When the subscription started
    pub started_at: DateTime<Utc>,
    /// When the current period started
    pub current_period_start: DateTime<Utc>,
    /// When the current period ends
    pub current_period_end: DateTime<Utc>,
    /// When the subscription was canceled (if applicable)
    pub canceled_at: Option<DateTime<Utc>>,
    /// When the trial ends (if applicable)
    pub trial_end: Option<DateTime<Utc>>,
    /// Stripe subscription ID
    pub stripe_subscription_id: Option<String>,
    /// Stripe customer ID
    pub stripe_customer_id: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl Subscription {
    /// Create a new free subscription for a tenant
    pub fn new_free(tenant_id: TenantId) -> Self {
        let now = Utc::now();
        Self {
            id: SubscriptionId::new(),
            tenant_id,
            plan_id: PlanId::Free,
            status: SubscriptionStatus::Active,
            billing_cycle: BillingCycle::Monthly,
            started_at: now,
            current_period_start: now,
            current_period_end: now + chrono::Duration::days(365 * 100), // Effectively unlimited
            canceled_at: None,
            trial_end: None,
            stripe_subscription_id: None,
            stripe_customer_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new trial subscription
    pub fn new_trial(tenant_id: TenantId, plan_id: PlanId, trial_days: u32) -> Self {
        let now = Utc::now();
        let trial_end = now + chrono::Duration::days(trial_days as i64);
        Self {
            id: SubscriptionId::new(),
            tenant_id,
            plan_id,
            status: SubscriptionStatus::Trialing,
            billing_cycle: BillingCycle::Monthly,
            started_at: now,
            current_period_start: now,
            current_period_end: trial_end,
            canceled_at: None,
            trial_end: Some(trial_end),
            stripe_subscription_id: None,
            stripe_customer_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if the subscription is in trial
    pub fn is_trial(&self) -> bool {
        self.status == SubscriptionStatus::Trialing
    }

    /// Check if the subscription allows feature access
    pub fn is_active(&self) -> bool {
        self.status.allows_access()
    }

    /// Get days remaining in trial
    pub fn trial_days_remaining(&self) -> Option<i64> {
        self.trial_end.map(|end| {
            let now = Utc::now();
            (end - now).num_days().max(0)
        })
    }

    /// Get days until period end
    pub fn days_until_renewal(&self) -> i64 {
        let now = Utc::now();
        (self.current_period_end - now).num_days().max(0)
    }
}

/// Subscription usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionUsage {
    pub tenant_id: TenantId,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Number of users
    pub user_count: u32,
    /// Invoices processed this period
    pub invoices_count: u32,
    /// Vendors created
    pub vendor_count: u32,
    /// Storage used in bytes
    pub storage_bytes: u64,
}

impl SubscriptionUsage {
    /// Get storage used in GB
    pub fn storage_gb(&self) -> f64 {
        self.storage_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// Invoice for billing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingInvoice {
    /// Invoice ID
    pub id: String,
    /// Tenant ID
    pub tenant_id: TenantId,
    /// Subscription ID
    pub subscription_id: SubscriptionId,
    /// Amount in cents
    pub amount_cents: i64,
    /// Currency
    pub currency: String,
    /// Invoice status
    pub status: InvoiceStatus,
    /// Period start
    pub period_start: DateTime<Utc>,
    /// Period end
    pub period_end: DateTime<Utc>,
    /// Stripe invoice ID
    pub stripe_invoice_id: Option<String>,
    /// PDF URL
    pub pdf_url: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Paid timestamp
    pub paid_at: Option<DateTime<Utc>>,
}

/// Billing invoice status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_status_allows_access() {
        assert!(SubscriptionStatus::Active.allows_access());
        assert!(SubscriptionStatus::Trialing.allows_access());
        assert!(SubscriptionStatus::PastDue.allows_access());
        assert!(!SubscriptionStatus::Canceled.allows_access());
        assert!(!SubscriptionStatus::Expired.allows_access());
    }

    #[test]
    fn test_new_free_subscription() {
        let tenant_id = TenantId::new();
        let sub = Subscription::new_free(tenant_id.clone());

        assert_eq!(sub.tenant_id, tenant_id);
        assert_eq!(sub.plan_id, PlanId::Free);
        assert_eq!(sub.status, SubscriptionStatus::Active);
        assert!(sub.is_active());
        assert!(!sub.is_trial());
    }

    #[test]
    fn test_trial_subscription() {
        let tenant_id = TenantId::new();
        let sub = Subscription::new_trial(tenant_id, PlanId::Professional, 14);

        assert_eq!(sub.status, SubscriptionStatus::Trialing);
        assert!(sub.is_trial());
        assert!(sub.is_active());

        let days = sub.trial_days_remaining().unwrap();
        assert!(days >= 13 && days <= 14);
    }

    #[test]
    fn test_usage_storage_calculation() {
        let usage = SubscriptionUsage {
            tenant_id: TenantId::new(),
            period_start: Utc::now(),
            period_end: Utc::now(),
            user_count: 5,
            invoices_count: 100,
            vendor_count: 20,
            storage_bytes: 1024 * 1024 * 1024 * 2, // 2 GB
        };

        assert!((usage.storage_gb() - 2.0).abs() < 0.01);
    }
}
