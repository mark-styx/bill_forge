-- AP-to-AP Migration Importer tables (refs #405)
--
-- One-shot ingestion path for BILL.com / Coupa export bundles. A bundle row
-- represents an uploaded ZIP. Preview rows are the staged-but-not-yet-applied
-- entity records derived from the bundle, used to render a side-by-side preview.
-- Audit rows record actor + action for the migration lifecycle.
--
-- All tables are tenant-scoped with RLS enabled and forced, following the
-- pattern in 132_create_vendor_onboarding_submissions.sql and 133_enable_rls_workflow_tables.sql.

-- ---------------------------------------------------------------------------
-- 1. ap_migration_bundle
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ap_migration_bundle (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('bill', 'coupa')),
    status TEXT NOT NULL DEFAULT 'uploaded'
        CHECK (status IN ('uploaded', 'parsed', 'previewed', 'committed', 'failed')),
    uploaded_by UUID NULL REFERENCES users(id),
    uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    original_filename TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    error_text TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ap_migration_bundle_tenant_status
    ON ap_migration_bundle(tenant_id, status, uploaded_at DESC);

-- ---------------------------------------------------------------------------
-- 2. ap_migration_preview (staged entities awaiting commit)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ap_migration_preview (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bundle_id UUID NOT NULL REFERENCES ap_migration_bundle(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    entity_type TEXT NOT NULL
        CHECK (entity_type IN ('vendor', 'invoice', 'approval_workflow', 'gl_mapping', 'approver', 'document')),
    source_payload JSONB NOT NULL,
    target_action TEXT NOT NULL CHECK (target_action IN ('create', 'update', 'skip')),
    target_match_id UUID NULL,
    conflict_reason TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ap_migration_preview_tenant_bundle
    ON ap_migration_preview(tenant_id, bundle_id);
CREATE INDEX IF NOT EXISTS idx_ap_migration_preview_bundle_entity
    ON ap_migration_preview(bundle_id, entity_type);

-- ---------------------------------------------------------------------------
-- 3. ap_migration_audit
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ap_migration_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bundle_id UUID NOT NULL REFERENCES ap_migration_bundle(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    actor_id UUID NULL REFERENCES users(id),
    action TEXT NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ap_migration_audit_tenant_bundle
    ON ap_migration_audit(tenant_id, bundle_id, occurred_at DESC);

-- ---------------------------------------------------------------------------
-- 4. RLS (mirrors 133_enable_rls_workflow_tables.sql)
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    EXECUTE 'ALTER TABLE ap_migration_bundle ENABLE ROW LEVEL SECURITY';
    EXECUTE 'ALTER TABLE ap_migration_bundle FORCE ROW LEVEL SECURITY';
    EXECUTE 'DROP POLICY IF EXISTS rls_tenant_ap_migration_bundle ON ap_migration_bundle';
    EXECUTE 'CREATE POLICY rls_tenant_ap_migration_bundle ON ap_migration_bundle
        USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
        WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';

    EXECUTE 'ALTER TABLE ap_migration_preview ENABLE ROW LEVEL SECURITY';
    EXECUTE 'ALTER TABLE ap_migration_preview FORCE ROW LEVEL SECURITY';
    EXECUTE 'DROP POLICY IF EXISTS rls_tenant_ap_migration_preview ON ap_migration_preview';
    EXECUTE 'CREATE POLICY rls_tenant_ap_migration_preview ON ap_migration_preview
        USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
        WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';

    EXECUTE 'ALTER TABLE ap_migration_audit ENABLE ROW LEVEL SECURITY';
    EXECUTE 'ALTER TABLE ap_migration_audit FORCE ROW LEVEL SECURITY';
    EXECUTE 'DROP POLICY IF EXISTS rls_tenant_ap_migration_audit ON ap_migration_audit';
    EXECUTE 'CREATE POLICY rls_tenant_ap_migration_audit ON ap_migration_audit
        USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
        WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';

    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ap_migration_bundle TO billforge_app';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ap_migration_preview TO billforge_app';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ap_migration_audit TO billforge_app';
EXCEPTION WHEN OTHERS THEN
    -- billforge_app role may not exist in some local envs; the grant is a no-op then.
    NULL;
END
$$;

COMMENT ON TABLE ap_migration_bundle IS 'Uploaded BILL.com / Coupa export bundles for one-click AP-to-AP migration';
COMMENT ON TABLE ap_migration_preview IS 'Staged-but-not-yet-applied entities derived from a bundle, used for side-by-side preview';
COMMENT ON TABLE ap_migration_audit IS 'Audit trail for AP migration bundle lifecycle (upload, parse, commit, cancel)';
