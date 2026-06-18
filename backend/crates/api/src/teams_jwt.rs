//! Microsoft AAD JWT validation for Teams Actionable Messages.
//!
//! Replaces the previous "any non-empty Bearer" stub at the `/teams/actions`
//! endpoint with real OIDC validation: RS256 signature against the configured
//! JWKS, plus `iss`, `aud`, `exp`, `nbf` checks. The validated `oid` claim is
//! then matched against `teams_webhooks.aad_object_id` so we can attribute the
//! action to a real BillForge user — not to the first active webhook row.
//!
//! Refs: issue #362.
//!
//! Operator configuration (required when `TEAMS_ACTIONS_ENABLED=true`):
//! - `TEAMS_OIDC_JWKS_URL`         — JWKS endpoint URL.
//! - `TEAMS_OIDC_EXPECTED_ISSUER`  — expected `iss` claim value.
//! - `TEAMS_OIDC_EXPECTED_AUDIENCE`— expected `aud` claim value.
//! - `TEAMS_JWKS_CACHE_TTL_SECS`   — optional, defaults to 3600.

use async_trait::async_trait;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TeamsJwtError {
    #[error("missing kid in token header")]
    MissingKid,
    #[error("kid not present in JWKS")]
    KeyNotFound,
    #[error("JWKS fetch failed: {0}")]
    JwksFetch(String),
    #[error("JWKS malformed: {0}")]
    JwksMalformed(String),
    #[error("invalid token: {0}")]
    InvalidToken(String),
    #[error("Teams JWT validation is not configured in this environment")]
    Disabled,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TeamsJwtClaims {
    pub oid: Uuid,
    pub tid: Option<Uuid>,
    pub iss: String,
    pub aud: String,
    pub exp: usize,
    pub nbf: Option<usize>,
}

#[async_trait]
pub trait JwksProvider: Send + Sync {
    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, TeamsJwtError>;
}

#[derive(Debug, Deserialize)]
struct JwksDoc {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

struct CachedJwks {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

/// Minimum gap between JWKS refreshes triggered by a `kid` miss on an
/// otherwise-fresh cache. Without this, an attacker can present a flood of
/// random `kid` values and force one outbound JWKS request per attack token.
/// Five minutes is well under the practical key-rotation interval but long
/// enough that the attack reduces to one outbound fetch per window.
const MIN_KID_MISS_REFETCH_GAP: Duration = Duration::from_secs(300);

pub struct MicrosoftJwksProvider {
    http: Client,
    discovery_url: String,
    cache: RwLock<Option<CachedJwks>>,
    ttl: Duration,
}

impl MicrosoftJwksProvider {
    pub fn new(discovery_url: String, ttl: Duration) -> Self {
        Self {
            http: Client::new(),
            discovery_url,
            cache: RwLock::new(None),
            ttl,
        }
    }

    async fn fetch_jwks(&self) -> Result<HashMap<String, DecodingKey>, TeamsJwtError> {
        let resp = self
            .http
            .get(&self.discovery_url)
            .send()
            .await
            .map_err(|e| TeamsJwtError::JwksFetch(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(TeamsJwtError::JwksFetch(format!("HTTP {}", resp.status())));
        }
        let doc: JwksDoc = resp
            .json()
            .await
            .map_err(|e| TeamsJwtError::JwksMalformed(e.to_string()))?;

        let mut keys = HashMap::with_capacity(doc.keys.len());
        for jwk in doc.keys {
            if jwk.kty != "RSA" {
                continue;
            }
            let (Some(n), Some(e)) = (jwk.n, jwk.e) else {
                continue;
            };
            let key = DecodingKey::from_rsa_components(&n, &e)
                .map_err(|err| TeamsJwtError::JwksMalformed(err.to_string()))?;
            keys.insert(jwk.kid, key);
        }
        Ok(keys)
    }
}

#[async_trait]
impl JwksProvider for MicrosoftJwksProvider {
    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, TeamsJwtError> {
        {
            let guard = self.cache.read().await;
            if let Some(c) = guard.as_ref() {
                if c.fetched_at.elapsed() < self.ttl {
                    if let Some(k) = c.keys.get(kid) {
                        return Ok(k.clone());
                    }
                }
            }
        }
        let mut guard = self.cache.write().await;
        if let Some(c) = guard.as_ref() {
            if c.fetched_at.elapsed() < self.ttl {
                if let Some(k) = c.keys.get(kid) {
                    return Ok(k.clone());
                }
                // Throttle: if a cache miss happens shortly after a successful
                // fetch, do NOT refetch — this prevents a flood of random
                // `kid` values from forcing one outbound JWKS request per
                // attack token. The TTL still drives normal periodic refresh.
                if c.fetched_at.elapsed() < MIN_KID_MISS_REFETCH_GAP {
                    return Err(TeamsJwtError::KeyNotFound);
                }
            }
        }
        let keys = self.fetch_jwks().await?;
        let result = keys.get(kid).cloned().ok_or(TeamsJwtError::KeyNotFound);
        *guard = Some(CachedJwks {
            keys,
            fetched_at: Instant::now(),
        });
        result
    }
}

/// JwksProvider used when `TEAMS_ACTIONS_ENABLED=false`. The Teams route is
/// not registered in that case, but keeping the validator field unconditional
/// in `AppState` simplifies plumbing.
pub struct DisabledJwksProvider;

#[async_trait]
impl JwksProvider for DisabledJwksProvider {
    async fn decoding_key(&self, _kid: &str) -> Result<DecodingKey, TeamsJwtError> {
        Err(TeamsJwtError::Disabled)
    }
}

pub struct TeamsJwtValidator {
    jwks: Arc<dyn JwksProvider>,
    expected_issuer: String,
    expected_audience: String,
}

impl TeamsJwtValidator {
    pub fn new(
        jwks: Arc<dyn JwksProvider>,
        expected_issuer: String,
        expected_audience: String,
    ) -> Self {
        Self {
            jwks,
            expected_issuer,
            expected_audience,
        }
    }

    pub fn disabled() -> Self {
        Self::new(Arc::new(DisabledJwksProvider), String::new(), String::new())
    }

    pub async fn validate(&self, bearer: &str) -> Result<TeamsJwtClaims, TeamsJwtError> {
        let header = decode_header(bearer)
            .map_err(|e| TeamsJwtError::InvalidToken(format!("header parse: {e}")))?;
        let kid = header.kid.ok_or(TeamsJwtError::MissingKid)?;
        let key = self.jwks.decoding_key(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.expected_issuer.clone()]);
        validation.set_audience(&[self.expected_audience.clone()]);
        validation.leeway = 60;
        validation.validate_nbf = true;

        let data = decode::<TeamsJwtClaims>(bearer, &key, &validation)
            .map_err(|e| TeamsJwtError::InvalidToken(e.to_string()))?;
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;

    const TEST_RSA_PRIVATE_PEM: &[u8] = include_bytes!("../tests/fixtures/teams_jwt_private.pem");
    const TEST_RSA_PUBLIC_PEM: &[u8] = include_bytes!("../tests/fixtures/teams_jwt_public.pem");

    #[derive(Serialize)]
    struct TestClaims {
        oid: Uuid,
        tid: Option<Uuid>,
        iss: String,
        aud: String,
        exp: usize,
        nbf: Option<usize>,
    }

    struct FakeJwks {
        keys: HashMap<String, DecodingKey>,
    }

    #[async_trait]
    impl JwksProvider for FakeJwks {
        async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, TeamsJwtError> {
            self.keys
                .get(kid)
                .cloned()
                .ok_or(TeamsJwtError::KeyNotFound)
        }
    }

    fn mint_token(claims: &TestClaims, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM).expect("test key");
        encode(&header, claims, &key).expect("encode")
    }

    fn validator() -> TeamsJwtValidator {
        let pub_key = DecodingKey::from_rsa_pem(TEST_RSA_PUBLIC_PEM).expect("test pub");
        let mut keys = HashMap::new();
        keys.insert("test-kid".to_string(), pub_key);
        TeamsJwtValidator::new(
            Arc::new(FakeJwks { keys }),
            "https://issuer.test/".to_string(),
            "billforge-test".to_string(),
        )
    }

    fn now_secs() -> usize {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
    }

    #[tokio::test]
    async fn valid_token_returns_claims() {
        let oid = Uuid::new_v4();
        let claims = TestClaims {
            oid,
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 300,
            nbf: Some(now_secs() - 30),
        };
        let token = mint_token(&claims, "test-kid");
        let out = validator().validate(&token).await.expect("validates");
        assert_eq!(out.oid, oid);
    }

    #[tokio::test]
    async fn expired_token_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() - 600,
            nbf: None,
        };
        let token = mint_token(&claims, "test-kid");
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn wrong_audience_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "wrong-audience".to_string(),
            exp: now_secs() + 300,
            nbf: None,
        };
        let token = mint_token(&claims, "test-kid");
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn wrong_issuer_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://attacker.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 300,
            nbf: None,
        };
        let token = mint_token(&claims, "test-kid");
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn future_nbf_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 3600,
            nbf: Some(now_secs() + 600), // 10 minutes in the future, beyond 60s leeway
        };
        let token = mint_token(&claims, "test-kid");
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn unknown_kid_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 300,
            nbf: None,
        };
        let token = mint_token(&claims, "other-kid");
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::KeyNotFound)
        ));
    }

    #[tokio::test]
    async fn missing_kid_rejected() {
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 300,
            nbf: None,
        };
        let header = Header::new(Algorithm::RS256); // no kid
        let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM).unwrap();
        let token = encode(&header, &claims, &key).unwrap();
        assert!(matches!(
            validator().validate(&token).await,
            Err(TeamsJwtError::MissingKid)
        ));
    }

    #[tokio::test]
    async fn disabled_validator_rejects() {
        let v = TeamsJwtValidator::disabled();
        let claims = TestClaims {
            oid: Uuid::new_v4(),
            tid: None,
            iss: "https://issuer.test/".to_string(),
            aud: "billforge-test".to_string(),
            exp: now_secs() + 300,
            nbf: None,
        };
        let token = mint_token(&claims, "test-kid");
        assert!(matches!(
            v.validate(&token).await,
            Err(TeamsJwtError::Disabled)
        ));
    }
}
