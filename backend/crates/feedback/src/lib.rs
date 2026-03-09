//! Customer Feedback Module
//!
//! Provides feedback collection, aggregation, and sentiment analysis.

pub mod models;
pub mod repository;
pub mod service;
pub mod handlers;

pub use models::*;
pub use repository::FeedbackRepository;
pub use service::FeedbackService;
pub use handlers::create_router;
