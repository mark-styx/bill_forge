-- Migration: Add notification system tables
-- Feature: Sprint 13 #5 - Slack/Teams Notification Integration

-- User notification preferences
CREATE TABLE user_notification_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    channel VARCHAR(50) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    notification_types TEXT[] DEFAULT '{}',
    priority_filter VARCHAR(20),
    quiet_hours_start TIME,
    quiet_hours_end TIME,
    quiet_hours_timezone VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,

    CONSTRAINT unique_user_channel UNIQUE (user_id, channel)
);

-- Slack connections (OAuth tokens per tenant)
CREATE TABLE slack_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    slack_team_id VARCHAR(50) NOT NULL,
    slack_team_name VARCHAR(255),
    slack_user_id VARCHAR(50) NOT NULL,
    bot_user_id VARCHAR(50) NOT NULL,
    access_token TEXT NOT NULL,
    bot_access_token TEXT NOT NULL,
    scope VARCHAR(500),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,

    CONSTRAINT unique_slack_team UNIQUE (tenant_id, slack_team_id)
);

-- Slack OAuth states (for OAuth flow)
CREATE TABLE slack_oauth_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    state_nonce VARCHAR(100) NOT NULL UNIQUE,
    redirect_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '10 minutes',
    used_at TIMESTAMPTZ,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Teams webhook configurations
CREATE TABLE teams_webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    webhook_url TEXT NOT NULL,
    channel_name VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Notification templates (customizable message formats)
CREATE TABLE notification_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    notification_type VARCHAR(50) NOT NULL,
    channel VARCHAR(50) NOT NULL,
    template_name VARCHAR(100) NOT NULL,
    title_template TEXT NOT NULL,
    body_template TEXT NOT NULL,
    include_actions BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_default BOOLEAN NOT NULL DEFAULT false,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,

    CONSTRAINT unique_template UNIQUE (tenant_id, notification_type, channel)
);

-- Notification delivery log
CREATE TABLE notification_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    notification_id UUID NOT NULL,
    channel VARCHAR(50) NOT NULL,
    external_id VARCHAR(255),
    success BOOLEAN NOT NULL,
    error_message TEXT,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    clicked_at TIMESTAMPTZ,
    action_taken VARCHAR(50),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,

    CONSTRAINT fk_notification FOREIGN KEY (notification_id) REFERENCES approval_requests(id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX idx_user_notification_prefs_user ON user_notification_preferences(user_id);
CREATE INDEX idx_user_notification_prefs_tenant ON user_notification_preferences(tenant_id);
CREATE INDEX idx_slack_connections_tenant ON slack_connections(tenant_id);
CREATE INDEX idx_slack_connections_user ON slack_connections(user_id);
CREATE INDEX idx_slack_oauth_states_nonce ON slack_oauth_states(state_nonce);
CREATE INDEX idx_slack_oauth_states_expires ON slack_oauth_states(expires_at);
CREATE INDEX idx_teams_webhooks_tenant ON teams_webhooks(tenant_id);
CREATE INDEX idx_teams_webhooks_user ON teams_webhooks(user_id);
CREATE INDEX idx_notification_templates_tenant ON notification_templates(tenant_id);
CREATE INDEX idx_notification_deliveries_user ON notification_deliveries(user_id);
CREATE INDEX idx_notification_deliveries_tenant ON notification_deliveries(tenant_id);
CREATE INDEX idx_notification_deliveries_sent_at ON notification_deliveries(delivered_at);

-- Insert default templates (only if system tenant exists, e.g. production seed)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM tenants WHERE id = '00000000-0000-0000-0000-000000000000') THEN
        INSERT INTO notification_templates (tenant_id, notification_type, channel, template_name, title_template, body_template, is_default) VALUES
        ('00000000-0000-0000-0000-000000000000', 'approval_request', 'slack', 'Default Approval Request', 'Invoice Approval Required', 'Invoice {invoice_number} from {vendor_name} for {amount} requires your approval.', true),
        ('00000000-0000-0000-0000-000000000000', 'approval_request', 'teams', 'Default Approval Request', 'Invoice Approval Required', 'Invoice {invoice_number} from {vendor_name} for {amount} requires your approval.', true),
        ('00000000-0000-0000-0000-000000000000', 'approval_reminder', 'slack', 'Default Reminder', 'Reminder: Invoice Approval Pending', 'Invoice {invoice_number} has been pending for {days_pending} days.', true),
        ('00000000-0000-0000-0000-000000000000', 'approval_completed', 'slack', 'Default Completed', 'Invoice Approved', 'Invoice {invoice_number} has been {action} by {approver_name}.', true),
        ('00000000-0000-0000-0000-000000000000', 'payment_due', 'slack', 'Default Payment Due', 'Payment Due Soon', 'Payment of {amount} to {vendor_name} is due on {due_date}.', true);
    END IF;
END
$$;

-- Trigger to update updated_at timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_user_notification_prefs_updated_at BEFORE UPDATE ON user_notification_preferences
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_slack_connections_updated_at BEFORE UPDATE ON slack_connections
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_teams_webhooks_updated_at BEFORE UPDATE ON teams_webhooks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_notification_templates_updated_at BEFORE UPDATE ON notification_templates
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Comment
COMMENT ON TABLE user_notification_preferences IS 'User preferences for notification channels and types';
COMMENT ON TABLE slack_connections IS 'Slack workspace connections and OAuth tokens per tenant';
COMMENT ON TABLE slack_oauth_states IS 'Temporary OAuth state tokens for Slack installation flow';
COMMENT ON TABLE teams_webhooks IS 'Microsoft Teams webhook configurations per user';
COMMENT ON TABLE notification_templates IS 'Customizable notification message templates';
COMMENT ON TABLE notification_deliveries IS 'Log of all notification deliveries and their status';
