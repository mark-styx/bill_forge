-- Bill.com AP payments integration tables

-- Connection credentials storage (session-based auth, not OAuth)
CREATE TABLE IF NOT EXISTS bill_com_connections (
    tenant_id UUID PRIMARY KEY,
    org_id TEXT NOT NULL,
    dev_key TEXT NOT NULL,
    user_name TEXT NOT NULL,
    password TEXT NOT NULL,           -- encrypted in production
    environment TEXT NOT NULL DEFAULT 'sandbox',
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Vendor mappings (Bill.com Vendor ↔ BillForge Vendor)
CREATE TABLE IF NOT EXISTS bill_com_vendor_mappings (
    tenant_id UUID NOT NULL,
    bill_com_vendor_id TEXT NOT NULL,
    billforge_vendor_id UUID NOT NULL,
    bill_com_vendor_name TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, bill_com_vendor_id)
);

-- Bill export records (BillForge invoices pushed to Bill.com as bills)
CREATE TABLE IF NOT EXISTS bill_com_bill_exports (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    bill_com_bill_id TEXT,
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    export_status TEXT NOT NULL DEFAULT 'pending',
    sync_error TEXT,
    UNIQUE (tenant_id, invoice_id)
);

-- Payment records (payments executed via Bill.com)
CREATE TABLE IF NOT EXISTS bill_com_payment_records (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    bill_com_payment_id TEXT NOT NULL,
    amount_cents BIGINT NOT NULL,
    process_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Scheduled',
    disbursement_type TEXT NOT NULL DEFAULT 'ACH',
    confirmation_number TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Sync log
CREATE TABLE IF NOT EXISTS bill_com_sync_log (
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

CREATE INDEX IF NOT EXISTS idx_bill_com_sync_log_tenant ON bill_com_sync_log(tenant_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_bill_com_vendor_mappings_vendor ON bill_com_vendor_mappings(billforge_vendor_id);
CREATE INDEX IF NOT EXISTS idx_bill_com_payment_records_tenant ON bill_com_payment_records(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_bill_com_bill_exports_tenant ON bill_com_bill_exports(tenant_id);
