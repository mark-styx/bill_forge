//! Winston AI Agent - Intelligent Invoice Assistant
//!
//! This crate provides AI-powered conversational assistance for invoice management,
//! powered by LangGraph/OpenAI integration.

pub mod agent;
pub mod config;
pub mod context;
pub mod tools;
pub mod handlers;
pub mod models;
pub mod provider;
pub mod fake_provider;
pub mod openai_compatible_provider;

pub use agent::WinstonAgent;
pub use config::{AiModelConfig, AiProviderConfig, AiProviderType, ConfigError};
pub use handlers::create_router;
pub use provider::{AiProvider, ProviderChatStream};
pub use fake_provider::FakeAiProvider;
pub use openai_compatible_provider::OpenAiCompatibleProvider;
