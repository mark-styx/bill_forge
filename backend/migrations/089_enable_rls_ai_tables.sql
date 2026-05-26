-- Migration 089: Re-apply RLS policies for tenant-bearing AI tables.
--
-- The AI table creation migrations enable RLS when the tables are first
-- created. This follow-up migration keeps existing tenants aligned when the
-- central tenant migration runner is re-run after those tables already exist.

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

ALTER TABLE ai_usage_events ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS rls_tenant_ai_usage_events ON ai_usage_events;
CREATE POLICY rls_tenant_ai_usage_events ON ai_usage_events
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

ALTER TABLE ai_message_feedback ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS rls_tenant_ai_message_feedback ON ai_message_feedback;
CREATE POLICY rls_tenant_ai_message_feedback ON ai_message_feedback
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

ALTER TABLE ai_action_proposals ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS rls_tenant_ai_action_proposals ON ai_action_proposals;
CREATE POLICY rls_tenant_ai_action_proposals ON ai_action_proposals
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
