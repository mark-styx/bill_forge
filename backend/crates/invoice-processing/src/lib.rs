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
pub mod recurring_patterns;
pub mod rules;

pub use categorization::{
    CategorizationEngine, CategorySuggestion, InvoiceCategorization, LineCategorization,
    LineSplitSuggestion, PerLineInvoiceCategorization, PriorLineCoding, VendorHistory,
    HistoricalSplit, KeywordResult,
    categorize_line_by_keywords, collect_gl_signals, detect_historical_splits,
    detect_line_splits, find_matching_prior, apply_line_correction,
    persist_line_categorizations,
};
pub use categorization_ml::MLCategorizer;
pub use contract_matching::{match_invoice_to_contract, ContractMatchInput, ContractMatchOutcome};
pub use embedding_cache::{CacheStats, EmbeddingCache};
pub use engine::WorkflowEngine;
pub use recurring_patterns::{
    detect_or_update_pattern, evaluate_pattern_match, find_pattern, PatternMatchResult,
    RecurringPattern,
};
pub use feedback_loop::{
    AccuracyMetrics, CategorizationFeedback, CorrectionRule, FeedbackLearning, FeedbackType,
};
