-- Migration 154: document that QBO and Xero OAuth tokens are now stored as
-- AES-256-GCM envelopes at rest (closes the TODO recorded in migration 079).
--
-- Implementation lives in billforge_core::security::TokenCipher; on write
-- the API and worker call `seal(plaintext) -> "v1:<nonce_b64>:<ct_b64>"`
-- and on read they call `open(envelope) -> plaintext`. Legacy plaintext
-- rows pass through `open` unchanged so existing connections keep working
-- until the next write re-seals them.
--
-- No column type change is required: TEXT continues to hold the envelope.
-- Refs #432.

COMMENT ON COLUMN quickbooks_connections.access_token IS
    'AES-256-GCM envelope (v1:nonce_b64:ct_b64) sealed by TokenCipher. Refs #432.';
COMMENT ON COLUMN quickbooks_connections.refresh_token IS
    'AES-256-GCM envelope (v1:nonce_b64:ct_b64) sealed by TokenCipher. Refs #432.';

COMMENT ON COLUMN xero_connections.access_token IS
    'AES-256-GCM envelope (v1:nonce_b64:ct_b64) sealed by TokenCipher. Refs #432.';
COMMENT ON COLUMN xero_connections.refresh_token IS
    'AES-256-GCM envelope (v1:nonce_b64:ct_b64) sealed by TokenCipher. Refs #432.';
