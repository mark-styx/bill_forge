-- Track category suggestions for analytics
CREATE TABLE IF NOT EXISTS category_suggestions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id),

    -- Suggestions
    suggested_gl_code TEXT,
    gl_code_confidence REAL,
    gl_code_source TEXT,

    suggested_department TEXT,
    department_confidence REAL,
    department_source TEXT,

    suggested_cost_center TEXT,
    cost_center_confidence REAL,
    cost_center_source TEXT,

    overall_confidence REAL NOT NULL,

    -- Actual values (if user accepted/rejected)
    accepted_gl_code BOOLEAN,
    accepted_department BOOLEAN,
    accepted_cost_center BOOLEAN,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_category_suggestions_tenant ON category_suggestions(tenant_id);
CREATE INDEX idx_category_suggestions_invoice ON category_suggestions(invoice_id);
CREATE INDEX idx_category_suggestions_created ON category_suggestions(created_at);

-- Track category accuracy over time
CREATE TABLE IF NOT EXISTS category_accuracy_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,

    -- Period
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,

    -- Metrics
    total_suggestions INTEGER NOT NULL DEFAULT 0,
    accepted_gl_codes INTEGER NOT NULL DEFAULT 0,
    accepted_departments INTEGER NOT NULL DEFAULT 0,
    accepted_cost_centers INTEGER NOT NULL DEFAULT 0,

    avg_confidence REAL NOT NULL DEFAULT 0.0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, period_start)
);

CREATE INDEX idx_category_accuracy_tenant ON category_accuracy_metrics(tenant_id);
CREATE INDEX idx_category_accuracy_period ON category_accuracy_metrics(period_start);
