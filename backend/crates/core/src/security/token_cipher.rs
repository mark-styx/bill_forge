//! AES-256-GCM envelope cipher for OAuth tokens (QBO, Xero) stored at rest.
//!
//! Closes the open TODO in migration 079 by ensuring tokens persisted in the
//! tenant DB are not directly usable from a DB dump or backup leak. The
//! runtime key lives only in `BILLFORGE_TOKEN_ENC_KEY` (base64 of 32 bytes)
//! and never touches the database.
//!
//! Envelope format (versioned so we can rotate algorithm or key without a
//! destructive backfill): `v1:<nonce_b64>:<ct_b64>` where `nonce_b64` is the
//! base64url-no-pad encoding of a fresh 96-bit nonce and `ct_b64` is the
//! base64url-no-pad encoding of `ciphertext || tag`.
//!
//! Reads tolerate legacy plaintext rows (any value not prefixed with `v1:`)
//! so a single deployment can carry pre-existing connections forward; the
//! next write reseals them.

use std::sync::atomic::{AtomicBool, Ordering};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use thiserror::Error;

const ENVELOPE_PREFIX: &str = "v1:";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const ENV_VAR: &str = "BILLFORGE_TOKEN_ENC_KEY";

#[derive(Debug, Error)]
pub enum CipherError {
    #[error("{ENV_VAR} is not set; cannot encrypt OAuth tokens at rest")]
    KeyMissing,
    #[error("{ENV_VAR} must be base64-encoded 32 bytes (256-bit AES key): {0}")]
    BadKey(String),
    #[error("token envelope is malformed")]
    BadEnvelope,
    #[error("token decryption failed")]
    DecryptFailed,
}

#[derive(Clone)]
pub struct TokenCipher {
    key: [u8; KEY_LEN],
}

impl std::fmt::Debug for TokenCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenCipher").finish_non_exhaustive()
    }
}

static LEGACY_WARNED: AtomicBool = AtomicBool::new(false);

impl TokenCipher {
    /// Load the cipher from `BILLFORGE_TOKEN_ENC_KEY`.
    pub fn from_env() -> Result<Self, CipherError> {
        let raw = std::env::var(ENV_VAR)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or(CipherError::KeyMissing)?;
        Self::from_base64(raw.trim())
    }

    /// Build a cipher from a base64-encoded 32-byte key (accepts standard or
    /// url-safe alphabets, with or without padding).
    pub fn from_base64(b64: &str) -> Result<Self, CipherError> {
        let decoded = decode_b64_flexible(b64)
            .map_err(|e| CipherError::BadKey(format!("invalid base64: {e}")))?;
        if decoded.len() != KEY_LEN {
            return Err(CipherError::BadKey(format!(
                "expected {KEY_LEN} bytes, got {}",
                decoded.len()
            )));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&decoded);
        Ok(Self { key })
    }

    /// Encrypt a plaintext token, returning a `v1:` envelope string safe to
    /// store in a TEXT column.
    pub fn seal(&self, plaintext: &str) -> String {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: b"billforge.oauth_token.v1",
                },
            )
            .expect("AES-GCM encryption with a valid key cannot fail");
        format!(
            "{ENVELOPE_PREFIX}{}:{}",
            URL_SAFE_NO_PAD.encode(nonce_bytes),
            URL_SAFE_NO_PAD.encode(ciphertext)
        )
    }

    /// Decrypt a stored token. Values that do not have the `v1:` envelope
    /// prefix are returned as-is so legacy plaintext rows continue to work
    /// during the rollover window; a one-shot warning is emitted in that
    /// case.
    pub fn open(&self, value: &str) -> Result<String, CipherError> {
        let Some(rest) = value.strip_prefix(ENVELOPE_PREFIX) else {
            if !LEGACY_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "Decrypting legacy plaintext OAuth token; next write will re-seal it. \
                     Migrate by re-connecting affected integrations."
                );
            }
            return Ok(value.to_string());
        };

        let (nonce_b64, ct_b64) = rest.split_once(':').ok_or(CipherError::BadEnvelope)?;
        let nonce_bytes = URL_SAFE_NO_PAD
            .decode(nonce_b64)
            .map_err(|_| CipherError::BadEnvelope)?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(CipherError::BadEnvelope);
        }
        let ciphertext = URL_SAFE_NO_PAD
            .decode(ct_b64)
            .map_err(|_| CipherError::BadEnvelope)?;

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &ciphertext,
                    aad: b"billforge.oauth_token.v1",
                },
            )
            .map_err(|_| CipherError::DecryptFailed)?;
        String::from_utf8(plaintext).map_err(|_| CipherError::DecryptFailed)
    }
}

fn decode_b64_flexible(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE};
    STANDARD
        .decode(input)
        .or_else(|_| STANDARD_NO_PAD.decode(input))
        .or_else(|_| URL_SAFE.decode(input))
        .or_else(|_| URL_SAFE_NO_PAD.decode(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_cipher() -> TokenCipher {
        // Deterministic 32-byte key for tests only.
        let key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        TokenCipher::from_base64(&base64::engine::general_purpose::STANDARD.encode(key)).unwrap()
    }

    #[test]
    fn seal_then_open_roundtrip() {
        let cipher = fixture_cipher();
        let plaintext = "qbo-access-token-abcdef.1234567890";
        let envelope = cipher.seal(plaintext);
        assert!(envelope.starts_with("v1:"));
        assert!(!envelope.contains(plaintext));
        let recovered = cipher.open(&envelope).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn each_seal_uses_fresh_nonce() {
        let cipher = fixture_cipher();
        let a = cipher.seal("same-token");
        let b = cipher.seal("same-token");
        assert_ne!(a, b, "nonces must differ across encryptions");
    }

    #[test]
    fn open_legacy_plaintext_passthrough() {
        let cipher = fixture_cipher();
        let legacy = "legacy-plaintext-refresh-token";
        let recovered = cipher.open(legacy).unwrap();
        assert_eq!(recovered, legacy);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let cipher = fixture_cipher();
        let envelope = cipher.seal("a-real-token");
        // Flip the last character of the ciphertext component.
        let mut bytes = envelope.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();
        let err = cipher.open(&tampered).unwrap_err();
        assert!(matches!(err, CipherError::DecryptFailed | CipherError::BadEnvelope));
    }

    #[test]
    fn bad_key_size_rejected() {
        let too_short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let err = TokenCipher::from_base64(&too_short).unwrap_err();
        assert!(matches!(err, CipherError::BadKey(_)));
    }

    #[test]
    fn malformed_envelope_rejected() {
        let cipher = fixture_cipher();
        let err = cipher.open("v1:not-a-valid-envelope").unwrap_err();
        assert!(matches!(err, CipherError::BadEnvelope));
    }
}
