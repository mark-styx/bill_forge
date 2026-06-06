-- Add is_sandbox flag to tenants table for self-serve sandbox provisioning
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS is_sandbox BOOLEAN NOT NULL DEFAULT FALSE;
