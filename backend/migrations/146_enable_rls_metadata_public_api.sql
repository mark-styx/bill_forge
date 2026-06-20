-- Migration 146: Enable + Force RLS on the public-API platform tables
-- (api_keys, webhook_subscriptions, webhook_deliveries) in the shared
-- metadata database.
--
-- Background (#411): migration 110 created these tenant-owned tables in the
-- shared metadata DB but never enabled RLS, leaving tenant isolation entirely
-- dependent on every handler remembering the `WHERE tenant_id = $X` clause.
-- This migration enforces FORCE ROW LEVEL SECURITY with the same
-- NULLIF-hardened policy pattern used by migration 121, so a missing predicate
-- in any current or future handler cannot leak or mutate another tenant's
-- rows.
--
-- Bootstrap carve-out: PAT verification (verify_pat) and prefix-based API key
-- lookup must read api_keys by `token_hash` / `key_prefix` BEFORE the tenant
-- is known. Those two paths set `app.public_api_auth_bootstrap = '1'` for
-- exactly one transaction; every other caller leaves it unset and is
-- constrained to its own tenant_id via `app.current_tenant_id`.

-- ===========================================================================
-- api_keys: direct tenant_id column, plus bootstrap carve-out for auth
-- ===========================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'api_keys') THEN
        EXECUTE 'ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE api_keys FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_api_keys ON api_keys';
        EXECUTE 'CREATE POLICY rls_tenant_api_keys ON api_keys
            USING (
                tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
                OR NULLIF(current_setting(''app.public_api_auth_bootstrap'', true), '''') = ''1''
            )
            WITH CHECK (
                tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            )';
    END IF;
END
$$;

-- ===========================================================================
-- webhook_subscriptions: direct tenant_id column
-- ===========================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'webhook_subscriptions') THEN
        EXECUTE 'ALTER TABLE webhook_subscriptions ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE webhook_subscriptions FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_webhook_subscriptions ON webhook_subscriptions';
        EXECUTE 'CREATE POLICY rls_tenant_webhook_subscriptions ON webhook_subscriptions
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
END
$$;

-- ===========================================================================
-- webhook_deliveries: no tenant_id column; join through subscription
-- ===========================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'webhook_deliveries') THEN
        EXECUTE 'ALTER TABLE webhook_deliveries ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE webhook_deliveries FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_webhook_deliveries ON webhook_deliveries';
        EXECUTE 'CREATE POLICY rls_tenant_webhook_deliveries ON webhook_deliveries
            USING (EXISTS (
                SELECT 1 FROM webhook_subscriptions s
                WHERE s.id = webhook_deliveries.subscription_id
                  AND s.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))
            WITH CHECK (EXISTS (
                SELECT 1 FROM webhook_subscriptions s
                WHERE s.id = webhook_deliveries.subscription_id
                  AND s.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))';
    END IF;
END
$$;

-- ===========================================================================
-- Re-issue privileges to the app role (idempotent; mirrors migrations 120/121)
-- ===========================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'billforge_app') THEN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'api_keys') THEN
            EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON api_keys TO billforge_app';
        END IF;
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'webhook_subscriptions') THEN
            EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON webhook_subscriptions TO billforge_app';
        END IF;
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'webhook_deliveries') THEN
            EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON webhook_deliveries TO billforge_app';
        END IF;
    END IF;
END
$$;
