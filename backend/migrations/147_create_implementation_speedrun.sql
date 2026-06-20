-- Issue #415: 2-hour Implementation Speedrun progress state.
--
-- Lives in the tenant DB alongside implementation_wizard_states (migration 095)
-- but tracked separately so the speedrun does not disturb the existing 14-day
-- wizard's serialized JSON blob.
--
-- target_minutes defaults to 120 (the under-2-hour promise from the issue);
-- first_invoices_processed counts toward the "process your first 5 invoices"
-- walkthrough, with completed_at stamped when the fifth invoice lands.

CREATE TABLE IF NOT EXISTS implementation_speedrun_state (
    tenant_id UUID PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    thresholds_inferred_at TIMESTAMPTZ,
    approval_chain_suggested_at TIMESTAMPTZ,
    first_invoices_processed INT NOT NULL DEFAULT 0,
    completed_at TIMESTAMPTZ,
    target_minutes INT NOT NULL DEFAULT 120,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS hardening mirrors migration 121's NULLIF pattern so an unset
-- app.current_tenant_id denies rather than raising a UUID cast error.
ALTER TABLE implementation_speedrun_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE implementation_speedrun_state FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS rls_tenant_implementation_speedrun_state
    ON implementation_speedrun_state;
CREATE POLICY rls_tenant_implementation_speedrun_state
    ON implementation_speedrun_state
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

DO $$
BEGIN
    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE
             ON implementation_speedrun_state TO billforge_app';
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;
