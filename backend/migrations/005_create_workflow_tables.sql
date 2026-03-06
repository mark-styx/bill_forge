-- Migration 005: Create workflow tables for approval system
-- Sprint 4: Approval Workflow & Email Actions

-- Workflow rules table
CREATE TABLE IF NOT EXISTS workflow_rules (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    rule_type VARCHAR(50) NOT NULL,
    conditions JSONB NOT NULL DEFAULT '[]',
    actions JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_workflow_rules_tenant ON workflow_rules(tenant_id);
CREATE INDEX idx_workflow_rules_type ON workflow_rules(tenant_id, rule_type);
CREATE INDEX idx_workflow_rules_active ON workflow_rules(tenant_id, is_active);

-- Work queues table
CREATE TABLE IF NOT EXISTS work_queues (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    queue_type VARCHAR(50) NOT NULL,
    assigned_users JSONB DEFAULT '[]',
    assigned_roles JSONB DEFAULT '[]',
    is_default BOOLEAN NOT NULL DEFAULT false,
    is_active BOOLEAN NOT NULL DEFAULT true,
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_work_queues_tenant ON work_queues(tenant_id);
CREATE INDEX idx_work_queues_type ON work_queues(tenant_id, queue_type);

-- Queue items table
CREATE TABLE IF NOT EXISTS queue_items (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    queue_id UUID NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    assigned_to UUID REFERENCES users(id),
    assigned_by_rule UUID,
    priority INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    entered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    due_at TIMESTAMPTZ,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    completion_action VARCHAR(100),
    notes TEXT
);

CREATE INDEX idx_queue_items_queue ON queue_items(queue_id);
CREATE INDEX idx_queue_items_invoice ON queue_items(invoice_id);
CREATE INDEX idx_queue_items_assigned ON queue_items(assigned_to);
CREATE INDEX idx_queue_items_status ON queue_items(queue_id, status);

-- Assignment rules table
CREATE TABLE IF NOT EXISTS assignment_rules (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    queue_id UUID NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    conditions JSONB NOT NULL DEFAULT '[]',
    assign_to JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_assignment_rules_tenant ON assignment_rules(tenant_id);
CREATE INDEX idx_assignment_rules_queue ON assignment_rules(queue_id);

-- Approval requests table
CREATE TABLE IF NOT EXISTS approval_requests (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    rule_id UUID REFERENCES workflow_rules(id),
    requested_from JSONB NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    comments TEXT,
    responded_by UUID REFERENCES users(id),
    responded_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_approval_requests_invoice ON approval_requests(invoice_id);
CREATE INDEX idx_approval_requests_status ON approval_requests(tenant_id, status);
CREATE INDEX idx_approval_requests_expires ON approval_requests(expires_at) WHERE status = 'pending';

-- Email action tokens table for secure email-based actions
CREATE TABLE IF NOT EXISTS email_action_tokens (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    action_type VARCHAR(100) NOT NULL,
    resource_type VARCHAR(100) NOT NULL,
    resource_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    metadata JSONB DEFAULT '{}',
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_email_tokens_hash ON email_action_tokens(token_hash);
CREATE INDEX idx_email_tokens_expires ON email_action_tokens(expires_at) WHERE used_at IS NULL;
CREATE INDEX idx_email_tokens_user ON email_action_tokens(user_id);

-- Approval delegation table
CREATE TABLE IF NOT EXISTS approval_delegations (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    delegator_id UUID NOT NULL REFERENCES users(id),
    delegate_id UUID NOT NULL REFERENCES users(id),
    start_date TIMESTAMPTZ NOT NULL,
    end_date TIMESTAMPTZ NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    conditions JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_delegations_delegator ON approval_delegations(delegator_id);
CREATE INDEX idx_delegations_delegate ON approval_delegations(delegate_id);
CREATE INDEX idx_delegations_active ON approval_delegations(tenant_id, is_active);

-- Audit log for workflow actions
CREATE TABLE IF NOT EXISTS workflow_audit_log (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    entity_type VARCHAR(100) NOT NULL,
    entity_id UUID NOT NULL,
    action VARCHAR(100) NOT NULL,
    actor_id UUID REFERENCES users(id),
    actor_type VARCHAR(50) NOT NULL,
    old_values JSONB,
    new_values JSONB,
    metadata JSONB DEFAULT '{}',
    ip_address INET,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_entity ON workflow_audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_actor ON workflow_audit_log(actor_id);
CREATE INDEX idx_audit_log_tenant ON workflow_audit_log(tenant_id, created_at);
