//! BillForge Billing & Subscription Management
//!
//! Handles subscription plans, billing cycles, and payment processing integration.

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod addons;
pub mod plans;
pub mod service;
pub mod stripe;
pub mod subscription;
pub mod usage;

pub use addons::{effective_features, quote_subscription, ModuleAddOn, SubscriptionQuote};
pub use plans::{Plan, PlanFeatures, PlanId, PlanTier};
pub use service::{BillingConfig, BillingService, BillingServiceTrait, CheckoutOutcome};
pub use subscription::{BillingCycle, Subscription, SubscriptionStatus};
pub use usage::{get_tenant_usage, record_invoice_meter_event};
