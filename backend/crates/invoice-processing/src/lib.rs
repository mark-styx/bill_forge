//! Invoice Processing Module
//!
//! Workflow engine for invoice approval and routing.

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod categorization;
pub mod engine;
pub mod rules;

pub use categorization::{CategorizationEngine, CategorySuggestion, InvoiceCategorization};
pub use engine::WorkflowEngine;
