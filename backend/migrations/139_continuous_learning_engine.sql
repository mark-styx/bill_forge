-- Migration 139: Per-tenant continuous learning engine (refs #404)
--
-- Closes the gap that AP corrections (GL re-coding, approver re-routing,
-- autopilot overrides, duplicate dismissals) are fragmented across separate
-- audit/feedback surfaces with no unified stream, no model versioning, and
-- no tenant-facing "what I learned this week" view.
--
-- Three tenant-scoped tables:
--   * learning_corrections    — unified correction ingestion stream
--   * tenant_model_versions   — versioned per-kind model snapshots
--   * tenant_weekly_insights  — materialized weekly insights for the UI panel
--
-- Tenant isolation: every table carries `tenant_id UUID NOT NULL` and is
-- protected by RLS. The tables live in tenant databases (one row per
-- correction/version/insight), mirroring the autopilot_decisions pattern
-- introduced by migration 137.

-- ---------------------------------------------------------------------------
-- 1. learning_corrections — every correction the human applied
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS learning_corrections (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    -- Type of correction; CHECK keeps the stream typed.
    correction_type TEXT NOT NULL
        CHECK (correction_type IN (
            'gl_recode',
            'approver_reroute',
            'autopilot_override',
            'duplicate_dismissal'
        )),
    source_entity_id   UUID,
    source_entity_type TEXT NOT NULL,
    original_value  JSONB NOT NULL DEFAULT '{}'::jsonb,
    corrected_value JSONB NOT NULL DEFAULT '{}'::jsonb,
    user_id         UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_learning_corrections_tenant_created
    ON learning_corrections (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_learning_corrections_tenant_kind
    ON learning_corrections (tenant_id, correction_type);

ALTER TABLE learning_corrections ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_corrections FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_learning_corrections ON learning_corrections;
CREATE POLICY rls_tenant_learning_corrections ON learning_corrections
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE learning_corrections IS
    'Unified per-tenant correction stream that feeds the ContinuousLearningEngine. Every GL recode, approver reroute, autopilot override, and duplicate dismissal is ingested here.';

-- ---------------------------------------------------------------------------
-- 2. tenant_model_versions — versioned snapshots replacing in-place overwrite
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tenant_model_versions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL,
    model_kind          TEXT NOT NULL
        CHECK (model_kind IN (
            'categorization',
            'routing',
            'vendor_matching',
            'confidence'
        )),
    version             INTEGER NOT NULL,
    snapshot            JSONB NOT NULL DEFAULT '{}'::jsonb,
    corrections_applied INTEGER NOT NULL DEFAULT 0,
    baseline_metric     DOUBLE PRECISION,
    new_metric          DOUBLE PRECISION,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    activated_at        TIMESTAMPTZ,
    UNIQUE (tenant_id, model_kind, version)
);

CREATE INDEX IF NOT EXISTS idx_tenant_model_versions_tenant_kind
    ON tenant_model_versions (tenant_id, model_kind, version DESC);

ALTER TABLE tenant_model_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_model_versions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_model_versions ON tenant_model_versions;
CREATE POLICY rls_tenant_model_versions ON tenant_model_versions
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE tenant_model_versions IS
    'Versioned per-tenant per-kind model snapshots. Replaces the legacy in-place overwrite pattern so each weekly learning pass produces a new immutable row with baseline vs new metric.';

-- ---------------------------------------------------------------------------
-- 3. tenant_weekly_insights — materialized "what I learned this week"
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tenant_weekly_insights (
    tenant_id    UUID NOT NULL,
    week_start   DATE NOT NULL,
    insights     JSONB NOT NULL DEFAULT '{}'::jsonb,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, week_start)
);

CREATE INDEX IF NOT EXISTS idx_tenant_weekly_insights_tenant_week
    ON tenant_weekly_insights (tenant_id, week_start DESC);

ALTER TABLE tenant_weekly_insights ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_weekly_insights FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_weekly_insights ON tenant_weekly_insights;
CREATE POLICY rls_tenant_weekly_insights ON tenant_weekly_insights
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE tenant_weekly_insights IS
    'Per-tenant materialized weekly learning insights surfaced by the "What I Learned This Week" panel.';
