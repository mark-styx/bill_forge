-- Sprint 13 Feature #1: ML-Based Invoice Categorization
-- Add tables for embedding storage, feedback learning, and ML model tracking

-- Store pre-computed embeddings for vendors
CREATE TABLE IF NOT EXISTS vendor_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL,
    vendor_id UUID NOT NULL,
    embedding_vector FLOAT[] NOT NULL, -- OpenAI embedding vector (1536 dimensions)
    vendor_name TEXT NOT NULL,
    last_invoice_summary TEXT, -- Summary of recent invoices for context
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tenant_id, vendor_id)
);

-- Store pre-computed embeddings for GL codes, departments, cost centers
CREATE TABLE IF NOT EXISTS category_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL,
    category_type TEXT NOT NULL CHECK (category_type IN ('gl_code', 'department', 'cost_center')),
    category_value TEXT NOT NULL,
    description TEXT,
    embedding_vector FLOAT[] NOT NULL,
    usage_count INTEGER DEFAULT 0, -- How often this category is used
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tenant_id, category_type, category_value)
);

-- Track user corrections to ML suggestions for feedback learning
CREATE TABLE IF NOT EXISTS categorization_feedback (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL,
    invoice_id UUID NOT NULL,
    vendor_id UUID,
    vendor_name TEXT NOT NULL,

    -- What the ML suggested
    suggested_gl_code TEXT,
    suggested_department TEXT,
    suggested_cost_center TEXT,
    suggestion_confidence FLOAT,
    suggestion_source TEXT,

    -- What the user actually chose
    accepted_gl_code TEXT,
    accepted_department TEXT,
    accepted_cost_center BOOLEAN DEFAULT FALSE, -- Did user accept the suggestion?

    -- Context for learning
    line_items_summary TEXT, -- Concatenated line item descriptions
    total_amount_cents BIGINT,

    -- Metadata
    feedback_type TEXT CHECK (feedback_type IN ('acceptance', 'correction', 'rejection')),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Track ML model performance metrics
CREATE TABLE IF NOT EXISTS categorization_ml_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL,
    metric_date DATE NOT NULL,

    -- Accuracy metrics
    total_suggestions INTEGER DEFAULT 0,
    accepted_suggestions INTEGER DEFAULT 0,
    corrected_suggestions INTEGER DEFAULT 0,
    rejected_suggestions INTEGER DEFAULT 0,

    -- Performance by source
    vendor_history_accuracy FLOAT,
    embedding_accuracy FLOAT,
    hybrid_accuracy FLOAT,

    -- Confidence calibration
    avg_confidence_accepted FLOAT,
    avg_confidence_rejected FLOAT,

    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tenant_id, metric_date)
);

-- Create indexes for efficient queries
CREATE INDEX idx_vendor_embeddings_tenant ON vendor_embeddings(tenant_id);
CREATE INDEX idx_vendor_embeddings_vector ON vendor_embeddings USING ivfflat (embedding_vector vector_cosine_ops) WITH (lists = 100);
CREATE INDEX idx_category_embeddings_tenant_type ON category_embeddings(tenant_id, category_type);
CREATE INDEX idx_category_embeddings_vector ON category_embeddings USING ivfflat (embedding_vector vector_cosine_ops) WITH (lists = 100);

CREATE INDEX idx_categorization_feedback_tenant ON categorization_feedback(tenant_id, created_at DESC);
CREATE INDEX idx_categorization_feedback_vendor ON categorization_feedback(vendor_id);
CREATE INDEX idx_categorization_feedback_invoice ON categorization_feedback(invoice_id);

CREATE INDEX idx_categorization_ml_metrics_tenant_date ON categorization_ml_metrics(tenant_id, metric_date DESC);

-- Enable pgvector extension for cosine similarity searches
CREATE EXTENSION IF NOT EXISTS vector;
