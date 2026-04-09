//! BillForge Billing & Subscription Management
//!
//! Handles subscription plans, billing cycles, and payment processing integration.

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod addons;
pub mod plans;
pub mod subscription;
pub mod service;
pub mod stripe;

pub use addons::{ModuleAddOn, SubscriptionQuote, effective_features, quote_subscription};
pub use plans::{Plan, PlanFeatures, PlanId, PlanTier};
pub use subscription::{Subscription, SubscriptionStatus, BillingCycle};
pub use service::{BillingService, BillingConfig, BillingServiceTrait};
