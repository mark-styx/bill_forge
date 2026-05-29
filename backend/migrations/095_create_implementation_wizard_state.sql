-- Server-backed state for the two-week implementation wizard.

CREATE TABLE IF NOT EXISTS implementation_wizard_states (
    tenant_id UUID PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_implementation_wizard_states_updated_at
    ON implementation_wizard_states(updated_at DESC);
