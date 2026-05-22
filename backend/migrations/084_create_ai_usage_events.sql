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

    -- Composite optional FK: when conversation_id is present it must match
    -- a real (id, tenant_id, user_id) in ai_conversations.  ON DELETE SET NULL
    -- preserves the usage event row by only clearing the nullable correlation
    -- column, keeping durable telemetry even after the conversation is deleted.
    FOREIGN KEY (conversation_id, tenant_id, user_id)
        REFERENCES ai_conversations(id, tenant_id, user_id)
        ON DELETE SET NULL (conversation_id),

    -- Composite optional FK: when both message_id and conversation_id are
    -- present, the tuple must match a real (id, tenant_id, user_id,
    -- conversation_id) in ai_messages, reusing the unique constraint
    -- introduced by migration 083.  Only message_id is nulled when the
    -- message is deleted so the conversation link survives if still valid.
    FOREIGN KEY (message_id, tenant_id, user_id, conversation_id)
        REFERENCES ai_messages(id, tenant_id, user_id, conversation_id)
        ON DELETE SET NULL (message_id),

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
