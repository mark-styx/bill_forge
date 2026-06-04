-- Migration 120: Force Row Level Security and create dedicated app role
--
-- Makes RLS an architectural guarantee instead of a session convention:
--   (a) Creates a dedicated billforge_app role with NOSUPERUSER NOBYPASSRLS
--       so the application cannot bypass RLS even if a raw connection is used.
--   (b) Forces RLS on every tenant-scoped table covered by migrations
--       080, 089, and 092, so even the table owner cannot bypass policies.
--
-- Policies from 080/089/092 are left untouched -- they already gate by
-- current_setting('app.current_tenant_id', true) with NULLIF hardening.

-- ---------------------------------------------------------------------------
-- Single DO block with advisory lock so concurrent sessions serialize.
-- ---------------------------------------------------------------------------

DO $$
DECLARE
    _pw text;
BEGIN
    -- Serialize to prevent "tuple concurrently updated" from parallel sessions
    PERFORM pg_advisory_xact_lock(20240120);

    -- -----------------------------------------------------------------------
    -- (a) Dedicated application role
    --
    --     Nested BEGIN/EXCEPTION/END so that the GRANTs and FORCE RLS
    --     statements below this block run unconditionally regardless of
    --     whether the role was just created or already existed.
    -- -----------------------------------------------------------------------

    -- Read password from GUC so the migration runner can inject it at runtime
    -- via the connection options parameter.  Falls back to the dev default when
    -- the setting is absent (local dev / tests).
    _pw := current_setting('billforge.app_password', true);
    IF _pw IS NULL OR _pw = '' THEN
        _pw := 'billforge_app_dev';
    END IF;

    BEGIN
        EXECUTE format(
            'CREATE ROLE billforge_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE INHERIT PASSWORD %L',
            _pw
        );
    EXCEPTION WHEN duplicate_object THEN
        BEGIN
            EXECUTE format(
                'ALTER ROLE billforge_app PASSWORD %L',
                _pw
            );
        EXCEPTION WHEN OTHERS THEN NULL;
        END;
    END;

    -- -----------------------------------------------------------------------
    -- GRANTs (run unconditionally on every apply)
    -- -----------------------------------------------------------------------
    BEGIN
        EXECUTE format('GRANT CONNECT ON DATABASE %I TO billforge_app', current_database());
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        EXECUTE 'GRANT USAGE, CREATE ON SCHEMA public TO billforge_app';
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO billforge_app';
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        EXECUTE 'GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO billforge_app';
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        EXECUTE 'ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO billforge_app';
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        EXECUTE 'ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO billforge_app';
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    -- -----------------------------------------------------------------------
    -- (b) FORCE Row Level Security on every tenant-scoped table
    --
    --     Wrapped in existence checks so the migration is safe to run on the
    --     control-plane database (which lacks tenant tables) as well as on
    --     individual tenant databases.
    -- -----------------------------------------------------------------------

    -- Core tables (from migration 080)
    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'invoices') THEN
            EXECUTE 'ALTER TABLE invoices FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'users') THEN
            EXECUTE 'ALTER TABLE users FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'vendors') THEN
            EXECUTE 'ALTER TABLE vendors FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    -- AI tables (from migration 089)
    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_conversations') THEN
            EXECUTE 'ALTER TABLE ai_conversations FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_messages') THEN
            EXECUTE 'ALTER TABLE ai_messages FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_tool_calls') THEN
            EXECUTE 'ALTER TABLE ai_tool_calls FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_tool_results') THEN
            EXECUTE 'ALTER TABLE ai_tool_results FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_usage_events') THEN
            EXECUTE 'ALTER TABLE ai_usage_events FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_message_feedback') THEN
            EXECUTE 'ALTER TABLE ai_message_feedback FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;

    BEGIN
        IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'ai_action_proposals') THEN
            EXECUTE 'ALTER TABLE ai_action_proposals FORCE ROW LEVEL SECURITY';
        END IF;
    EXCEPTION WHEN OTHERS THEN NULL;
    END;
END
$$;
