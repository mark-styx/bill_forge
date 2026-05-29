-- Ensure workflow/reporting tenant_id columns are UUIDs in tenant databases
-- created by PgManager's tracked tenant migration path.

ALTER TABLE IF EXISTS workflow_rules
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS work_queues
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS queue_items
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS assignment_rules
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS approval_requests
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS email_action_tokens
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS approval_delegations
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS workflow_audit_log
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;

ALTER TABLE IF EXISTS report_digests
    ALTER COLUMN tenant_id TYPE UUID USING tenant_id::UUID;
