-- Migration 051: Add intelligent routing tables
-- Sprint 13 Feature #7: Intelligent Approval Routing

-- Approver workload tracking
CREATE TABLE IF NOT EXISTS approver_workload_tracking (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    -- Current workload metrics
    active_approvals INTEGER NOT NULL DEFAULT 0,
    pending_approvals INTEGER NOT NULL DEFAULT 0,
    completed_this_week INTEGER NOT NULL DEFAULT 0,
    completed_this_month INTEGER NOT NULL DEFAULT 0,
    -- Performance metrics
    avg_approval_time_hours DECIMAL(10, 2),
    approval_rate DECIMAL(5, 4), -- 0.0 to 1.0
    -- Workload score (calculated composite, higher = more loaded)
    workload_score DECIMAL(10, 4) NOT NULL DEFAULT 0.0,
    -- Timestamps
    last_assignment_at TIMESTAMPTZ,
    last_approval_at TIMESTAMPTZ,
    metrics_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_workload_tracking_tenant_user ON approver_workload_tracking(tenant_id, user_id);
CREATE INDEX idx_workload_tracking_score ON approver_workload_tracking(tenant_id, workload_score);
CREATE INDEX idx_workload_tracking_updated ON approver_workload_tracking(metrics_updated_at);

-- Routing optimization log
CREATE TABLE IF NOT EXISTS routing_optimization_log (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    -- Routing decision details
    queue_id UUID NOT NULL REFERENCES work_queues(id),
    routing_strategy VARCHAR(100) NOT NULL, -- 'least_loaded', 'round_robin', 'vendor_expert', 'availability_based'
    selected_approver_id UUID REFERENCES users(id),
    -- Candidates considered
    candidate_approvers JSONB NOT NULL DEFAULT '[]', -- [{"user_id": "...", "score": 0.9, "reason": "least_loaded"}]
    -- Routing factors
    routing_factors JSONB NOT NULL DEFAULT '{}', -- {"workload_weight": 0.4, "expertise_weight": 0.3, "availability_weight": 0.3}
    -- Outcome tracking
    outcome VARCHAR(50), -- 'approved', 'rejected', 'escalated', 'timeout'
    time_to_decision_hours DECIMAL(10, 2),
    -- Learning signals
    was_optimal BOOLEAN, -- Did this routing meet SLA?
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    outcome_recorded_at TIMESTAMPTZ
);

CREATE INDEX idx_routing_log_tenant ON routing_optimization_log(tenant_id);
CREATE INDEX idx_routing_log_invoice ON routing_optimization_log(invoice_id);
CREATE INDEX idx_routing_log_approver ON routing_optimization_log(selected_approver_id);
CREATE INDEX idx_routing_log_strategy ON routing_optimization_log(tenant_id, routing_strategy);
CREATE INDEX idx_routing_log_created ON routing_optimization_log(created_at);

-- Approver availability calendar
CREATE TABLE IF NOT EXISTS approver_availability (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    -- Availability status
    status VARCHAR(50) NOT NULL DEFAULT 'available', -- 'available', 'busy', 'out_of_office', 'vacation'
    -- Time window
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ NOT NULL,
    -- Delegation
    delegate_id UUID REFERENCES users(id),
    -- Calendar integration source
    calendar_source VARCHAR(50), -- 'google', 'outlook', 'manual'
    calendar_event_id VARCHAR(255),
    -- Metadata
    reason TEXT,
    is_working_hours BOOLEAN NOT NULL DEFAULT true,
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_availability_tenant_user ON approver_availability(tenant_id, user_id);
CREATE INDEX idx_availability_time ON approver_availability(start_at, end_at);
CREATE INDEX idx_availability_status ON approver_availability(tenant_id, status, start_at, end_at);
CREATE INDEX idx_availability_calendar ON approver_availability(calendar_source, calendar_event_id);

-- Approver expertise/vendor patterns
CREATE TABLE IF NOT EXISTS approver_expertise (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    -- Expertise area
    expertise_type VARCHAR(100) NOT NULL, -- 'vendor', 'department', 'gl_code', 'amount_range'
    expertise_key VARCHAR(255) NOT NULL, -- vendor_id, department name, gl code, etc.
    -- Performance metrics for this expertise
    total_approved INTEGER NOT NULL DEFAULT 0,
    total_rejected INTEGER NOT NULL DEFAULT 0,
    avg_time_hours DECIMAL(10, 2),
    expertise_score DECIMAL(5, 4) NOT NULL DEFAULT 0.5, -- 0.0 to 1.0, learned from outcomes
    -- Timestamps
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint
    UNIQUE(tenant_id, user_id, expertise_type, expertise_key)
);

CREATE INDEX idx_expertise_tenant_user ON approver_expertise(tenant_id, user_id);
CREATE INDEX idx_expertise_type_key ON approver_expertise(expertise_type, expertise_key);
CREATE INDEX idx_expertise_score ON approver_expertise(tenant_id, expertise_type, expertise_key, expertise_score);

-- Routing configuration
CREATE TABLE IF NOT EXISTS routing_configuration (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    -- Strategy weights (should sum to 1.0)
    workload_weight DECIMAL(5, 4) NOT NULL DEFAULT 0.4,
    expertise_weight DECIMAL(5, 4) NOT NULL DEFAULT 0.3,
    availability_weight DECIMAL(5, 4) NOT NULL DEFAULT 0.3,
    -- Thresholds
    max_workload_score DECIMAL(10, 4) NOT NULL DEFAULT 100.0,
    min_expertise_score DECIMAL(5, 4) NOT NULL DEFAULT 0.3,
    -- Features
    enable_auto_delegation BOOLEAN NOT NULL DEFAULT true,
    enable_pattern_learning BOOLEAN NOT NULL DEFAULT true,
    enable_calendar_sync BOOLEAN NOT NULL DEFAULT false,
    -- Working hours (for availability detection)
    working_hours_start TIME NOT NULL DEFAULT '09:00:00',
    working_hours_end TIME NOT NULL DEFAULT '17:00:00',
    working_timezone VARCHAR(50) NOT NULL DEFAULT 'UTC',
    working_days JSONB NOT NULL DEFAULT '[1,2,3,4,5]', -- Monday=1, Sunday=7
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- One config per tenant
    UNIQUE(tenant_id)
);

CREATE INDEX idx_routing_config_tenant ON routing_configuration(tenant_id);
