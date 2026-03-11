//! Analytics Module
//!
//! Provides user behavior tracking, feature usage metrics, performance analytics,
//! predictive forecasting, and anomaly detection.

pub mod models;
pub mod repository;
pub mod service;
pub mod handlers;
pub mod jobs;
pub mod predictive_models;
pub mod forecasting;
pub mod anomaly_detection;

pub use models::*;
pub use repository::AnalyticsRepository;
pub use service::AnalyticsService;
pub use handlers::create_router;
pub use predictive_models::*;
pub use forecasting::*;
pub use anomaly_detection::*;
