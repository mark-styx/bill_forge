//! Invoice Processing Module
//!
//! Workflow engine for invoice approval and routing.
//! Sprint 13: ML-based categorization with OpenAI embeddings

pub mod categorization;
pub mod categorization_ml;
pub mod contract_matching;
pub mod embedding_cache;
pub mod engine;
pub mod feedback_loop;
pub mod rules;

pub use categorization::{CategorizationEngine, CategorySuggestion, InvoiceCategorization};
pub use categorization_ml::MLCategorizer;
pub use contract_matching::{match_invoice_to_contract, ContractMatchInput, ContractMatchOutcome};
pub use embedding_cache::{CacheStats, EmbeddingCache};
pub use engine::WorkflowEngine;
pub use feedback_loop::{
    AccuracyMetrics, CategorizationFeedback, CorrectionRule, FeedbackLearning, FeedbackType,
};
