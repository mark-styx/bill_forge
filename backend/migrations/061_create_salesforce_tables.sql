-- Salesforce CRM integration tables

-- OAuth state tokens (temporary, for CSRF protection during OAuth flow)
CREATE TABLE IF NOT EXISTS salesforce_oauth_states (
    tenant_id UUID PRIMARY KEY,
    state_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Connection storage (OAuth tokens + instance info)
CREATE TABLE IF NOT EXISTS salesforce_connections (
    tenant_id UUID PRIMARY KEY,
    instance_url TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    org_id TEXT,
    sync_enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Account-to-vendor mappings (Salesforce Account ↔ BillForge Vendor)
CREATE TABLE IF NOT EXISTS salesforce_account_mappings (
    tenant_id UUID NOT NULL,
    salesforce_account_id TEXT NOT NULL,
    billforge_vendor_id UUID NOT NULL,
    salesforce_account_name TEXT NOT NULL,
    salesforce_account_type TEXT,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, salesforce_account_id)
);

-- Sync log
CREATE TABLE IF NOT EXISTS salesforce_sync_log (
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

CREATE INDEX IF NOT EXISTS idx_salesforce_sync_log_tenant ON salesforce_sync_log(tenant_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_salesforce_account_mappings_vendor ON salesforce_account_mappings(billforge_vendor_id);
