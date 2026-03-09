-- Feedback Table Migration
-- Creates table for customer feedback collection

-- Feedback table
CREATE TABLE feedback (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    comment TEXT,
    sentiment TEXT CHECK (sentiment IN ('positive', 'neutral', 'negative')),
    sentiment_score FLOAT CHECK (sentiment_score >= 0 AND sentiment_score <= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common query patterns
CREATE INDEX idx_feedback_tenant_created ON feedback(tenant_id, created_at DESC);
CREATE INDEX idx_feedback_user ON feedback(user_id);
CREATE INDEX idx_feedback_category ON feedback(category);
CREATE INDEX idx_feedback_rating ON feedback(rating);
CREATE INDEX idx_feedback_sentiment ON feedback(sentiment);

-- Comments
COMMENT ON TABLE feedback IS 'Customer feedback entries with optional sentiment analysis';
COMMENT ON COLUMN feedback.category IS 'Feedback category (e.g., general, invoice_processing, ui_ux)';
COMMENT ON COLUMN feedback.rating IS 'Customer rating from 1 (worst) to 5 (best)';
COMMENT ON COLUMN feedback.sentiment IS 'Sentiment analysis result: positive, neutral, or negative';
COMMENT ON COLUMN feedback.sentiment_score IS 'Sentiment score from 0.0 (negative) to 1.0 (positive)';
