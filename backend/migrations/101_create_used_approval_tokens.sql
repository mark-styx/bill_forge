-- Single-use approval token store (replaces in-memory HashSet).
-- Survives server restarts so that replayed JWTs are rejected within the 7-day TTL.
CREATE TABLE IF NOT EXISTS used_approval_tokens (
    jti        UUID PRIMARY KEY,
    tenant_id  UUID NOT NULL,
    invoice_id UUID,
    used_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS used_approval_tokens_expires_at_idx
    ON used_approval_tokens (expires_at);
