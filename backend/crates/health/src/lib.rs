//! Customer Health Scoring Module
//!
//! Automatically identifies at-risk customers and power users

#![allow(warnings)]

pub mod handlers;
pub mod models;
pub mod repository;
pub mod scoring;

pub use handlers::create_router;
