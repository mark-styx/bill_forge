-- Add vendor columns used by QuickBooks vendor sync and the vendor management UI.
-- These columns were referenced by the API handler (quickbooks.rs:487-589) but
-- were never added to the schema via migration.

ALTER TABLE vendors ADD COLUMN IF NOT EXISTS vendor_type TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS phone TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
