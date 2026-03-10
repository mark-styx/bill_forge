-- Create health scores table (in control plane database)
CREATE TABLE IF NOT EXISTS health_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    score INTEGER NOT NULL CHECK (score >= 0 AND score <= 100),
    classification TEXT NOT NULL CHECK (classification IN ('at_risk', 'needs_attention', 'healthy')),
    usage_score DOUBLE PRECISION NOT NULL,
    feature_adoption_score DOUBLE PRECISION NOT NULL,
    error_rate_score DOUBLE PRECISION NOT NULL,
    sentiment_score DOUBLE PRECISION NOT NULL,
    payment_score DOUBLE PRECISION NOT NULL,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, calculated_at)
);

-- Indexes
CREATE INDEX idx_health_scores_tenant ON health_scores(tenant_id);
CREATE INDEX idx_health_scores_classification ON health_scores(classification);
CREATE INDEX idx_health_scores_calculated ON health_scores(calculated_at DESC);

-- Add comments
COMMENT ON TABLE health_scores IS 'Customer health scores calculated daily';
COMMENT ON COLUMN health_scores.score IS 'Overall health score (0-100)';
COMMENT ON COLUMN health_scores.classification IS 'Risk classification based on score';
COMMENT ON COLUMN health_scores.usage_score IS 'Usage frequency component (30% weight)';
COMMENT ON COLUMN health_scores.feature_adoption_score IS 'Feature adoption component (25% weight)';
COMMENT ON COLUMN health_scores.error_rate_score IS 'Error rate component (20% weight)';
COMMENT ON COLUMN health_scores.sentiment_score IS 'Sentiment from feedback (15% weight)';
COMMENT ON COLUMN health_scores.payment_score IS 'Payment/subscription status (10% weight)';
