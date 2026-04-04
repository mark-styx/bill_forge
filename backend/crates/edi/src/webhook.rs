//! Webhook signature verification for EDI middleware callbacks

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify an inbound webhook signature (HMAC-SHA256)
///
/// Most EDI middleware providers sign webhook payloads with a shared secret
/// using HMAC-SHA256. The signature is sent in a header (typically
/// `X-Signature` or `X-Webhook-Signature`).
pub fn verify_webhook_signature(payload: &[u8], signature: &str, secret: &str) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);

    // Signature may be hex-encoded or prefixed with "sha256="
    let sig_hex = signature.strip_prefix("sha256=").unwrap_or(signature);
    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };

    mac.verify_slice(&sig_bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_valid_signature() {
        let secret = "test-webhook-secret";
        let payload = b"hello world";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verify_webhook_signature(payload, &signature, secret));
    }

    #[test]
    fn test_reject_invalid_signature() {
        assert!(!verify_webhook_signature(b"payload", "bad-sig", "secret"));
    }

    #[test]
    fn test_verify_prefixed_signature() {
        let secret = "my-secret";
        let payload = b"test payload";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_webhook_signature(payload, &sig, secret));
    }
}
