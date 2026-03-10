//! Invoice Processing Module
//!
//! Workflow engine for invoice approval and routing.
//! Sprint 13: ML-based categorization with OpenAI embeddings

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod categorization;
pub mod categorization_ml;
pub mod embedding_cache;
pub mod engine;
pub mod feedback_loop;
pub mod rules;

pub use categorization::{CategorizationEngine, CategorySuggestion, InvoiceCategorization};
pub use categorization_ml::MLCategorizer;
pub use embedding_cache::{CacheStats, EmbeddingCache};
pub use engine::WorkflowEngine;
pub use feedback_loop::{AccuracyMetrics, CategorizationFeedback, FeedbackLearning, FeedbackType};
