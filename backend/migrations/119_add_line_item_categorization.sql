-- Per-line-item GL categorization suggestions (issue #315)
--
-- Stores per-line GL/department/cost-center proposals including automatic
-- split suggestions when a single line maps to multiple GL accounts.

CREATE TABLE IF NOT EXISTS invoice_line_categorizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    line_item_id TEXT NOT NULL,
    line_index INTEGER NOT NULL,
    suggested_gl_code TEXT NOT NULL,
    suggested_department TEXT,
    suggested_cost_center TEXT,
    confidence DOUBLE PRECISION NOT NULL,
    rationale TEXT,
    source TEXT NOT NULL,
    splits JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoice_line_cat_tenant_invoice
    ON invoice_line_categorizations(tenant_id, invoice_id);
