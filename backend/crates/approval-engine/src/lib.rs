//! BillForge Approval Engine
//!
//! Multi-level approval chain engine with:
//! - Policy-based routing (amount thresholds, department, vendor matching)
//! - Sequential or parallel approval levels
//! - Escalation on timeout
//! - Delegation support
//! - Activity audit logging
//! - Auto-approve below configurable thresholds

pub mod engine;
pub mod error;
pub mod policy;
pub mod types;

pub use engine::ApprovalEngine;
pub use error::ApprovalError;
pub use policy::PolicyService;
pub use types::*;
