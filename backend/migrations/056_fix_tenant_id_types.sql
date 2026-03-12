-- Migration 056: Fix tenant_id types from VARCHAR to UUID
-- Required for type-safe database queries

-- Workflow tables (existing tables with VARCHAR tenant_id)
ALTER TABLE workflow_rules ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE work_queues ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE queue_items ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE assignment_rules ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE approval_requests ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE email_action_tokens ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE approval_delegations ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
ALTER TABLE workflow_audit_log ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

-- Reporting tables
ALTER TABLE report_digests ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
