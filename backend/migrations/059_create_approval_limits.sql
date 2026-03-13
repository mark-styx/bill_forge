-- Create approval_limits table
CREATE TABLE IF NOT EXISTS approval_limits (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    max_amount_cents BIGINT NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    vendor_restrictions JSONB,
    department_restrictions JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_approval_limits_tenant ON approval_limits(tenant_id);
CREATE INDEX idx_approval_limits_user ON approval_limits(tenant_id, user_id);
