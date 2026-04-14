-- QBO sync support: add external_id column to vendors and sync status tracking.
-- This migration supports the lightweight QBO integration module (routes/qbo.rs)
-- which operates independently from the full billforge-quickbooks integration.

-- Add external_id column for matching vendors from external systems (e.g. QBO).
-- Format: "qbo:{Id}" for QuickBooks Online vendors.
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS external_id TEXT;

-- Unique index so only one vendor per tenant can map to a given external ID.
CREATE UNIQUE INDEX IF NOT EXISTS idx_vendors_tenant_external_id
    ON vendors(tenant_id, external_id)
    WHERE external_id IS NOT NULL;

-- Add last_sync_status / last_sync_error columns to quickbooks_connections
-- so the lightweight qbo module can report sync health without requiring
-- the billforge-quickbooks crate's sync_log table.
ALTER TABLE quickbooks_connections ADD COLUMN IF NOT EXISTS last_sync_status TEXT;
ALTER TABLE quickbooks_connections ADD COLUMN IF NOT EXISTS last_sync_error TEXT;

-- TODO: Token encryption at rest. Currently plaintext TEXT columns.
-- Follow-up: integrate pgcrypto or a KMS--backed encryption helper.
