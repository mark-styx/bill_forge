-- Migration 111: Add last_auto_adjusted_at to routing_configuration
-- Supports routing config auto-adjustment cooldown tracking (issue #284)

ALTER TABLE routing_configuration
    ADD COLUMN IF NOT EXISTS last_auto_adjusted_at TIMESTAMPTZ NULL;
