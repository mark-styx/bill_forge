//! EDI middleware configuration

use serde::{Deserialize, Serialize};

/// EDI middleware provider
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdiProvider {
    Stedi,
    Orderful,
    SpsCommerce,
    Custom,
}

impl std::fmt::Display for EdiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stedi => write!(f, "stedi"),
            Self::Orderful => write!(f, "orderful"),
            Self::SpsCommerce => write!(f, "sps_commerce"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Configuration for the EDI middleware connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiConfig {
    /// API key for the EDI middleware
    pub api_key: String,
    /// Webhook signing secret for verifying inbound payloads
    pub webhook_secret: String,
    /// EDI middleware provider
    pub provider: EdiProvider,
    /// Base URL for the middleware API
    pub api_base_url: String,
    /// Our ISA qualifier (e.g., "ZZ", "01", "08")
    pub our_isa_qualifier: String,
    /// Our ISA ID (our EDI identifier)
    pub our_isa_id: String,
}

impl EdiConfig {
    /// Create config for Stedi
    pub fn stedi(api_key: String, webhook_secret: String) -> Self {
        Self {
            api_key,
            webhook_secret,
            provider: EdiProvider::Stedi,
            api_base_url: "https://core.us.stedi.com/2023-08-01".to_string(),
            our_isa_qualifier: "ZZ".to_string(),
            our_isa_id: String::new(),
        }
    }
}
