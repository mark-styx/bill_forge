-- EDI receiver ID to tenant mapping (metadata database)
-- Allows webhook endpoint to look up which tenant owns a given EDI receiver ID
-- without needing to scan all tenant databases.
CREATE TABLE IF NOT EXISTS edi_receiver_map (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    receiver_id VARCHAR(50) NOT NULL,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(receiver_id)
);

CREATE INDEX IF NOT EXISTS idx_edi_receiver_map_receiver ON edi_receiver_map(receiver_id);
