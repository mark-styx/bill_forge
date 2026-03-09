-- Email notifications queue for batch sending
-- Stores pending emails to be sent by the background worker

CREATE TABLE IF NOT EXISTS email_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255) NOT NULL,
    recipient_email VARCHAR(255) NOT NULL,
    recipient_name VARCHAR(255),
    subject VARCHAR(500) NOT NULL,
    html_body TEXT NOT NULL,
    text_body TEXT NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 0,
    batch_id UUID,
    metadata JSONB DEFAULT '{}',
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_email_notifications_tenant ON email_notifications(tenant_id);
CREATE INDEX idx_email_notifications_status ON email_notifications(status, created_at);
CREATE INDEX idx_email_notifications_batch ON email_notifications(batch_id);
CREATE INDEX idx_email_notifications_pending ON email_notifications(tenant_id, status, priority, created_at) WHERE status = 'pending';

-- Add comment
COMMENT ON TABLE email_notifications IS 'Queue of email notifications to be sent by background worker';
COMMENT ON COLUMN email_notifications.status IS 'pending, sent, failed, expired';
COMMENT ON COLUMN email_notifications.batch_id IS 'Optional grouping for batch operations';
COMMENT ON COLUMN email_notifications.metadata IS 'Additional context (invoice_id, approval_id, etc.)';
