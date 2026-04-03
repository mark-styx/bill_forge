//! BillForge Billing & Subscription Management
//!
//! Handles subscription plans, billing cycles, and payment processing integration.

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod plans;
pub mod service;
pub mod stripe;
pub mod subscription;

pub use plans::{Plan, PlanFeatures, PlanId, PlanTier};
pub use service::{BillingConfig, BillingService};
pub use subscription::{BillingCycle, Subscription, SubscriptionStatus};
