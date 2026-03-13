-- Invoice Status Config: Per-tenant customizable invoice status definitions
-- Allows tenants to customize display labels, colors, and ordering of statuses

CREATE TABLE IF NOT EXISTS invoice_status_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    status_key VARCHAR(50) NOT NULL,
    display_label VARCHAR(100) NOT NULL,
    color VARCHAR(50) NOT NULL DEFAULT 'gray',
    bg_color VARCHAR(50) NOT NULL DEFAULT 'bg-secondary',
    text_color VARCHAR(50) NOT NULL DEFAULT 'text-muted-foreground',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_terminal BOOLEAN NOT NULL DEFAULT false,
    is_active BOOLEAN NOT NULL DEFAULT true,
    category VARCHAR(20) NOT NULL DEFAULT 'processing',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, status_key)
);

CREATE INDEX IF NOT EXISTS idx_invoice_status_config_tenant ON invoice_status_config(tenant_id);
CREATE INDEX IF NOT EXISTS idx_invoice_status_config_active ON invoice_status_config(tenant_id, is_active);
CREATE INDEX IF NOT EXISTS idx_invoice_status_config_category ON invoice_status_config(tenant_id, category);
