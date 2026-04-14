-- Migration 080: Enable Row Level Security on core tenant tables
--
-- Defense-in-depth: ensures queries cannot return rows for the wrong tenant_id
-- even if application code forgets WHERE tenant_id = $1.
--
-- The application sets app.current_tenant_id as a session variable on each
-- connection after auth resolves the tenant.  RLS policies then filter all
-- reads (USING) and writes (WITH CHECK) to rows matching that session value.
--
-- NOTE: FORCE ROW LEVEL SECURITY is intentionally NOT used here.  Until the
-- connection-pool plumbing that issues SET app.current_tenant_id per request
-- is landed, FORCE RLS would block the app role entirely (the session var is
-- NULL, so tenant_id = NULL is never true).  Once that plumbing ships, add
-- FORCE ROW LEVEL SECURITY back as a separate migration.
--
-- Uses DROP IF EXISTS / CREATE POLICY for idempotency across re-runs.

-- ---------------------------------------------------------------------------
-- INVOICES
-- ---------------------------------------------------------------------------
ALTER TABLE invoices ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_invoices ON invoices;
CREATE POLICY rls_tenant_invoices ON invoices
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- ---------------------------------------------------------------------------
-- USERS
-- ---------------------------------------------------------------------------
ALTER TABLE users ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_users ON users;
CREATE POLICY rls_tenant_users ON users
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- ---------------------------------------------------------------------------
-- VENDORS
-- ---------------------------------------------------------------------------
ALTER TABLE vendors ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_vendors ON vendors;
CREATE POLICY rls_tenant_vendors ON vendors
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
