//! Customer Feedback Module
//!
//! Provides feedback collection, aggregation, and sentiment analysis.

#![allow(warnings)]

pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use handlers::create_router;
pub use models::*;
pub use repository::FeedbackRepository;
pub use service::FeedbackService;
