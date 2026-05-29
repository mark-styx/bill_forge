-- Harden core RLS policies so an unset or reset tenant setting denies rows
-- instead of raising an invalid UUID cast error.

DROP POLICY IF EXISTS rls_tenant_invoices ON invoices;
CREATE POLICY rls_tenant_invoices ON invoices
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

DROP POLICY IF EXISTS rls_tenant_users ON users;
CREATE POLICY rls_tenant_users ON users
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

DROP POLICY IF EXISTS rls_tenant_vendors ON vendors;
CREATE POLICY rls_tenant_vendors ON vendors
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
