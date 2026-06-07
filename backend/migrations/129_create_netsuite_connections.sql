-- NetSuite ERP integration connection table

CREATE TABLE IF NOT EXISTS netsuite_connections (
    tenant_id UUID PRIMARY KEY,
    account_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret TEXT NOT NULL,      -- TODO: encrypt in production
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Row-Level Security: fail-closed — only the owning tenant can see its row.
-- Uses the NULLIF-hardened pattern from migration 092/121 so an unset or empty
-- app.current_tenant_id denies rows instead of raising a UUID cast error.
ALTER TABLE netsuite_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE netsuite_connections FORCE ROW LEVEL SECURITY;
CREATE POLICY rls_tenant_netsuite_connections ON netsuite_connections
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
