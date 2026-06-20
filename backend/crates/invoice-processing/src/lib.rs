//! Invoice Processing Module
//!
//! Workflow engine for invoice approval and routing.
//! Sprint 13: ML-based categorization with OpenAI embeddings

pub mod categorization;
pub mod categorization_ml;
pub mod continuous_learning;
pub mod contract_matching;
pub mod embedding_cache;
pub mod engine;
pub mod feedback_loop;
pub mod recurring_patterns;
pub mod rules;

pub use categorization::{
    apply_line_correction, categorize_line_by_keywords, collect_gl_signals,
    detect_historical_splits, detect_line_splits, find_matching_prior,
    persist_line_categorizations, CategorizationEngine, CategorySuggestion, HistoricalSplit,
    InvoiceCategorization, KeywordResult, LineCategorization, LineSplitSuggestion,
    PerLineInvoiceCategorization, PriorLineCoding, VendorHistory,
};
pub use categorization_ml::MLCategorizer;
pub use continuous_learning::{
    ContinuousLearningEngine, CorrectionType, CorrectionsByKind, ModelChange, RoutingShift,
    TopRecategorization, WeeklyInsights, WeeklyLearningSummary,
};
pub use contract_matching::{match_invoice_to_contract, ContractMatchInput, ContractMatchOutcome};
pub use embedding_cache::{CacheStats, EmbeddingCache};
pub use engine::WorkflowEngine;
pub use feedback_loop::{
    AccuracyMetrics, CategorizationFeedback, CorrectionRule, FeedbackLearning, FeedbackType,
};
pub use recurring_patterns::{
    detect_or_update_pattern, evaluate_pattern_match, find_pattern, PatternMatchResult,
    RecurringPattern,
};
