-- Migration 121: Enable + Force RLS on tenant_db-created tables
--
-- Closes the RLS coverage gap for every tenant-scoped table created by
-- tenant_db.rs that was not already covered by migrations 076/080/089/092/120.
--
-- Uses the NULLIF-hardened policy pattern from migration 092 so that an unset
-- or empty app.current_tenant_id denies rows instead of raising a UUID cast
-- error.  Each table is wrapped in its own DO block with existence checks so
-- the migration is safe on databases where a given tenant table has not yet
-- been provisioned (tenant_db.rs creates tables lazily per tenant).

-- ===========================================================================
-- Tables with a direct tenant_id column
-- ===========================================================================

-- documents
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'documents') THEN
        EXECUTE 'ALTER TABLE documents ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE documents FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_documents ON documents';
        EXECUTE 'CREATE POLICY rls_tenant_documents ON documents
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- audit_log
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'audit_log') THEN
        EXECUTE 'ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE audit_log FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_audit_log ON audit_log';
        EXECUTE 'CREATE POLICY rls_tenant_audit_log ON audit_log
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- invoice_status_config
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'invoice_status_config') THEN
        EXECUTE 'ALTER TABLE invoice_status_config ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE invoice_status_config FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_invoice_status_config ON invoice_status_config';
        EXECUTE 'CREATE POLICY rls_tenant_invoice_status_config ON invoice_status_config
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- approval_limits
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'approval_limits') THEN
        EXECUTE 'ALTER TABLE approval_limits ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE approval_limits FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_approval_limits ON approval_limits';
        EXECUTE 'CREATE POLICY rls_tenant_approval_limits ON approval_limits
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- edi_connections
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'edi_connections') THEN
        EXECUTE 'ALTER TABLE edi_connections ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE edi_connections FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_edi_connections ON edi_connections';
        EXECUTE 'CREATE POLICY rls_tenant_edi_connections ON edi_connections
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- edi_documents
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'edi_documents') THEN
        EXECUTE 'ALTER TABLE edi_documents ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE edi_documents FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_edi_documents ON edi_documents';
        EXECUTE 'CREATE POLICY rls_tenant_edi_documents ON edi_documents
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- edi_trading_partners
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'edi_trading_partners') THEN
        EXECUTE 'ALTER TABLE edi_trading_partners ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE edi_trading_partners FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_edi_trading_partners ON edi_trading_partners';
        EXECUTE 'CREATE POLICY rls_tenant_edi_trading_partners ON edi_trading_partners
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- edi_webhook_nonces
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'edi_webhook_nonces') THEN
        EXECUTE 'ALTER TABLE edi_webhook_nonces ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE edi_webhook_nonces FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_edi_webhook_nonces ON edi_webhook_nonces';
        EXECUTE 'CREATE POLICY rls_tenant_edi_webhook_nonces ON edi_webhook_nonces
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- vendor_statements (has both tenant_id and vendor_id)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'vendor_statements') THEN
        EXECUTE 'ALTER TABLE vendor_statements ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE vendor_statements FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_vendor_statements ON vendor_statements';
        EXECUTE 'CREATE POLICY rls_tenant_vendor_statements ON vendor_statements
            USING (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)
            WITH CHECK (tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid)';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- ===========================================================================
-- Tables without a direct tenant_id — use EXISTS subquery
-- ===========================================================================

-- invoice_line_items (has invoice_id, joins to invoices.tenant_id)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'invoice_line_items') THEN
        EXECUTE 'ALTER TABLE invoice_line_items ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE invoice_line_items FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_invoice_line_items ON invoice_line_items';
        EXECUTE 'CREATE POLICY rls_tenant_invoice_line_items ON invoice_line_items
            USING (EXISTS (
                SELECT 1 FROM invoices i
                WHERE i.id = invoice_line_items.invoice_id
                  AND i.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))
            WITH CHECK (EXISTS (
                SELECT 1 FROM invoices i
                WHERE i.id = invoice_line_items.invoice_id
                  AND i.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- vendor_contacts (has vendor_id, joins to vendors.tenant_id)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'vendor_contacts') THEN
        EXECUTE 'ALTER TABLE vendor_contacts ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE vendor_contacts FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_vendor_contacts ON vendor_contacts';
        EXECUTE 'CREATE POLICY rls_tenant_vendor_contacts ON vendor_contacts
            USING (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_contacts.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))
            WITH CHECK (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_contacts.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- vendor_bank_accounts (has vendor_id, joins to vendors.tenant_id)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'vendor_bank_accounts') THEN
        EXECUTE 'ALTER TABLE vendor_bank_accounts ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE vendor_bank_accounts FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_vendor_bank_accounts ON vendor_bank_accounts';
        EXECUTE 'CREATE POLICY rls_tenant_vendor_bank_accounts ON vendor_bank_accounts
            USING (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_bank_accounts.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))
            WITH CHECK (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_bank_accounts.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- vendor_statement_settings (has vendor_id, joins to vendors.tenant_id)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'vendor_statement_settings') THEN
        EXECUTE 'ALTER TABLE vendor_statement_settings ENABLE ROW LEVEL SECURITY';
        EXECUTE 'ALTER TABLE vendor_statement_settings FORCE ROW LEVEL SECURITY';
        EXECUTE 'DROP POLICY IF EXISTS rls_tenant_vendor_statement_settings ON vendor_statement_settings';
        EXECUTE 'CREATE POLICY rls_tenant_vendor_statement_settings ON vendor_statement_settings
            USING (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_statement_settings.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))
            WITH CHECK (EXISTS (
                SELECT 1 FROM vendors v
                WHERE v.id = vendor_statement_settings.vendor_id
                  AND v.tenant_id = NULLIF(current_setting(''app.current_tenant_id'', true), '''')::uuid
            ))';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;

-- ===========================================================================
-- Grant privileges to the app role (mirrors 120_force_rls_and_app_role.sql)
-- ===========================================================================

DO $$
BEGIN
    EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO billforge_app';
EXCEPTION WHEN OTHERS THEN NULL;
END
$$;
