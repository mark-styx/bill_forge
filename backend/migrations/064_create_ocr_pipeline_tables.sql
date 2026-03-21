-- Migration 064: OCR Pipeline — job tracking, extraction results, vendor matching
-- Provides async job processing, multi-provider OCR, and intelligent field extraction

-- OCR processing jobs (async pipeline)
CREATE TABLE IF NOT EXISTS ocr_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    document_id UUID NOT NULL,
    invoice_id UUID REFERENCES invoices(id),

    -- Job configuration
    provider TEXT NOT NULL DEFAULT 'auto',       -- tesseract, aws_textract, google_vision, auto
    priority INTEGER NOT NULL DEFAULT 0,         -- higher = processed first
    
    -- Status tracking
    status TEXT NOT NULL DEFAULT 'queued',        -- queued, processing, completed, failed, cancelled
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    
    -- Results
    raw_extraction JSONB,                         -- raw OCR output from provider
    structured_result JSONB,                      -- normalized extraction result
    confidence_score REAL,                        -- overall confidence 0.0-1.0
    field_confidences JSONB DEFAULT '{}',         -- per-field confidence scores
    
    -- Vendor matching
    matched_vendor_id UUID REFERENCES vendors(id),
    vendor_match_confidence REAL,
    vendor_match_method TEXT,                     -- exact, fuzzy, learned
    
    -- Line item extraction
    extracted_line_items JSONB DEFAULT '[]',
    line_item_count INTEGER DEFAULT 0,
    
    -- Processing metadata
    processing_started_at TIMESTAMPTZ,
    processing_completed_at TIMESTAMPTZ,
    processing_duration_ms INTEGER,
    error_message TEXT,
    provider_response_id TEXT,                    -- external provider job/request ID
    
    -- Audit
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ocr_jobs_tenant ON ocr_jobs(tenant_id);
CREATE INDEX idx_ocr_jobs_status ON ocr_jobs(tenant_id, status);
CREATE INDEX idx_ocr_jobs_document ON ocr_jobs(document_id);
CREATE INDEX idx_ocr_jobs_invoice ON ocr_jobs(invoice_id);
CREATE INDEX idx_ocr_jobs_queued ON ocr_jobs(priority DESC, created_at ASC) WHERE status = 'queued';

-- OCR field corrections (learning from human review)
CREATE TABLE IF NOT EXISTS ocr_field_corrections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    ocr_job_id UUID NOT NULL REFERENCES ocr_jobs(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    
    field_name TEXT NOT NULL,                     -- vendor_name, invoice_number, total_amount, etc.
    original_value TEXT,                          -- what OCR extracted
    corrected_value TEXT NOT NULL,                -- what the human corrected to
    confidence_before REAL,
    
    -- For training/learning
    correction_type TEXT NOT NULL DEFAULT 'manual', -- manual, auto_matched, rule_based
    corrected_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ocr_corrections_tenant ON ocr_field_corrections(tenant_id);
CREATE INDEX idx_ocr_corrections_job ON ocr_field_corrections(ocr_job_id);
CREATE INDEX idx_ocr_corrections_field ON ocr_field_corrections(tenant_id, field_name);

-- Vendor aliases (learned from corrections, used for fuzzy matching)
CREATE TABLE IF NOT EXISTS vendor_aliases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,                          -- alternate name/spelling
    source TEXT NOT NULL DEFAULT 'manual',        -- manual, ocr_learned, import
    match_count INTEGER NOT NULL DEFAULT 1,       -- how many times this alias matched
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(tenant_id, vendor_id, alias)
);

CREATE INDEX idx_vendor_aliases_tenant ON vendor_aliases(tenant_id);
CREATE INDEX idx_vendor_aliases_lookup ON vendor_aliases(tenant_id, alias);

-- OCR processing stats (per-tenant analytics)
CREATE TABLE IF NOT EXISTS ocr_processing_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    
    -- Volume metrics
    total_jobs INTEGER NOT NULL DEFAULT 0,
    successful_jobs INTEGER NOT NULL DEFAULT 0,
    failed_jobs INTEGER NOT NULL DEFAULT 0,
    
    -- Quality metrics
    avg_confidence REAL,
    avg_processing_time_ms INTEGER,
    auto_match_rate REAL,                        -- % of invoices auto-matched to vendors
    touchless_rate REAL,                         -- % requiring no human correction
    
    -- Field-level accuracy
    field_accuracy JSONB DEFAULT '{}',           -- per-field accuracy rates
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(tenant_id, period_start)
);

CREATE INDEX idx_ocr_stats_tenant ON ocr_processing_stats(tenant_id, period_start);
