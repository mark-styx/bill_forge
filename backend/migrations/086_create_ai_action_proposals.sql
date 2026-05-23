-- Migration 086: AI action proposals table
--
-- Stores approval-gated AI action proposals before execution. Proposals are
-- scoped by tenant, user, and conversation for multi-tenant isolation with RLS
-- matching migrations 082/083/084/085.

-- ---------------------------------------------------------------------------
-- AI ACTION PROPOSALS
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_action_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    tool_name TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    risk TEXT NOT NULL CHECK (risk IN ('low', 'medium', 'high')),
    permission TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'approval_required'
        CHECK (status IN ('approval_required', 'approved', 'rejected', 'executed', 'cancelled', 'expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Composite FK: must match a real (id, tenant_id, user_id) in
    -- ai_conversations, preventing cross-tenant/user proposal attachment.
    -- ON DELETE CASCADE propagates conversation deletion.
    FOREIGN KEY (conversation_id, tenant_id, user_id)
        REFERENCES ai_conversations(id, tenant_id, user_id)
        ON DELETE CASCADE
);

-- Proposals for a tenant+user by status, newest first
CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_tenant_user_status_created_at
    ON ai_action_proposals(tenant_id, user_id, status, created_at DESC);

-- Proposals in a conversation, newest first
CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_conversation_created_at
    ON ai_action_proposals(conversation_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- ROW LEVEL SECURITY
-- ---------------------------------------------------------------------------

ALTER TABLE ai_action_proposals ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ai_action_proposals ON ai_action_proposals;
CREATE POLICY rls_tenant_ai_action_proposals ON ai_action_proposals
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
