-- Per-tenant invoice forwarding mailbox tables (METADATA DATABASE).
-- Enables ap@tenant.billforge.com inbound email ingestion.
-- These tables live in the metadata DB because they reference tenants(id)
-- and are queried during tenant resolution (before a tenant pool is available).

-- ---------------------------------------------------------------------------
-- tenant_forwarding_addresses: one row per tenant, provisioned on first access.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tenant_forwarding_addresses (
    tenant_id   UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    local_part  TEXT NOT NULL,
    full_address TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenant_forwarding_addresses_full
    ON tenant_forwarding_addresses(full_address);

-- ---------------------------------------------------------------------------
-- inbound_email_messages: every inbound email received for a tenant.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS inbound_email_messages (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    message_id    TEXT,
    from_address  TEXT NOT NULL,
    from_domain   TEXT NOT NULL,
    subject       TEXT,
    received_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status        TEXT NOT NULL DEFAULT 'processed'
                       CHECK (status IN ('processed','triage','rejected')),
    triage_reason TEXT,
    raw_payload   JSONB
);

CREATE INDEX IF NOT EXISTS idx_inbound_email_tenant_received
    ON inbound_email_messages(tenant_id, received_at DESC);

-- ---------------------------------------------------------------------------
-- email_triage_queue: items that need manual review (unknown sender, no
-- attachments, etc.).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS email_triage_queue (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    inbound_email_id UUID NOT NULL REFERENCES inbound_email_messages(id) ON DELETE CASCADE,
    reason           TEXT NOT NULL,
    resolved_at      TIMESTAMPTZ,
    resolved_by      UUID
);

CREATE INDEX IF NOT EXISTS idx_email_triage_unresolved
    ON email_triage_queue(inbound_email_id) WHERE resolved_at IS NULL;
