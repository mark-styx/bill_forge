-- Persist a-la-carte module entitlements with billing subscriptions.
--
-- Base plan modules remain defined in code; this JSONB column records modules
-- purchased independently so tenant.enabled_modules can be derived from
-- base plan + add-ons instead of hard-coded bundles alone.

ALTER TABLE tenant_subscriptions
    ADD COLUMN IF NOT EXISTS add_on_modules JSONB NOT NULL DEFAULT '[]';
