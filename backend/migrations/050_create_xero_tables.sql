-- Xero integration tables
-- Stores OAuth tokens, contact/account mappings, and sync status

-- Xero OAuth state tokens (for CSRF protection during OAuth flow)
CREATE TABLE IF NOT EXISTS xero_oauth_states (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    state_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Xero OAuth connections (per tenant)
CREATE TABLE IF NOT EXISTS xero_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    xero_tenant_id TEXT NOT NULL,  -- Xero organization/tenant ID
    organization_name TEXT,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    refresh_token_expires_at TIMESTAMPTZ,  -- Xero refresh tokens may not expire
    environment TEXT NOT NULL DEFAULT 'production',  -- 'production' or 'sandbox'
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id),  -- One Xero connection per tenant
    UNIQUE(tenant_id, xero_tenant_id)
);

-- Contact mappings (Xero contact <-> BillForge vendor)
CREATE TABLE IF NOT EXISTS xero_contact_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    xero_contact_id TEXT NOT NULL,  -- Xero contact ID
    billforge_vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    xero_contact_name TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, xero_contact_id),
    UNIQUE(tenant_id, billforge_vendor_id)
);

-- Account mappings (Xero account <-> BillForge GL code)
CREATE TABLE IF NOT EXISTS xero_account_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    xero_account_id TEXT NOT NULL,  -- Xero account ID
    xero_account_code TEXT NOT NULL,  -- Xero account code
    xero_account_name TEXT NOT NULL,
    xero_account_type TEXT NOT NULL,  -- EXPENSE, ASSET, LIABILITY, etc.
    billforge_gl_code TEXT NOT NULL,  -- BillForge GL code
    billforge_department TEXT,  -- Optional department
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, xero_account_id),
    UNIQUE(tenant_id, billforge_gl_code)
);

-- Xero sync history (track all sync operations)
CREATE TABLE IF NOT EXISTS xero_sync_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sync_type TEXT NOT NULL,  -- 'contacts', 'accounts', 'invoice_export'
    status TEXT NOT NULL,  -- 'running', 'completed', 'failed'
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    records_processed INTEGER DEFAULT 0,
    records_created INTEGER DEFAULT 0,
    records_updated INTEGER DEFAULT 0,
    records_skipped INTEGER DEFAULT 0,
    error_message TEXT,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Invoice export tracking (which invoices have been exported to Xero)
CREATE TABLE IF NOT EXISTS xero_invoice_exports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    xero_invoice_id TEXT NOT NULL,  -- Xero invoice ID
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    export_status TEXT NOT NULL DEFAULT 'synced',  -- 'synced', 'failed', 'pending_update'
    last_sync_at TIMESTAMPTZ,
    sync_error TEXT,
    UNIQUE(tenant_id, invoice_id),
    UNIQUE(tenant_id, xero_invoice_id)
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_xero_oauth_states_tenant ON xero_oauth_states(tenant_id, expires_at);
CREATE INDEX IF NOT EXISTS idx_xero_connections_tenant ON xero_connections(tenant_id);
CREATE INDEX IF NOT EXISTS idx_xero_contact_mappings_tenant ON xero_contact_mappings(tenant_id);
CREATE INDEX IF NOT EXISTS idx_xero_contact_mappings_xero_contact ON xero_contact_mappings(tenant_id, xero_contact_id);
CREATE INDEX IF NOT EXISTS idx_xero_account_mappings_tenant ON xero_account_mappings(tenant_id);
CREATE INDEX IF NOT EXISTS idx_xero_sync_log_tenant ON xero_sync_log(tenant_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_xero_invoice_exports_tenant ON xero_invoice_exports(tenant_id);
CREATE INDEX IF NOT EXISTS idx_xero_invoice_exports_invoice ON xero_invoice_exports(invoice_id);
