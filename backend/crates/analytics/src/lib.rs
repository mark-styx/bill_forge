//! Analytics Module
//!
//! Provides user behavior tracking, feature usage metrics, and performance analytics.

pub mod models;
pub mod repository;
pub mod service;
pub mod handlers;
pub mod jobs;

pub use models::*;
pub use repository::AnalyticsRepository;
pub use service::AnalyticsService;
pub use handlers::create_router;
