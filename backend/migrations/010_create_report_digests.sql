-- Report digests configuration
-- Stores user preferences for scheduled report email digests

CREATE TABLE IF NOT EXISTS report_digests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255) NOT NULL,
    user_id UUID NOT NULL,
    digest_type VARCHAR(50) NOT NULL,
    frequency VARCHAR(20) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    filters JSONB DEFAULT '{}',
    last_sent_at TIMESTAMPTZ,
    next_send_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, user_id, digest_type)
);

CREATE INDEX idx_report_digests_tenant ON report_digests(tenant_id);
CREATE INDEX idx_report_digests_user ON report_digests(user_id);
CREATE INDEX idx_report_digests_next_send ON report_digests(next_send_at) WHERE enabled = true;

COMMENT ON TABLE report_digests IS 'Scheduled report digest configurations for users';
COMMENT ON COLUMN report_digests.digest_type IS 'Type of digest: daily_summary, weekly_summary, monthly_summary, approval_reminder';
COMMENT ON COLUMN report_digests.frequency IS 'How often to send: daily, weekly, monthly';
COMMENT ON COLUMN report_digests.filters IS 'JSON filters to customize digest content (e.g., departments, vendors, amount thresholds)';
COMMENT ON COLUMN report_digests.next_send_at IS 'Next scheduled send time (UTC)';
