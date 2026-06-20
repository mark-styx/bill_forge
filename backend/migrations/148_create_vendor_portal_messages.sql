-- Issue #418: Vendor self-service portal — in-thread AP <-> vendor messaging.
--
-- Stores per-invoice message threads between the authenticated AP user and the
-- vendor authenticated via the vendor-portal JWT. Both sides post into the same
-- table so the conversation reads as a single ordered thread.

CREATE TABLE IF NOT EXISTS vendor_portal_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    sender_kind TEXT NOT NULL CHECK (sender_kind IN ('vendor', 'ap_user')),
    sender_user_id UUID NULL,
    sender_vendor_contact_id UUID NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_vendor_portal_messages_tenant_invoice_created
    ON vendor_portal_messages(tenant_id, invoice_id, created_at);

CREATE INDEX IF NOT EXISTS idx_vendor_portal_messages_unread
    ON vendor_portal_messages(tenant_id, invoice_id)
    WHERE read_at IS NULL;

-- RLS hardening mirrors migration 132's pattern.
ALTER TABLE vendor_portal_messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_vendor_portal_messages ON vendor_portal_messages;
CREATE POLICY rls_tenant_vendor_portal_messages ON vendor_portal_messages
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

COMMENT ON TABLE vendor_portal_messages IS 'In-thread messages between AP users and vendors, scoped per invoice; vendor side authenticates via vendor-portal JWT (#418)';
