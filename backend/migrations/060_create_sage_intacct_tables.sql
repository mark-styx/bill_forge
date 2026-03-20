-- Sage Intacct integration tables

-- Connection credentials storage
CREATE TABLE IF NOT EXISTS sage_intacct_connections (
    tenant_id UUID PRIMARY KEY,
    company_id TEXT NOT NULL,
    entity_id TEXT,
    sender_id TEXT NOT NULL,
    sender_password TEXT NOT NULL,  -- encrypted in production
    user_id TEXT NOT NULL,
    user_password TEXT NOT NULL,    -- encrypted in production
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Vendor mappings (Sage Intacct vendor ↔ BillForge vendor)
CREATE TABLE IF NOT EXISTS sage_intacct_vendor_mappings (
    tenant_id UUID NOT NULL,
    sage_vendor_id TEXT NOT NULL,
    sage_record_no TEXT NOT NULL,
    billforge_vendor_id UUID NOT NULL,
    sage_vendor_name TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, sage_vendor_id)
);

-- GL account mappings
CREATE TABLE IF NOT EXISTS sage_intacct_account_mappings (
    tenant_id UUID NOT NULL,
    sage_account_no TEXT NOT NULL,
    sage_account_title TEXT NOT NULL,
    sage_account_type TEXT NOT NULL,
    billforge_gl_code TEXT,
    billforge_department TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, sage_account_no)
);

-- Invoice export records
CREATE TABLE IF NOT EXISTS sage_intacct_invoice_exports (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    sage_record_no TEXT,
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    export_status TEXT NOT NULL DEFAULT 'pending',
    sync_error TEXT,
    UNIQUE (tenant_id, invoice_id)
);

-- Sync log
CREATE TABLE IF NOT EXISTS sage_intacct_sync_log (
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

CREATE INDEX IF NOT EXISTS idx_sage_intacct_sync_log_tenant ON sage_intacct_sync_log(tenant_id, started_at DESC);
