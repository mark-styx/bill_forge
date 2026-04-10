-- Vendor Statement Reconciliation tables

CREATE TABLE IF NOT EXISTS vendor_statements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    statement_number TEXT,
    statement_date DATE NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    opening_balance_cents BIGINT NOT NULL DEFAULT 0,
    closing_balance_cents BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    status TEXT NOT NULL DEFAULT 'pending',
    reconciled_by UUID REFERENCES users(id),
    reconciled_at TIMESTAMPTZ,
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_statements_tenant ON vendor_statements(tenant_id);
CREATE INDEX IF NOT EXISTS idx_vendor_statements_vendor ON vendor_statements(vendor_id);
CREATE INDEX IF NOT EXISTS idx_vendor_statements_status ON vendor_statements(tenant_id, status);

CREATE TABLE IF NOT EXISTS vendor_statement_lines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    statement_id UUID NOT NULL REFERENCES vendor_statements(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    line_date DATE NOT NULL,
    description TEXT NOT NULL,
    reference_number TEXT,
    amount_cents BIGINT NOT NULL,
    line_type TEXT NOT NULL DEFAULT 'invoice',
    match_status TEXT NOT NULL DEFAULT 'unmatched',
    matched_invoice_id UUID REFERENCES invoices(id),
    variance_cents BIGINT DEFAULT 0,
    matched_at TIMESTAMPTZ,
    matched_by TEXT DEFAULT 'manual',
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_statement_lines_statement ON vendor_statement_lines(statement_id);
CREATE INDEX IF NOT EXISTS idx_statement_lines_match ON vendor_statement_lines(match_status);
CREATE INDEX IF NOT EXISTS idx_statement_lines_invoice ON vendor_statement_lines(matched_invoice_id);
CREATE INDEX IF NOT EXISTS idx_statement_lines_tenant ON vendor_statement_lines(tenant_id);
