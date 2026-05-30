-- Migration: Add chat approval threads table and audit-log source columns
-- Feature: Sprint #270 - Native Slack/Teams Approval Surface

-- Tracks Slack/Teams messages that carry interactive approval actions so that
-- inbound thread replies (Slack Events) and action callbacks can be routed back
-- to the correct invoice and approver.
CREATE TABLE chat_approval_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('slack', 'teams')),
    channel_id TEXT NOT NULL,
    message_ts TEXT NOT NULL,
    approver_user_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (approver_user_id) REFERENCES users(id) ON DELETE CASCADE,

    CONSTRAINT unique_provider_message UNIQUE (provider, channel_id, message_ts)
);

-- Add source-tracking columns to invoice_audit_log if they do not exist yet.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'invoice_audit_log' AND column_name = 'source_channel'
    ) THEN
        ALTER TABLE invoice_audit_log ADD COLUMN source_channel TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'invoice_audit_log' AND column_name = 'source_message_ts'
    ) THEN
        ALTER TABLE invoice_audit_log ADD COLUMN source_message_ts TEXT;
    END IF;
END
$$;

-- Indexes
CREATE INDEX idx_chat_approval_threads_invoice ON chat_approval_threads(invoice_id);
CREATE INDEX idx_chat_approval_threads_tenant ON chat_approval_threads(tenant_id);
CREATE INDEX idx_chat_approval_threads_approver ON chat_approval_threads(approver_user_id);
CREATE INDEX idx_chat_approval_threads_lookup
    ON chat_approval_threads(provider, channel_id, message_ts);

COMMENT ON TABLE chat_approval_threads IS 'Maps Slack/Teams messages to invoices for approval-thread routing';
