-- This migration is a no-op for new tenants (table created in tenant_db.rs)
-- For existing tenants, run against each tenant DB
CREATE TABLE IF NOT EXISTS edi_webhook_nonces (
    tenant_id UUID NOT NULL,
    nonce VARCHAR(128) NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, nonce)
);
CREATE INDEX IF NOT EXISTS idx_edi_nonces_received_at ON edi_webhook_nonces(received_at);
