-- Workday Financial Management integration tables

-- OAuth state tokens (temporary, for CSRF protection during OAuth flow)
CREATE TABLE IF NOT EXISTS workday_oauth_states (
    tenant_id UUID PRIMARY KEY,
    state_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Connection storage (OAuth tokens + tenant info)
CREATE TABLE IF NOT EXISTS workday_connections (
    tenant_id UUID PRIMARY KEY,
    workday_tenant_url TEXT NOT NULL,
    workday_tenant_name TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Supplier-to-vendor mappings (Workday Supplier ↔ BillForge Vendor)
CREATE TABLE IF NOT EXISTS workday_supplier_mappings (
    tenant_id UUID NOT NULL,
    workday_supplier_id TEXT NOT NULL,
    billforge_vendor_id UUID NOT NULL,
    workday_supplier_name TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, workday_supplier_id)
);

-- Ledger account mappings
CREATE TABLE IF NOT EXISTS workday_account_mappings (
    tenant_id UUID NOT NULL,
    workday_account_id TEXT NOT NULL,
    workday_account_name TEXT NOT NULL,
    workday_account_type TEXT NOT NULL,
    billforge_gl_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, workday_account_id)
);

-- Invoice export records
CREATE TABLE IF NOT EXISTS workday_invoice_exports (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    workday_invoice_id TEXT,
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    export_status TEXT NOT NULL DEFAULT 'pending',
    sync_error TEXT,
    UNIQUE (tenant_id, invoice_id)
);

-- Sync log
CREATE TABLE IF NOT EXISTS workday_sync_log (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    sync_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    records_processed INT DEFAULT 0,
    records_created INT DEFAULT 0,
    records_updated INT DEFAULT 0,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_workday_sync_log_tenant ON workday_sync_log(tenant_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_workday_supplier_mappings_vendor ON workday_supplier_mappings(billforge_vendor_id);
