-- Natural-language AP query console saved views and delivery hooks.
CREATE TABLE IF NOT EXISTS nl_query_saved_views (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    question TEXT NOT NULL,
    query_kind TEXT NOT NULL,
    filters JSONB NOT NULL DEFAULT '{}'::jsonb,
    columns JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_nl_query_saved_views_tenant
    ON nl_query_saved_views(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS nl_query_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    saved_view_id UUID NOT NULL REFERENCES nl_query_saved_views(id) ON DELETE CASCADE,
    schedule TEXT NOT NULL,
    recipients JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_nl_query_schedules_tenant
    ON nl_query_schedules(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS nl_query_alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    saved_view_id UUID NOT NULL REFERENCES nl_query_saved_views(id) ON DELETE CASCADE,
    condition JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_nl_query_alerts_tenant
    ON nl_query_alerts(tenant_id, created_at DESC);

ALTER TABLE nl_query_saved_views ENABLE ROW LEVEL SECURITY;
ALTER TABLE nl_query_saved_views FORCE ROW LEVEL SECURITY;
ALTER TABLE nl_query_schedules ENABLE ROW LEVEL SECURITY;
ALTER TABLE nl_query_schedules FORCE ROW LEVEL SECURITY;
ALTER TABLE nl_query_alerts ENABLE ROW LEVEL SECURITY;
ALTER TABLE nl_query_alerts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_nl_query_saved_views ON nl_query_saved_views;
CREATE POLICY rls_tenant_nl_query_saved_views ON nl_query_saved_views
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

DROP POLICY IF EXISTS rls_tenant_nl_query_schedules ON nl_query_schedules;
CREATE POLICY rls_tenant_nl_query_schedules ON nl_query_schedules
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

DROP POLICY IF EXISTS rls_tenant_nl_query_alerts ON nl_query_alerts;
CREATE POLICY rls_tenant_nl_query_alerts ON nl_query_alerts
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
