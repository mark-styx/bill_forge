-- Migration 080: Enable Row Level Security on core tenant tables
--
-- Defense-in-depth: ensures queries cannot return rows for the wrong tenant_id
-- even if application code forgets WHERE tenant_id = $1.
--
-- The application sets app.current_tenant_id as a session variable on each
-- connection after auth resolves the tenant.  RLS policies then filter all
-- reads (USING) and writes (WITH CHECK) to rows matching that session value.
--
-- NOTE: FORCE ROW LEVEL SECURITY is intentionally NOT used here.  The
-- connection-pool plumbing now lands in PgManager::tenant(), which sets
-- app.current_tenant_id on every pooled connection via the after_connect hook.
-- Connections that bypass the pool (e.g. raw acquire) should use
-- PgManager::set_tenant_context().  FORCE RLS is still deferred because the
-- default Docker role is a superuser, which bypasses RLS regardless; adding
-- FORCE ROW LEVEL SECURITY is a separate step that requires a dedicated app role.
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
