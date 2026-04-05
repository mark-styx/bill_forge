-- Payment Request tables

CREATE TABLE IF NOT EXISTS payment_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    request_number TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    vendor_id UUID REFERENCES vendors(id) ON DELETE SET NULL,
    total_amount_cents BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    invoice_count INT NOT NULL DEFAULT 0,
    earliest_due_date DATE,
    latest_due_date DATE,
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    submitted_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, request_number)
);

CREATE TABLE IF NOT EXISTS payment_request_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_request_id UUID NOT NULL REFERENCES payment_requests(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    amount_cents BIGINT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payment_requests_tenant ON payment_requests(tenant_id);
CREATE INDEX IF NOT EXISTS idx_payment_requests_status ON payment_requests(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_payment_requests_vendor ON payment_requests(tenant_id, vendor_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_payment_request_items_unique ON payment_request_items(payment_request_id, invoice_id);
CREATE INDEX IF NOT EXISTS idx_payment_request_items_request ON payment_request_items(payment_request_id);
CREATE INDEX IF NOT EXISTS idx_payment_request_items_invoice ON payment_request_items(invoice_id);
