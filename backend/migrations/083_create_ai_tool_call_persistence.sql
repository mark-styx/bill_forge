-- Migration 083: AI tool calls and tool results tables
--
-- Stores AI agent tool invocations and their execution results, scoped by
-- tenant/user/conversation/message for multi-tenant isolation and cascading
-- cleanup when parent messages or conversations are deleted.
--
-- RLS policies match the pattern from migrations 080/082.

-- ---------------------------------------------------------------------------
-- Composite unique constraint on ai_messages for scoped foreign keys
-- (idempotent: skip if already present from a previous run)
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ai_messages_id_tenant_user_conversation_unique'
    ) THEN
        ALTER TABLE ai_messages
            ADD CONSTRAINT ai_messages_id_tenant_user_conversation_unique
            UNIQUE (id, tenant_id, user_id, conversation_id);
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- AI TOOL CALLS
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_tool_calls (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    message_id UUID NOT NULL,
    provider_tool_call_id TEXT,
    tool_name TEXT NOT NULL,
    arguments JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'requested'
        CHECK (status IN ('requested', 'executing', 'completed', 'failed', 'cancelled')),
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Composite FK: must match a real (id, tenant_id, user_id, conversation_id)
    -- in ai_messages.  ON DELETE CASCADE propagates message/conversation deletion.
    FOREIGN KEY (message_id, tenant_id, user_id, conversation_id)
        REFERENCES ai_messages(id, tenant_id, user_id, conversation_id)
        ON DELETE CASCADE
);

-- Composite unique constraint so tool_results can reference the full scope
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ai_tool_calls_id_tenant_user_conv_msg_unique'
    ) THEN
        ALTER TABLE ai_tool_calls
            ADD CONSTRAINT ai_tool_calls_id_tenant_user_conv_msg_unique
            UNIQUE (id, tenant_id, user_id, conversation_id, message_id);
    END IF;
END $$;

-- Tool calls for a tenant+user within a conversation+message, ordered by time
CREATE INDEX IF NOT EXISTS idx_ai_tool_calls_tenant_user_conv_msg_created
    ON ai_tool_calls(tenant_id, user_id, conversation_id, message_id, created_at ASC);

-- ---------------------------------------------------------------------------
-- AI TOOL RESULTS
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_tool_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    message_id UUID NOT NULL,
    tool_call_id UUID NOT NULL,
    success BOOLEAN NOT NULL,
    result JSONB,
    error TEXT,
    latency_ms BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Composite FK to parent message (same scope as tool_calls)
    FOREIGN KEY (message_id, tenant_id, user_id, conversation_id)
        REFERENCES ai_messages(id, tenant_id, user_id, conversation_id)
        ON DELETE CASCADE,
    -- Composite FK to parent tool call: must match the exact same
    -- tenant/user/conversation/message scope, preventing cross-scope results.
    FOREIGN KEY (tool_call_id, tenant_id, user_id, conversation_id, message_id)
        REFERENCES ai_tool_calls(id, tenant_id, user_id, conversation_id, message_id)
        ON DELETE CASCADE
);

-- Tool results for a specific tool call
CREATE INDEX IF NOT EXISTS idx_ai_tool_results_tool_call_id
    ON ai_tool_results(tool_call_id);

-- Tool results for a tenant+user within a conversation+message
CREATE INDEX IF NOT EXISTS idx_ai_tool_results_tenant_user_conv_msg_created
    ON ai_tool_results(tenant_id, user_id, conversation_id, message_id, created_at ASC);

-- ---------------------------------------------------------------------------
-- ROW LEVEL SECURITY
-- ---------------------------------------------------------------------------

ALTER TABLE ai_tool_calls ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_tool_calls ON ai_tool_calls;
CREATE POLICY rls_tenant_ai_tool_calls ON ai_tool_calls
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

ALTER TABLE ai_tool_results ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_tool_results ON ai_tool_results;
CREATE POLICY rls_tenant_ai_tool_results ON ai_tool_results
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
