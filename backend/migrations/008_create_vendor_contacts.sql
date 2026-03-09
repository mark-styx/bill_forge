-- Create vendor_contacts table
CREATE TABLE IF NOT EXISTS vendor_contacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    title TEXT,
    email TEXT,
    phone TEXT,
    is_primary BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_vendor_contacts_vendor_id ON vendor_contacts(vendor_id);
CREATE INDEX idx_vendor_contacts_primary ON vendor_contacts(vendor_id, is_primary) WHERE is_primary = true;
CREATE INDEX idx_vendor_contacts_email ON vendor_contacts(email);

-- Trigger to auto-set is_primary to false when adding a new primary contact
CREATE OR REPLACE FUNCTION ensure_single_primary_contact()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.is_primary = true THEN
        UPDATE vendor_contacts
        SET is_primary = false, updated_at = NOW()
        WHERE vendor_id = NEW.vendor_id AND id != NEW.id AND is_primary = true;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER vendor_contacts_primary_trigger
BEFORE INSERT OR UPDATE ON vendor_contacts
FOR EACH ROW
EXECUTE FUNCTION ensure_single_primary_contact();
