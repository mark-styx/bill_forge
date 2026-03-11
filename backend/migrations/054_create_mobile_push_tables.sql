-- Sprint 14 Feature #8: Mobile App Backend - Push Notifications Tables
-- Creates device token management and push notification tracking tables

-- Device tokens table for user device registration
CREATE TABLE device_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    platform VARCHAR(20) NOT NULL CHECK (platform IN ('ios', 'android')),
    token VARCHAR(255) NOT NULL,
    device_name VARCHAR(255),
    os_version VARCHAR(50),
    app_version VARCHAR(50),
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, user_id, device_id)
);

-- Push notification logs for delivery tracking
CREATE TABLE push_notification_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    device_token_id UUID NOT NULL REFERENCES device_tokens(id) ON DELETE CASCADE,
    notification_id UUID NOT NULL,
    external_message_id VARCHAR(255),
    platform VARCHAR(20) NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('pending', 'sent', 'delivered', 'failed', 'cancelled')),
    error_message TEXT,
    sent_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Push notification receipts for user interaction tracking
CREATE TABLE push_notification_receipts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    notification_log_id UUID NOT NULL REFERENCES push_notification_logs(id) ON DELETE CASCADE,
    delivered BOOLEAN NOT NULL DEFAULT false,
    opened BOOLEAN NOT NULL DEFAULT false,
    dismissed BOOLEAN NOT NULL DEFAULT false,
    action_taken VARCHAR(50),
    opened_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient querying
CREATE INDEX idx_device_tokens_tenant_user ON device_tokens(tenant_id, user_id);
CREATE INDEX idx_device_tokens_device_id ON device_tokens(device_id);
CREATE INDEX idx_device_tokens_token ON device_tokens(token);
CREATE INDEX idx_push_logs_tenant_user ON push_notification_logs(tenant_id, user_id);
CREATE INDEX idx_push_logs_notification ON push_notification_logs(notification_id);
CREATE INDEX idx_push_logs_status ON push_notification_logs(status);
CREATE INDEX idx_push_receipts_notification_log ON push_notification_receipts(notification_log_id);

-- Update timestamp trigger for device_tokens
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER device_tokens_updated_at
    BEFORE UPDATE ON device_tokens
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

-- Comments for documentation
COMMENT ON TABLE device_tokens IS 'Stores user device tokens for push notifications (FCM/APNS)';
COMMENT ON TABLE push_notification_logs IS 'Tracks push notification delivery status and attempts';
COMMENT ON TABLE push_notification_receipts IS 'Tracks user interactions with push notifications (open, dismiss, action)';
COMMENT ON COLUMN device_tokens.token IS 'FCM token (152 alphanumeric) or APNS token (64 hex chars)';
COMMENT ON COLUMN device_tokens.device_id IS 'Unique device identifier provided by mobile app';
COMMENT ON COLUMN device_tokens.platform IS 'ios or android';
COMMENT ON COLUMN push_notification_logs.status IS 'pending, sent, delivered, failed, cancelled';
COMMENT ON COLUMN push_notification_receipts.action_taken IS 'User action: approve, reject, view, etc.';
