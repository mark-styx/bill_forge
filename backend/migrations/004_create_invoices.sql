-- Create invoices table (in tenant databases)
CREATE TABLE IF NOT EXISTS invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,

    -- Vendor information
    vendor_id UUID REFERENCES vendors(id),
    vendor_name TEXT NOT NULL,

    -- Invoice details
    invoice_number TEXT NOT NULL,
    invoice_date DATE,
    due_date DATE,
    po_number TEXT,

    -- Financial
    subtotal_cents BIGINT,
    tax_amount_cents BIGINT,
    total_amount_cents BIGINT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',

    -- Line items
    line_items JSONB NOT NULL DEFAULT '[]',

    -- Status tracking
    capture_status TEXT NOT NULL DEFAULT 'pending',
    processing_status TEXT NOT NULL DEFAULT 'draft',

    -- Queue tracking
    current_queue_id UUID,
    assigned_to UUID REFERENCES users(id),

    -- Document reference
    document_id UUID NOT NULL,
    supporting_documents JSONB NOT NULL DEFAULT '[]',
    ocr_confidence REAL,

    -- Department/GL coding
    department TEXT,
    gl_code TEXT,
    cost_center TEXT,

    -- Metadata
    notes TEXT,
    tags JSONB NOT NULL DEFAULT '[]',
    custom_fields JSONB NOT NULL DEFAULT '{}',

    -- Audit
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, invoice_number)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_invoices_tenant_id ON invoices(tenant_id);
CREATE INDEX IF NOT EXISTS idx_invoices_vendor_id ON invoices(vendor_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(capture_status, processing_status);
CREATE INDEX IF NOT EXISTS idx_invoices_date ON invoices(invoice_date);
CREATE INDEX IF NOT EXISTS idx_invoices_created_at ON invoices(created_at);
CREATE INDEX IF NOT EXISTS idx_invoices_queue ON invoices(current_queue_id);
CREATE INDEX IF NOT EXISTS idx_invoices_assigned ON invoices(assigned_to);
