-- Purchase orders (tenant database)
CREATE TABLE IF NOT EXISTS purchase_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    po_number VARCHAR(100) NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id),
    vendor_name VARCHAR(255) NOT NULL,
    order_date DATE NOT NULL,
    expected_delivery DATE,
    status VARCHAR(30) NOT NULL DEFAULT 'open',
    total_amount_cents BIGINT NOT NULL,
    total_currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    ship_to_address TEXT,
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, po_number)
);

CREATE INDEX IF NOT EXISTS idx_purchase_orders_tenant ON purchase_orders(tenant_id);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_vendor ON purchase_orders(vendor_id);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_status ON purchase_orders(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_po_number ON purchase_orders(tenant_id, po_number);

-- Purchase order line items
CREATE TABLE IF NOT EXISTS po_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    po_id UUID NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
    line_number INTEGER NOT NULL,
    description TEXT NOT NULL,
    quantity REAL NOT NULL,
    unit_of_measure VARCHAR(10) NOT NULL DEFAULT 'EA',
    unit_price_cents BIGINT NOT NULL,
    unit_price_currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    total_cents BIGINT NOT NULL,
    total_currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    product_id VARCHAR(100),
    received_quantity REAL NOT NULL DEFAULT 0,
    invoiced_quantity REAL NOT NULL DEFAULT 0,
    UNIQUE(po_id, line_number)
);

CREATE INDEX IF NOT EXISTS idx_po_line_items_po ON po_line_items(po_id);

-- Receiving records (from ASN 856 or manual receipt)
CREATE TABLE IF NOT EXISTS receiving_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    po_id UUID NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
    received_date DATE NOT NULL,
    edi_document_id UUID REFERENCES edi_documents(id),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_receiving_records_po ON receiving_records(po_id);

-- Receiving record line items
CREATE TABLE IF NOT EXISTS receiving_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    receiving_id UUID NOT NULL REFERENCES receiving_records(id) ON DELETE CASCADE,
    po_line_number INTEGER NOT NULL,
    quantity_received REAL NOT NULL,
    quantity_damaged REAL NOT NULL DEFAULT 0,
    product_id VARCHAR(100)
);

CREATE INDEX IF NOT EXISTS idx_receiving_line_items_rec ON receiving_line_items(receiving_id);

-- 3-way match results (invoice <-> PO <-> receiving)
CREATE TABLE IF NOT EXISTS match_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    po_id UUID REFERENCES purchase_orders(id),
    receiving_id UUID REFERENCES receiving_records(id),
    match_type VARCHAR(20) NOT NULL,
    price_variance_pct REAL,
    quantity_variance_pct REAL,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_match_results_invoice ON match_results(invoice_id);
CREATE INDEX IF NOT EXISTS idx_match_results_po ON match_results(po_id);
CREATE INDEX IF NOT EXISTS idx_match_results_tenant ON match_results(tenant_id);

-- Add po_id reference to edi_documents for linking 850/856 docs
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS po_id UUID REFERENCES purchase_orders(id);
