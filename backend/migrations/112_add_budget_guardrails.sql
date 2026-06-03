-- Budget guardrails: per-department, cost-center, GL-code, and project budgets
-- configured by finance, with live remaining-balance checks at approval time.

CREATE TABLE IF NOT EXISTS budgets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('department', 'cost_center', 'gl_account', 'project')),
    scope_value TEXT NOT NULL,
    period_type TEXT NOT NULL CHECK (period_type IN ('monthly', 'quarterly', 'annual')),
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    amount_cents BIGINT NOT NULL CHECK (amount_cents >= 0),
    enforcement TEXT NOT NULL DEFAULT 'warn' CHECK (enforcement IN ('warn', 'block')),
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, scope_type, scope_value, period_start, period_end)
);

CREATE INDEX idx_budgets_tenant_scope ON budgets (tenant_id, scope_type, scope_value, period_start, period_end);
