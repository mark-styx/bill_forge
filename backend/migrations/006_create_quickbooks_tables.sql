-- QuickBooks integration tables
-- Stores OAuth tokens, vendor/account mappings, and sync status

-- QuickBooks OAuth state tokens (for CSRF protection during OAuth flow)
CREATE TABLE IF NOT EXISTS quickbooks_oauth_states (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    state_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- QuickBooks OAuth connections (per tenant)
CREATE TABLE IF NOT EXISTS quickbooks_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    company_id TEXT NOT NULL,  -- QuickBooks realm ID
    company_name TEXT,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    refresh_token_expires_at TIMESTAMPTZ NOT NULL,
    environment TEXT NOT NULL DEFAULT 'sandbox',  -- 'sandbox' or 'production'
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id),  -- One QuickBooks connection per tenant
    UNIQUE(tenant_id, company_id)
);

-- Vendor mappings (QuickBooks vendor <-> BillForge vendor)
CREATE TABLE IF NOT EXISTS quickbooks_vendor_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    quickbooks_vendor_id TEXT NOT NULL,  -- QuickBooks vendor ID
    billforge_vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    quickbooks_vendor_name TEXT NOT NULL,
    sync_token TEXT,  -- QuickBooks sync token for updates
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, quickbooks_vendor_id),
    UNIQUE(tenant_id, billforge_vendor_id)
);

-- Account mappings (QuickBooks account <-> BillForge GL code)
CREATE TABLE IF NOT EXISTS quickbooks_account_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    quickbooks_account_id TEXT NOT NULL,  -- QuickBooks account ID
    quickbooks_account_name TEXT NOT NULL,
    quickbooks_account_type TEXT NOT NULL,  -- Expense, Asset, Liability, etc.
    billforge_gl_code TEXT NOT NULL,  -- BillForge GL code
    billforge_department TEXT,  -- Optional department
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, quickbooks_account_id),
    UNIQUE(tenant_id, billforge_gl_code)
);

-- QuickBooks sync history (track all sync operations)
CREATE TABLE IF NOT EXISTS quickbooks_sync_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sync_type TEXT NOT NULL,  -- 'vendors', 'accounts', 'invoice_export'
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

-- Invoice export tracking (which invoices have been exported to QuickBooks)
CREATE TABLE IF NOT EXISTS quickbooks_invoice_exports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    quickbooks_bill_id TEXT NOT NULL,  -- QuickBooks bill ID
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    export_status TEXT NOT NULL DEFAULT 'synced',  -- 'synced', 'failed', 'pending_update'
    last_sync_at TIMESTAMPTZ,
    sync_error TEXT,
    UNIQUE(tenant_id, invoice_id),
    UNIQUE(tenant_id, quickbooks_bill_id)
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_quickbooks_oauth_states_tenant ON quickbooks_oauth_states(tenant_id, expires_at);
CREATE INDEX IF NOT EXISTS idx_quickbooks_connections_tenant ON quickbooks_connections(tenant_id);
CREATE INDEX IF NOT EXISTS idx_quickbooks_vendor_mappings_tenant ON quickbooks_vendor_mappings(tenant_id);
CREATE INDEX IF NOT EXISTS idx_quickbooks_vendor_mappings_qb_vendor ON quickbooks_vendor_mappings(tenant_id, quickbooks_vendor_id);
CREATE INDEX IF NOT EXISTS idx_quickbooks_account_mappings_tenant ON quickbooks_account_mappings(tenant_id);
CREATE INDEX IF NOT EXISTS idx_quickbooks_sync_log_tenant ON quickbooks_sync_log(tenant_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_quickbooks_invoice_exports_tenant ON quickbooks_invoice_exports(tenant_id);
CREATE INDEX IF NOT EXISTS idx_quickbooks_invoice_exports_invoice ON quickbooks_invoice_exports(invoice_id);
