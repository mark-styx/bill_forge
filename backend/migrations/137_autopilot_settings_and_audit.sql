-- Migration 137: Autopilot cockpit decision audit table (refs #379)
--
-- Ships the Exception-Only Autopilot Cockpit's per-tenant decision log.
-- Settings are NOT added as a typed struct field; the autopilot handler reads
-- autopilot_threshold / autopilot_enabled_types directly from tenants.settings
-- JSONB (via MetadataDatabase), defaulting threshold to 0.95 and the enabled
-- list to empty when the keys are absent. The migration backfills those JSONB
-- keys when run against the metadata DB (via the migrate.rs binary); the
-- statements are wrapped in exception guards so the same file is safe to apply
-- against tenant databases (where the `tenants` table does not exist).
--
-- The autopilot_decisions table lives in tenant databases so it shares RLS
-- isolation with invoice_audit_log (migration 076) and surfaces alongside the
-- rest of the per-tenant exception data.

-- ---------------------------------------------------------------------------
-- 1. Backfill default autopilot settings into tenants.settings JSONB
--    (metadata-DB only; silently skipped on tenant DBs that lack tenants).
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'tenants'
          AND table_schema = current_schema()
    ) THEN
        UPDATE tenants
        SET settings = settings
            || jsonb_build_object('autopilot_threshold', 0.95)
            || jsonb_build_object('autopilot_enabled_types', '[]'::jsonb)
        WHERE NOT (settings ? 'autopilot_threshold');
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- 2. autopilot_decisions table (tenant-scoped)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS autopilot_decisions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    -- Composite key back to the originating exception. exception_id is the
    -- canonical id surfaced by GET /autopilot/queue (e.g. "ocr:<invoice_uuid>"
    -- or "duplicate:<invoice_uuid>"). invoice_id is denormalized for report
    -- drill-down without a join.
    exception_id    TEXT NOT NULL,
    invoice_id      UUID,
    exception_type  TEXT NOT NULL
        CHECK (exception_type IN (
            'missing_po',
            'vendor_mismatch',
            'duplicate',
            'gl_ambiguity',
            'policy_violation',
            'ocr_low_confidence'
        )),
    -- 'confirm' = human accepted the proposed resolution.
    -- 'override' = human chose a different action.
    -- 'auto_resolved' = background sweep applied it (confidence >= threshold).
    decision        TEXT NOT NULL
        CHECK (decision IN ('confirm', 'override', 'auto_resolved')),
    -- The confidence of the proposed resolution at decision time. The Daily
    -- Report uses this to surface "where the model is uncertain".
    confidence      REAL NOT NULL,
    actor_id        UUID,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Report query: counts per (tenant, exception_type, day) bucket.
CREATE INDEX IF NOT EXISTS idx_autopilot_decisions_tenant_day
    ON autopilot_decisions (tenant_id, occurred_at, exception_type);

-- Per-exception lookup (idempotency: don't double-resolve).
CREATE INDEX IF NOT EXISTS idx_autopilot_decisions_exception
    ON autopilot_decisions (tenant_id, exception_id);

-- ---------------------------------------------------------------------------
-- 3. RLS (defense-in-depth; mirrors migration 076/135)
-- ---------------------------------------------------------------------------
ALTER TABLE autopilot_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE autopilot_decisions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_autopilot_decisions ON autopilot_decisions;
CREATE POLICY rls_tenant_autopilot_decisions ON autopilot_decisions
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE autopilot_decisions IS 'Per-tenant autopilot decision audit. One row per confirm/override/auto-resolve. Drives the Daily Report endpoint (GET /autopilot/report).';
