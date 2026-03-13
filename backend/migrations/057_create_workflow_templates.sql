-- Workflow Templates: Multi-step pipeline definitions for invoice processing
-- Each template defines an ordered sequence of stages that invoices flow through

CREATE TABLE IF NOT EXISTS workflow_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_default BOOLEAN NOT NULL DEFAULT false,
    stages JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_workflow_templates_tenant ON workflow_templates(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workflow_templates_active ON workflow_templates(tenant_id, is_active);
CREATE INDEX IF NOT EXISTS idx_workflow_templates_default ON workflow_templates(tenant_id, is_default) WHERE is_default = true;
