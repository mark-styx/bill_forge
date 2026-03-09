-- Analytics Tables Migration
-- Creates tables for tracking user behavior, feature usage, and performance metrics

-- Analytics events table
CREATE TABLE analytics_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    event_category TEXT NOT NULL,
    event_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common query patterns
CREATE INDEX idx_analytics_events_tenant_created ON analytics_events(tenant_id, created_at DESC);
CREATE INDEX idx_analytics_events_user ON analytics_events(user_id);
CREATE INDEX idx_analytics_events_type ON analytics_events(event_type);
CREATE INDEX idx_analytics_events_category ON analytics_events(event_category);

-- Daily analytics aggregation table (for faster queries)
CREATE TABLE analytics_daily_summaries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    summary_date DATE NOT NULL,
    total_events BIGINT NOT NULL DEFAULT 0,
    unique_users BIGINT NOT NULL DEFAULT 0,
    top_features JSONB NOT NULL DEFAULT '[]'::jsonb,
    performance_metrics JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, summary_date)
);

CREATE INDEX idx_analytics_daily_summaries_tenant_date ON analytics_daily_summaries(tenant_id, summary_date DESC);

-- Comments
COMMENT ON TABLE analytics_events IS 'Raw analytics events tracking user behavior and system performance';
COMMENT ON TABLE analytics_daily_summaries IS 'Pre-aggregated daily analytics summaries for faster queries';
COMMENT ON COLUMN analytics_events.event_type IS 'Type of event (e.g., invoice_uploaded, invoice_approved)';
COMMENT ON COLUMN analytics_events.event_category IS 'Category of event (e.g., user_action, api_request, system_event)';
COMMENT ON COLUMN analytics_events.event_data IS 'JSON payload with event-specific data';
