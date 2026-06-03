-- Public API Platform: PAT auth keys, webhook subscriptions, webhook delivery audit
-- Issue #293

-- API keys (Personal Access Tokens) for tenant-scoped external API access
CREATE TABLE IF NOT EXISTS api_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    token_hash      TEXT NOT NULL,          -- SHA-256 of the bearer secret
    token_prefix    TEXT NOT NULL,          -- first 8 chars for lookup hints
    scopes          TEXT[] NOT NULL DEFAULT '{}',
    rate_limit_per_minute INT NOT NULL DEFAULT 60,
    last_used_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at      TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_token_hash ON api_keys(token_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_revoked ON api_keys(tenant_id, revoked_at);

-- Webhook subscriptions: tenant registers a target URL for specific event types
CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    api_key_id          UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    target_url          TEXT NOT NULL,
    event_types         TEXT[] NOT NULL DEFAULT '{}',
    signing_secret      TEXT NOT NULL,
    is_active           BOOLEAN NOT NULL DEFAULT true,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_delivery_at    TIMESTAMPTZ,
    last_delivery_status TEXT
);

CREATE INDEX IF NOT EXISTS idx_webhook_subscriptions_tenant_active ON webhook_subscriptions(tenant_id, is_active);

-- Webhook delivery audit log
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id   UUID NOT NULL REFERENCES webhook_subscriptions(id) ON DELETE CASCADE,
    event_type        TEXT NOT NULL,
    payload           JSONB NOT NULL DEFAULT '{}',
    response_status   INT,
    response_body     TEXT,
    attempted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success           BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_subscription ON webhook_deliveries(subscription_id, attempted_at DESC);
