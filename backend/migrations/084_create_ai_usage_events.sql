-- Migration 084: AI usage events table
--
-- Records per-request AI usage telemetry: provider, model, latency, token
-- counts, and error details.  Scoped by tenant and user for multi-tenant
-- isolation with RLS matching migrations 082/083.
--
-- Optional FKs to ai_conversations / ai_messages allow correlating usage
-- events with specific chat turns without making them mandatory (usage can
-- be recorded for non-conversation requests like embeddings or
-- classification).

-- ---------------------------------------------------------------------------
-- AI USAGE EVENTS
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_usage_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Optional links to conversation / message context
    conversation_id UUID,
    message_id UUID,

    -- Provider / model identification
    provider TEXT NOT NULL,
    model TEXT,
    model_route TEXT,

    -- Latency and token telemetry (nullable because a failed request may
    -- never receive these values from the provider)
    latency_ms BIGINT CHECK (latency_ms IS NULL OR latency_ms >= 0),
    prompt_tokens INT CHECK (prompt_tokens IS NULL OR prompt_tokens >= 0),
    completion_tokens INT CHECK (completion_tokens IS NULL OR completion_tokens >= 0),
    total_tokens INT CHECK (total_tokens IS NULL OR total_tokens >= 0),

    -- Success / error tracking
    success BOOLEAN NOT NULL DEFAULT true,
    error_code TEXT,
    error_message TEXT,

    -- Provider-side request identifier for support / debugging
    provider_request_id TEXT,

    -- Arbitrary extension point
    metadata JSONB NOT NULL DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Optional FK: when conversation_id is present it must match a real
    -- conversation. ON DELETE SET NULL preserves durable telemetry even after
    -- the conversation is deleted.
    FOREIGN KEY (conversation_id)
        REFERENCES ai_conversations(id)
        ON DELETE SET NULL,

    -- Optional FK: when message_id is present it must match a real message.
    -- ON DELETE SET NULL preserves the usage event row when a message is
    -- deleted.
    FOREIGN KEY (message_id)
        REFERENCES ai_messages(id)
        ON DELETE SET NULL,

    -- Invariant: success rows should not carry error fields; failed rows
    -- may still have partial latency/token data if the provider returned
    -- headers or a partial response before failing.
    CHECK (
        (success = true AND error_code IS NULL AND error_message IS NULL)
        OR success = false
    )
);

-- ---------------------------------------------------------------------------
-- INDEXES for usage reporting queries
-- ---------------------------------------------------------------------------

-- Tenant-scoped time-series queries (e.g. usage over time)
CREATE INDEX IF NOT EXISTS idx_ai_usage_events_tenant_created_at
    ON ai_usage_events(tenant_id, created_at DESC);

-- Per-provider / per-model breakdown
CREATE INDEX IF NOT EXISTS idx_ai_usage_events_tenant_provider_model_created_at
    ON ai_usage_events(tenant_id, provider, model, created_at DESC);

-- Per-user usage within a tenant
CREATE INDEX IF NOT EXISTS idx_ai_usage_events_tenant_user_created_at
    ON ai_usage_events(tenant_id, user_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- ROW LEVEL SECURITY
-- ---------------------------------------------------------------------------

ALTER TABLE ai_usage_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_usage_events ON ai_usage_events;
CREATE POLICY rls_tenant_ai_usage_events ON ai_usage_events
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
