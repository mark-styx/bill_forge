-- Integration webhook nonce table for replay protection
-- Used by QuickBooks, Xero, Salesforce, Bill.com, Workday, and Sage Intacct
CREATE TABLE IF NOT EXISTS integration_webhook_nonces (
    provider VARCHAR(32) NOT NULL,
    tenant_id UUID NOT NULL,
    nonce VARCHAR(256) NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (provider, tenant_id, nonce)
);
CREATE INDEX IF NOT EXISTS idx_iwn_received_at ON integration_webhook_nonces(received_at);

-- Add webhook_secret to each integration connection table (safe if table doesn't exist yet)
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'quickbooks_connections') THEN
        ALTER TABLE quickbooks_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'xero_connections') THEN
        ALTER TABLE xero_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'salesforce_connections') THEN
        ALTER TABLE salesforce_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'bill_com_connections') THEN
        ALTER TABLE bill_com_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'workday_connections') THEN
        ALTER TABLE workday_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'sage_intacct_connections') THEN
        ALTER TABLE sage_intacct_connections ADD COLUMN IF NOT EXISTS webhook_secret TEXT;
    END IF;
END $$;
