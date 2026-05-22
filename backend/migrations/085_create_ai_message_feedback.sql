-- Migration 085: AI message feedback table
--
-- Stores user feedback (thumbs up / down with optional comment) on Winston
-- assistant answers.  Scoped by tenant, user, conversation, and message for
-- multi-tenant isolation with RLS matching migrations 082/083/084.
--
-- A unique constraint on (tenant_id, user_id, message_id) ensures a user has
-- at most one current feedback record per Winston answer; later submissions
-- update the existing row (upsert).

-- ---------------------------------------------------------------------------
-- AI MESSAGE FEEDBACK
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_message_feedback (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    message_id UUID NOT NULL,

    rating TEXT NOT NULL CHECK (rating IN ('positive', 'negative')),
    comment TEXT,

    metadata JSONB NOT NULL DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Composite FK: must match a real (id, tenant_id, user_id, conversation_id)
    -- in ai_messages.  ON DELETE CASCADE propagates message/conversation deletion.
    FOREIGN KEY (message_id, tenant_id, user_id, conversation_id)
        REFERENCES ai_messages(id, tenant_id, user_id, conversation_id)
        ON DELETE CASCADE
);

-- One feedback record per user per message (upsert target)
CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_message_feedback_tenant_user_message
    ON ai_message_feedback(tenant_id, user_id, message_id);

-- ---------------------------------------------------------------------------
-- ROW LEVEL SECURITY
-- ---------------------------------------------------------------------------

ALTER TABLE ai_message_feedback ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_message_feedback ON ai_message_feedback;
CREATE POLICY rls_tenant_ai_message_feedback ON ai_message_feedback
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
