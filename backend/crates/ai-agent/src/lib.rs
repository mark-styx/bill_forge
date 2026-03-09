//! Winston AI Agent - Intelligent Invoice Assistant
//!
//! This crate provides AI-powered conversational assistance for invoice management,
//! powered by LangGraph/OpenAI integration.

pub mod agent;
pub mod context;
pub mod tools;
pub mod handlers;
pub mod models;

pub use agent::WinstonAgent;
pub use handlers::create_router;
