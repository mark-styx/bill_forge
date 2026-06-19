-- Migration 133: Enable + Force RLS on migration-005 workflow tables
--
-- Closes the RLS coverage gap for the three tenant-scoped workflow tables
-- created in migration 005 (approval_requests, queue_items,
-- approval_delegations) that no prior RLS migration (076/080/089/092/120/121)
-- covered.
--
-- Issue #368: bare-id UPDATEs in approval_expiry::mark_sla_alert_sent
-- (`UPDATE approval_requests ... WHERE id = $1`) and the approve/reject
-- UPDATEs in routes/workflows.rs were never tenant-scoped at the SQL layer;
-- queue_items UPDATEs in invoices.rs / ocr_processing.rs / workflow_repo.rs
-- are inconsistent. With RLS forced, the tenant pool's
-- `app.current_tenant_id` setting silently scopes every WHERE clause, so the
-- dangerous bare-id UPDATEs no-op against foreign tenants instead of
-- mutating their rows.
--
-- Tenant_id columns were widened to UUID by migration 093, so the same
-- NULLIF-hardened policy pattern from migrations 092/121 works here without
-- cast errors when the setting is unset or empty.

-- ===========================================================================
-- Tables with a direct UUID tenant_id column
-- ===========================================================================

-- approval_requests
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'approval_requests') THEN
        EXECUTE 'ALTER TABLE approval_requests ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE approval_requests FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_approval_requests ON approval_requests';
        EXECUTE 'CREATE POLICY rls_tenant_approval_requests ON approval_requests
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
END
$$;

-- queue_items
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'queue_items') THEN
        EXECUTE 'ALTER TABLE queue_items ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE queue_items FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_queue_items ON queue_items';
        EXECUTE 'CREATE POLICY rls_tenant_queue_items ON queue_items
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
END
$$;

-- approval_delegations
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'approval_delegations') THEN
        EXECUTE 'ALTER TABLE approval_delegations ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE approval_delegations FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_approval_delegations ON approval_delegations';
        EXECUTE 'CREATE POLICY rls_tenant_approval_delegations ON approval_delegations
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
END
$$;

-- ===========================================================================
-- Grant privileges to the app role (mirrors 120_force_rls_and_app_role.sql)
-- ===========================================================================

DO $$
BEGIN
    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO billforge_app';
END
$$;
