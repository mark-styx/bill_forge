//! Winston AI Agent - Intelligent Invoice Assistant
//!
//! This crate provides AI-powered conversational assistance for invoice management,
//! powered by LangGraph/OpenAI integration.

#![allow(warnings)]

pub mod agent;
pub mod config;
pub mod context;
pub mod fake_provider;
pub mod handlers;
pub mod models;
pub mod openai_compatible_provider;
pub mod product_knowledge;
pub mod proposals;
pub mod provider;
pub mod tools;

pub mod issue_intake;

pub use agent::WinstonAgent;
pub use config::{AiModelConfig, AiProviderConfig, AiProviderType, ConfigError};
pub use fake_provider::FakeAiProvider;
pub use handlers::create_router;
pub use openai_compatible_provider::OpenAiCompatibleProvider;
pub use proposals::{CreateWinstonProposalInput, WinstonProposalService};
pub use provider::{AiProvider, ProviderChatStream};
