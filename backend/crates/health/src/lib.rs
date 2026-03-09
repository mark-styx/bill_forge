//! Customer Health Scoring Module
//!
//! Automatically identifies at-risk customers and power users

pub mod models;
pub mod scoring;
pub mod repository;
pub mod handlers;

pub use handlers::create_router;
