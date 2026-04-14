-- Invoice Capture module: persistent storage for OCR captures and extracted line items.
-- Each row is scoped to a tenant; all repo-layer queries MUST filter by tenant_id.

CREATE TABLE IF NOT EXISTS invoice_captures (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    original_filename TEXT,
    mime_type       TEXT,
    provider        TEXT NOT NULL DEFAULT 'tesseract',
    overall_confidence REAL,
    status          TEXT NOT NULL DEFAULT 'processing',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    uploaded_by     UUID
);

CREATE TABLE IF NOT EXISTS invoice_line_items (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    capture_id      UUID NOT NULL REFERENCES invoice_captures(id) ON DELETE CASCADE,
    tenant_id       UUID NOT NULL,
    line_no         INT NOT NULL,
    description     TEXT,
    quantity        NUMERIC,
    unit_price      NUMERIC,
    total           NUMERIC,
    confidence      REAL,
    raw_text        TEXT
);

CREATE INDEX IF NOT EXISTS idx_invoice_captures_tenant_created
    ON invoice_captures (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_invoice_line_items_capture
    ON invoice_line_items (capture_id);
