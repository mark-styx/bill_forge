-- ERP outbound export idempotency keys.
--
-- Adds a per-attempt stable token that is sent to QuickBooks (`requestid`
-- query parameter) and Xero (`Idempotency-Key` header) on every retry. The
-- token is persisted on the export row BEFORE the network call so that
-- worker-/route-level retries reuse the same value and the upstream dedups
-- the create instead of producing a second bill/invoice.
--
-- Columns are left nullable in this slice so the schema change is backwards
-- compatible with already-synced historical rows; a follow-up migration can
-- enforce NOT NULL once every new export populates the field.
--
-- Each ALTER is wrapped in a DO block so the migration is idempotent across
-- environments where the per-tenant DB may not yet have the Xero export
-- table provisioned at the time this migration runs.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'quickbooks_invoice_exports'
    ) THEN
        ALTER TABLE quickbooks_invoice_exports
            ADD COLUMN IF NOT EXISTS request_id TEXT;
        ALTER TABLE quickbooks_invoice_exports
            ALTER COLUMN quickbooks_bill_id DROP NOT NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_quickbooks_invoice_exports_request_id
            ON quickbooks_invoice_exports (tenant_id, request_id)
            WHERE request_id IS NOT NULL;
    END IF;
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'xero_invoice_exports'
    ) THEN
        ALTER TABLE xero_invoice_exports
            ADD COLUMN IF NOT EXISTS idempotency_key TEXT;
        ALTER TABLE xero_invoice_exports
            ALTER COLUMN xero_invoice_id DROP NOT NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_xero_invoice_exports_idempotency_key
            ON xero_invoice_exports (tenant_id, idempotency_key)
            WHERE idempotency_key IS NOT NULL;
    END IF;
END
$$;
