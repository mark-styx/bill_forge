//! AI provider configuration parsed from environment variables.
//!
//! Centralises all Winston AI env-var consumption so provider adapters and the
//! agent itself share a single, validated source of truth. Empty strings are
//! trimmed to `None`, numeric fields are validated, and unsupported provider
//! types produce a typed error.

use std::env;
use std::fmt;
use std::time::Duration;

/// Serialises tests that mutate AI config environment variables.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

// ---------------------------------------------------------------------------
// Environment variable names
// ---------------------------------------------------------------------------

mod env_keys {
    pub const PROVIDER_TYPE: &str = "WINSTON_AI_PROVIDER_TYPE";
    pub const BASE_URL: &str = "WINSTON_AI_BASE_URL";
    pub const API_KEY: &str = "WINSTON_AI_API_KEY";
    pub const CHAT_MODEL: &str = "WINSTON_AI_CHAT_MODEL";
    /// Legacy alias honoured when `WINSTON_AI_CHAT_MODEL` is unset.
    pub const MODEL_LEGACY: &str = "WINSTON_AI_MODEL";
    pub const EMBEDDING_MODEL: &str = "WINSTON_AI_EMBEDDING_MODEL";
    pub const TIMEOUT_SECONDS: &str = "WINSTON_AI_TIMEOUT_SECONDS";
    pub const MAX_TOKENS: &str = "WINSTON_AI_MAX_TOKENS";
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error produced when provider configuration is invalid.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// The `WINSTON_AI_PROVIDER_TYPE` value is not a recognised variant.
    UnsupportedProviderType { value: String },
    /// A numeric env var could not be parsed.
    InvalidNumeric { key: &'static str, value: String, source: String },
    /// A numeric env var is syntactically valid but outside the allowed range.
    OutOfRange { key: &'static str, value: u64, max: u64 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProviderType { value } => {
                write!(f, "unsupported AI provider type: {:?}", value)
            }
            Self::InvalidNumeric { key, value, source } => {
                write!(
                    f,
                    "invalid numeric value for {}: {:?} ({})",
                    key, value, source
                )
            }
            Self::OutOfRange { key, value, max } => {
                write!(
                    f,
                    "value {} for {} exceeds maximum allowed {}",
                    value, key, max
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

// ---------------------------------------------------------------------------
// Provider type
// ---------------------------------------------------------------------------

/// Supported AI provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProviderType {
    /// Any OpenAI-compatible chat completions endpoint (e.g. GLM proxy).
    OpenAiCompatible,
}

impl AiProviderType {
    /// Parse from the string value stored in `WINSTON_AI_PROVIDER_TYPE`.
    ///
    /// Case-insensitive. Returns `None` for unrecognised values.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "openai_compatible"
            | "openai-compatible"
            | "openai"
            | "glm_proxy"
            | "glm-proxy" => Some(Self::OpenAiCompatible),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Model configuration
// ---------------------------------------------------------------------------

/// Model-level configuration consumed by provider adapters.
#[derive(Debug, Clone)]
pub struct AiModelConfig {
    /// Chat completions model identifier.
    pub chat_model: String,
    /// Embeddings model identifier, if configured.
    pub embedding_model: Option<String>,
    /// Default maximum tokens for completions when the caller does not specify.
    pub max_tokens: u32,
}

// ---------------------------------------------------------------------------
// Full provider configuration
// ---------------------------------------------------------------------------

/// Validated AI provider configuration parsed from environment variables.
#[derive(Debug, Clone)]
pub struct AiProviderConfig {
    /// Which provider backend to use.
    pub provider_type: AiProviderType,
    /// Base URL override for the provider API endpoint.
    pub base_url: Option<String>,
    /// API key for authentication.
    pub api_key: Option<String>,
    /// Model-level settings.
    pub models: AiModelConfig,
    /// Request timeout, if configured.
    pub timeout: Option<Duration>,
}

// ---------------------------------------------------------------------------
// Defaults (current-safe)
// ---------------------------------------------------------------------------

const DEFAULT_PROVIDER_TYPE: AiProviderType = AiProviderType::OpenAiCompatible;
const DEFAULT_CHAT_MODEL: &str = "gpt-4-turbo-preview";
const DEFAULT_MAX_TOKENS: u32 = 1000;

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Read an env var, returning `None` when unset **or** when the value is empty
/// / whitespace-only.
fn env_opt(key: &str) -> Option<String> {
    let val = env::var(key).ok()?;
    let trimmed = val.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Parse a required numeric env var with validation.
fn parse_numeric(key: &'static str) -> Result<Option<u64>, ConfigError> {
    match env::var(key) {
        Err(_) => Ok(None),
        Ok(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed
                .parse::<u64>()
                .map(Some)
                .map_err(|e| ConfigError::InvalidNumeric {
                    key,
                    value: trimmed,
                    source: e.to_string(),
                })
        }
    }
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

impl AiProviderConfig {
    /// Parse and validate configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnsupportedProviderType`] when
    /// `WINSTON_AI_PROVIDER_TYPE` is set to an unrecognised value,
    /// [`ConfigError::InvalidNumeric`] when timeout/max-tokens values cannot
    /// be parsed, or [`ConfigError::OutOfRange`] when `WINSTON_AI_MAX_TOKENS`
    /// exceeds the u16 limit imposed by the async-openai SDK.
    pub fn try_from_env() -> Result<Self, ConfigError> {
        // Provider type
        let provider_type = match env_opt(env_keys::PROVIDER_TYPE) {
            None => DEFAULT_PROVIDER_TYPE,
            Some(ref raw) => AiProviderType::from_str_loose(raw).ok_or_else(|| {
                ConfigError::UnsupportedProviderType {
                    value: raw.clone(),
                }
            })?,
        };

        // Endpoint credentials
        let base_url = env_opt(env_keys::BASE_URL);
        let api_key = env_opt(env_keys::API_KEY);

        // Chat model: prefer the new var, fall back to legacy alias.
        let chat_model = env_opt(env_keys::CHAT_MODEL)
            .or_else(|| env_opt(env_keys::MODEL_LEGACY))
            .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

        // Embedding model: purely optional.
        let embedding_model = env_opt(env_keys::EMBEDDING_MODEL);

        // Timeout
        let timeout_secs = parse_numeric(env_keys::TIMEOUT_SECONDS)?;
        let timeout = timeout_secs.map(Duration::from_secs);

        // Max tokens - must fit in u16 as required by the async-openai SDK.
        let max_tokens = match parse_numeric(env_keys::MAX_TOKENS)? {
            Some(v) => {
                if v > u16::MAX as u64 {
                    return Err(ConfigError::OutOfRange {
                        key: env_keys::MAX_TOKENS,
                        value: v,
                        max: u16::MAX as u64,
                    });
                }
                v as u32
            }
            None => DEFAULT_MAX_TOKENS,
        };

        Ok(Self {
            provider_type,
            base_url,
            api_key,
            models: AiModelConfig {
                chat_model,
                embedding_model,
                max_tokens,
            },
            timeout,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: remove all config-related env vars to avoid cross-test leakage.
    fn clear_env() {
        for key in &[
            env_keys::PROVIDER_TYPE,
            env_keys::BASE_URL,
            env_keys::API_KEY,
            env_keys::CHAT_MODEL,
            env_keys::MODEL_LEGACY,
            env_keys::EMBEDDING_MODEL,
            env_keys::TIMEOUT_SECONDS,
            env_keys::MAX_TOKENS,
        ] {
            env::remove_var(key);
        }
    }

    #[test]
    fn defaults_when_no_env_set() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert_eq!(cfg.provider_type, AiProviderType::OpenAiCompatible);
        assert!(cfg.base_url.is_none());
        assert!(cfg.api_key.is_none());
        assert_eq!(cfg.models.chat_model, DEFAULT_CHAT_MODEL);
        assert!(cfg.models.embedding_model.is_none());
        assert!(cfg.timeout.is_none());
        assert_eq!(cfg.models.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn full_env_parsing() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::PROVIDER_TYPE, "openai_compatible");
        env::set_var(env_keys::BASE_URL, "https://glm-proxy.example.com/v1");
        env::set_var(env_keys::API_KEY, "sk-test-key");
        env::set_var(env_keys::CHAT_MODEL, "glm-4-flash");
        env::set_var(env_keys::EMBEDDING_MODEL, "glm-embedding");
        env::set_var(env_keys::TIMEOUT_SECONDS, "60");
        env::set_var(env_keys::MAX_TOKENS, "2048");

        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert_eq!(cfg.provider_type, AiProviderType::OpenAiCompatible);
        assert_eq!(
            cfg.base_url.as_deref(),
            Some("https://glm-proxy.example.com/v1")
        );
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test-key"));
        assert_eq!(cfg.models.chat_model, "glm-4-flash");
        assert_eq!(cfg.models.embedding_model.as_deref(), Some("glm-embedding"));
        assert_eq!(cfg.timeout, Some(Duration::from_secs(60)));
        assert_eq!(cfg.models.max_tokens, 2048);

        clear_env();
    }

    #[test]
    fn legacy_model_alias_used_when_chat_model_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::MODEL_LEGACY, "legacy-model");

        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert_eq!(cfg.models.chat_model, "legacy-model");

        clear_env();
    }

    #[test]
    fn chat_model_takes_precedence_over_legacy() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::CHAT_MODEL, "new-model");
        env::set_var(env_keys::MODEL_LEGACY, "old-model");

        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert_eq!(cfg.models.chat_model, "new-model");

        clear_env();
    }

    #[test]
    fn optional_embedding_model_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert!(cfg.models.embedding_model.is_none());
    }

    #[test]
    fn empty_strings_trimmed_to_none() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::BASE_URL, "   ");
        env::set_var(env_keys::API_KEY, "");
        env::set_var(env_keys::EMBEDDING_MODEL, "\t");

        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert!(cfg.base_url.is_none());
        assert!(cfg.api_key.is_none());
        assert!(cfg.models.embedding_model.is_none());

        clear_env();
    }

    #[test]
    fn unsupported_provider_type_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::PROVIDER_TYPE, "anthropic");

        let err = AiProviderConfig::try_from_env().expect_err("should fail");
        match err {
            ConfigError::UnsupportedProviderType { value } => {
                assert_eq!(value, "anthropic");
            }
            other => panic!("expected UnsupportedProviderType, got {:?}", other),
        }

        clear_env();
    }

    #[test]
    fn invalid_timeout_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::TIMEOUT_SECONDS, "not-a-number");

        let err = AiProviderConfig::try_from_env().expect_err("should fail");
        match err {
            ConfigError::InvalidNumeric { key, value, .. } => {
                assert_eq!(key, env_keys::TIMEOUT_SECONDS);
                assert_eq!(value, "not-a-number");
            }
            other => panic!("expected InvalidNumeric, got {:?}", other),
        }

        clear_env();
    }

    #[test]
    fn invalid_max_tokens_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::MAX_TOKENS, "abc");

        let err = AiProviderConfig::try_from_env().expect_err("should fail");
        match err {
            ConfigError::InvalidNumeric { key, value, .. } => {
                assert_eq!(key, env_keys::MAX_TOKENS);
                assert_eq!(value, "abc");
            }
            other => panic!("expected InvalidNumeric, got {:?}", other),
        }

        clear_env();
    }

    #[test]
    fn max_tokens_out_of_range_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        // u16::MAX = 65535; anything larger should be rejected.
        env::set_var(env_keys::MAX_TOKENS, "65536");

        let err = AiProviderConfig::try_from_env().expect_err("should fail");
        match err {
            ConfigError::OutOfRange { key, value, max } => {
                assert_eq!(key, env_keys::MAX_TOKENS);
                assert_eq!(value, 65536);
                assert_eq!(max, u16::MAX as u64);
            }
            other => panic!("expected OutOfRange, got {:?}", other),
        }

        clear_env();
    }

    #[test]
    fn max_tokens_at_u16_max_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var(env_keys::MAX_TOKENS, "65535");

        let cfg = AiProviderConfig::try_from_env().expect("should parse");
        assert_eq!(cfg.models.max_tokens, 65535);

        clear_env();
    }

    #[test]
    fn provider_type_from_str_loose_variants() {
        assert_eq!(
            AiProviderType::from_str_loose("OpenAI_Compatible"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(
            AiProviderType::from_str_loose("openai-compatible"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(
            AiProviderType::from_str_loose("OPENAI"),
            Some(AiProviderType::OpenAiCompatible)
        );
        // GLM proxy aliases
        assert_eq!(
            AiProviderType::from_str_loose("glm_proxy"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(
            AiProviderType::from_str_loose("glm-proxy"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(
            AiProviderType::from_str_loose("GLM_PROXY"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(
            AiProviderType::from_str_loose("GLM-Proxy"),
            Some(AiProviderType::OpenAiCompatible)
        );
        assert_eq!(AiProviderType::from_str_loose("unknown"), None);
    }
}
