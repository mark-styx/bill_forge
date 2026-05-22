-- Migration 082: AI conversations and messages tables
--
-- Provides durable storage for Winston AI chat conversations and individual
-- messages, scoped by tenant and user for multi-tenant isolation.
--
-- Every row is keyed by tenant_id and user_id so that the application can
-- list/load conversations per-user within a tenant.  RLS policies mirror the
-- existing style from migration 080 (app.current_tenant_id session variable).

-- ---------------------------------------------------------------------------
-- AI CONVERSATIONS
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensures (id, tenant_id, user_id) tuple is unique so messages can
    -- reference all three columns via a composite foreign key, preventing
    -- a message from being attached to another tenant/user's conversation.
    UNIQUE(id, tenant_id, user_id)
);

-- List conversations for a tenant+user, most recently updated first
CREATE INDEX IF NOT EXISTS idx_ai_conversations_tenant_user_updated_at
    ON ai_conversations(tenant_id, user_id, updated_at DESC);

-- ---------------------------------------------------------------------------
-- AI MESSAGES
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant')),
    content TEXT NOT NULL,
    -- Provider-neutral usage / telemetry fields
    provider TEXT,
    model TEXT,
    model_route TEXT,
    prompt_tokens INT,
    completion_tokens INT,
    total_tokens INT,
    finish_reason TEXT,
    provider_request_id TEXT,
    latency_ms BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Composite FK: a message's (conversation_id, tenant_id, user_id) must
    -- match the referenced conversation row, preventing cross-tenant/user
    -- attachment.  ON DELETE CASCADE propagates conversation deletion.
    FOREIGN KEY (conversation_id, tenant_id, user_id)
        REFERENCES ai_conversations(id, tenant_id, user_id)
        ON DELETE CASCADE
);

-- Messages for a tenant+user within a conversation, in order
CREATE INDEX IF NOT EXISTS idx_ai_messages_tenant_user_conversation_created_at
    ON ai_messages(tenant_id, user_id, conversation_id, created_at ASC);

-- All messages in a conversation regardless of user (for loading full history)
CREATE INDEX IF NOT EXISTS idx_ai_messages_conversation_created_at
    ON ai_messages(conversation_id, created_at ASC);

-- ---------------------------------------------------------------------------
-- ROW LEVEL SECURITY
-- ---------------------------------------------------------------------------

ALTER TABLE ai_conversations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_conversations ON ai_conversations;
CREATE POLICY rls_tenant_ai_conversations ON ai_conversations
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

ALTER TABLE ai_messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_messages ON ai_messages;
CREATE POLICY rls_tenant_ai_messages ON ai_messages
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
