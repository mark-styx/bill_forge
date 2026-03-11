//! Configuration for the API server

use anyhow::Result;
use billforge_auth::JwtConfig;
use billforge_email::EmailConfig;
use billforge_mobile_push::{FcmConfig, ApnsConfig, ApnsEnvironment};

/// Server configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub tenant_db_path: String,
    pub jwt: JwtConfig,
    pub frontend_url: String,
    pub storage_path: String,
    pub ocr_provider: String,
    /// Additional allowed CORS origins (comma-separated)
    pub allowed_origins: Vec<String>,
    /// Rate limiting: requests per minute per IP
    pub rate_limit_rpm: u64,
    /// Rate limiting: burst size
    pub rate_limit_burst: u32,
    /// Environment mode (development, staging, production)
    pub environment: Environment,
    /// Email service configuration (None if email is disabled)
    pub email: Option<EmailConfig>,
    /// QuickBooks integration configuration (None if QuickBooks is disabled)
    pub quickbooks: Option<QuickBooksConfig>,
    /// Xero integration configuration (None if Xero is disabled)
    pub xero: Option<XeroConfig>,
    /// FCM (Firebase Cloud Messaging) configuration (None if push notifications are disabled)
    pub fcm: Option<FcmConfig>,
    /// APNS (Apple Push Notification Service) configuration (None if push notifications are disabled)
    pub apns: Option<ApnsConfig>,
}

/// QuickBooks Online integration configuration
#[derive(Debug, Clone)]
pub struct QuickBooksConfig {
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// OAuth redirect URI
    pub redirect_uri: String,
    /// Environment (sandbox or production)
    pub environment: QuickBooksEnvironment,
}

/// QuickBooks environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickBooksEnvironment {
    Sandbox,
    Production,
}

impl QuickBooksEnvironment {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            _ => Self::Sandbox,
        }
    }
}

/// Xero accounting integration configuration
#[derive(Debug, Clone)]
pub struct XeroConfig {
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// OAuth redirect URI
    pub redirect_uri: String,
    /// Environment (sandbox or production)
    pub environment: XeroEnvironment,
}

/// Xero environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XeroEnvironment {
    Sandbox,
    Production,
}

impl XeroEnvironment {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            _ => Self::Sandbox,
        }
    }
}

/// Environment mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            "staging" | "stage" => Self::Staging,
            _ => Self::Development,
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }

    pub fn is_development(&self) -> bool {
        matches!(self, Self::Development)
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let environment = Environment::from_str(
            &std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string())
        );

        // Parse allowed origins from comma-separated list
        let frontend_url = std::env::var("FRONTEND_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        let mut allowed_origins: Vec<String> = std::env::var("ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Always include the frontend URL
        if !allowed_origins.contains(&frontend_url) {
            allowed_origins.push(frontend_url.clone());
        }

        // In development, also allow common dev origins
        if environment.is_development() {
            let dev_origins = vec![
                "http://localhost:3000".to_string(),
                "http://localhost:3001".to_string(),
                "http://127.0.0.1:3000".to_string(),
            ];
            for origin in dev_origins {
                if !allowed_origins.contains(&origin) {
                    allowed_origins.push(origin);
                }
            }
        }

        // Validate JWT secret in production
        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "development-secret-change-in-production".to_string());

        if environment.is_production() && jwt_secret.contains("development") {
            anyhow::bail!("JWT_SECRET must be set to a secure value in production");
        }

        // Load email configuration (optional - only if SMTP_HOST is set)
        let email = EmailConfig::from_env();

        // Load QuickBooks configuration (optional - only if QUICKBOOKS_CLIENT_ID is set)
        let quickbooks = if let Ok(client_id) = std::env::var("QUICKBOOKS_CLIENT_ID") {
            Some(QuickBooksConfig {
                client_id,
                client_secret: std::env::var("QUICKBOOKS_CLIENT_SECRET")
                    .unwrap_or_default(),
                redirect_uri: std::env::var("QUICKBOOKS_REDIRECT_URI")
                    .unwrap_or_else(|_| format!("{}/api/v1/quickbooks/callback", frontend_url)),
                environment: QuickBooksEnvironment::from_str(
                    &std::env::var("QUICKBOOKS_ENVIRONMENT").unwrap_or_default()
                ),
            })
        } else {
            None
        };

        // Load Xero configuration (optional - only if XERO_CLIENT_ID is set)
        let xero = if let Ok(client_id) = std::env::var("XERO_CLIENT_ID") {
            Some(XeroConfig {
                client_id,
                client_secret: std::env::var("XERO_CLIENT_SECRET")
                    .unwrap_or_default(),
                redirect_uri: std::env::var("XERO_REDIRECT_URI")
                    .unwrap_or_else(|_| format!("{}/api/v1/xero/callback", frontend_url)),
                environment: XeroEnvironment::from_str(
                    &std::env::var("XERO_ENVIRONMENT").unwrap_or_default()
                ),
            })
        } else {
            None
        };

        // Load FCM configuration (optional - only if FCM_API_KEY is set)
        let fcm = if let Ok(api_key) = std::env::var("FCM_API_KEY") {
            Some(FcmConfig {
                api_key,
            })
        } else {
            None
        };

        // Load APNS configuration (optional - only if APNS_KEY_ID is set)
        let apns = if let Ok(key_id) = std::env::var("APNS_KEY_ID") {
            Some(ApnsConfig {
                environment: match std::env::var("APNS_ENVIRONMENT").unwrap_or_default().as_str() {
                    "production" => ApnsEnvironment::Production,
                    _ => ApnsEnvironment::Sandbox,
                },
                private_key_path: std::env::var("APNS_PRIVATE_KEY_PATH")
                    .unwrap_or_else(|_| "./AuthKey.p8".to_string()),
                key_id,
                team_id: std::env::var("APNS_TEAM_ID")
                    .unwrap_or_default(),
                bundle_id: std::env::var("APNS_BUNDLE_ID")
                    .unwrap_or_else(|_| "com.billforge.mobile".to_string()),
            })
        } else {
            None
        };

        Ok(Self {
            host: std::env::var("BACKEND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("BACKEND_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()?,
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://./data/billforge.db".to_string()),
            tenant_db_path: std::env::var("TENANT_DB_PATH")
                .unwrap_or_else(|_| "./data/tenants".to_string()),
            jwt: JwtConfig {
                secret: jwt_secret,
                access_token_expiry_hours: std::env::var("JWT_EXPIRY")
                    .unwrap_or_else(|_| "24".to_string())
                    .parse()
                    .unwrap_or(24),
                refresh_token_expiry_days: 7,
            },
            frontend_url,
            storage_path: std::env::var("LOCAL_STORAGE_PATH")
                .unwrap_or_else(|_| "./data/files".to_string()),
            ocr_provider: std::env::var("OCR_PROVIDER")
                .unwrap_or_else(|_| "tesseract".to_string()),
            allowed_origins,
            rate_limit_rpm: std::env::var("RATE_LIMIT_RPM")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .unwrap_or(20),
            environment,
            email,
            quickbooks,
            xero,
            fcm,
            apns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_parsing() {
        assert!(matches!(Environment::from_str("production"), Environment::Production));
        assert!(matches!(Environment::from_str("prod"), Environment::Production));
        assert!(matches!(Environment::from_str("staging"), Environment::Staging));
        assert!(matches!(Environment::from_str("development"), Environment::Development));
        assert!(matches!(Environment::from_str("dev"), Environment::Development));
        assert!(matches!(Environment::from_str("unknown"), Environment::Development));
    }

    #[test]
    fn test_environment_checks() {
        assert!(Environment::Production.is_production());
        assert!(!Environment::Development.is_production());
        assert!(Environment::Development.is_development());
        assert!(!Environment::Production.is_development());
    }
}
