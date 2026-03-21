-- Migration 065: Enhanced Approval Chains — multi-level, threshold-based, with escalation
-- Extends the existing approval_requests and workflow_rules with structured chain support

-- Approval policies: define how approvals work per tenant
CREATE TABLE IF NOT EXISTS approval_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_default BOOLEAN NOT NULL DEFAULT false,
    
    -- Matching criteria (which invoices this policy applies to)
    match_criteria JSONB NOT NULL DEFAULT '{}',   -- {min_amount, max_amount, departments, vendors, cost_centers}
    priority INTEGER NOT NULL DEFAULT 0,          -- higher priority = evaluated first
    
    -- Policy configuration
    require_sequential BOOLEAN NOT NULL DEFAULT true,  -- must approve in order vs any order
    require_all_levels BOOLEAN NOT NULL DEFAULT true,  -- all levels must approve vs any single level
    allow_self_approval BOOLEAN NOT NULL DEFAULT false,
    auto_approve_below_cents BIGINT,              -- auto-approve invoices below this amount
    
    -- Escalation settings
    escalation_enabled BOOLEAN NOT NULL DEFAULT true,
    escalation_timeout_hours INTEGER DEFAULT 48,  -- hours before escalating to next level
    final_escalation_user_id UUID REFERENCES users(id), -- ultimate escalation target (e.g., CFO)
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_approval_policies_tenant ON approval_policies(tenant_id);
CREATE INDEX idx_approval_policies_active ON approval_policies(tenant_id, is_active, priority DESC);

-- Approval chain levels: ordered steps within a policy
CREATE TABLE IF NOT EXISTS approval_chain_levels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id UUID NOT NULL REFERENCES approval_policies(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    
    level_order INTEGER NOT NULL,                 -- 1, 2, 3... (execution order)
    name VARCHAR(255) NOT NULL,                   -- "Manager Approval", "Director Approval", etc.
    
    -- Who can approve at this level
    approver_type TEXT NOT NULL,                   -- user, role, department_head, amount_authority
    approver_user_ids JSONB DEFAULT '[]',          -- specific user IDs
    approver_role TEXT,                            -- role name (e.g., 'ap_manager', 'controller')
    
    -- Threshold: this level only required if amount >= threshold
    min_amount_cents BIGINT DEFAULT 0,
    max_amount_cents BIGINT,                      -- NULL = no upper limit
    
    -- Approval requirements
    required_approver_count INTEGER NOT NULL DEFAULT 1,  -- how many approvers needed at this level
    
    -- Timeout/escalation for this specific level
    timeout_hours INTEGER,                        -- NULL = use policy default
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(policy_id, level_order)
);

CREATE INDEX idx_chain_levels_policy ON approval_chain_levels(policy_id);
CREATE INDEX idx_chain_levels_tenant ON approval_chain_levels(tenant_id);

-- Active approval chains: tracks a running approval for a specific invoice
CREATE TABLE IF NOT EXISTS active_approval_chains (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES approval_policies(id),
    
    -- Current state
    status TEXT NOT NULL DEFAULT 'in_progress',   -- in_progress, approved, rejected, escalated, cancelled
    current_level INTEGER NOT NULL DEFAULT 1,     -- which level we're at
    total_levels INTEGER NOT NULL,                -- total levels in this chain
    
    -- Outcome
    final_decision TEXT,                          -- approved, rejected
    final_decided_by UUID REFERENCES users(id),
    final_decided_at TIMESTAMPTZ,
    
    -- Escalation tracking
    escalation_count INTEGER NOT NULL DEFAULT 0,
    last_escalated_at TIMESTAMPTZ,
    
    -- Metadata
    initiated_by UUID NOT NULL REFERENCES users(id),
    initiated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_active_chains_tenant ON active_approval_chains(tenant_id);
CREATE INDEX idx_active_chains_invoice ON active_approval_chains(invoice_id);
CREATE INDEX idx_active_chains_status ON active_approval_chains(tenant_id, status);

-- Individual approval steps within an active chain
CREATE TABLE IF NOT EXISTS approval_chain_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chain_id UUID NOT NULL REFERENCES active_approval_chains(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    level_id UUID NOT NULL REFERENCES approval_chain_levels(id),
    
    level_order INTEGER NOT NULL,
    
    -- Assigned approver
    assigned_to UUID NOT NULL REFERENCES users(id),
    
    -- Status
    status TEXT NOT NULL DEFAULT 'pending',       -- pending, approved, rejected, skipped, escalated, delegated
    decision TEXT,                                -- approve, reject
    comments TEXT,
    
    -- Delegation
    delegated_to UUID REFERENCES users(id),
    delegated_at TIMESTAMPTZ,
    delegation_reason TEXT,
    
    -- Timing
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    due_at TIMESTAMPTZ,                           -- when this step should be completed by
    responded_at TIMESTAMPTZ,
    
    -- Escalation
    escalated_at TIMESTAMPTZ,
    escalated_to UUID REFERENCES users(id),
    escalation_reason TEXT,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_chain_steps_chain ON approval_chain_steps(chain_id);
CREATE INDEX idx_chain_steps_assigned ON approval_chain_steps(assigned_to, status);
CREATE INDEX idx_chain_steps_pending ON approval_chain_steps(tenant_id, status, due_at) WHERE status = 'pending';

-- Approval activity log (immutable audit trail)
CREATE TABLE IF NOT EXISTS approval_activity_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    chain_id UUID NOT NULL REFERENCES active_approval_chains(id) ON DELETE CASCADE,
    step_id UUID REFERENCES approval_chain_steps(id),
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    
    -- Activity details
    action TEXT NOT NULL,                         -- submitted, approved, rejected, escalated, delegated, commented, recalled
    actor_id UUID NOT NULL REFERENCES users(id),
    actor_role TEXT,
    
    -- Context
    comments TEXT,
    metadata JSONB DEFAULT '{}',                  -- additional context (amount, vendor, etc.)
    ip_address INET,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_approval_activity_chain ON approval_activity_log(chain_id);
CREATE INDEX idx_approval_activity_invoice ON approval_activity_log(invoice_id);
CREATE INDEX idx_approval_activity_actor ON approval_activity_log(actor_id);
CREATE INDEX idx_approval_activity_tenant ON approval_activity_log(tenant_id, created_at DESC);
