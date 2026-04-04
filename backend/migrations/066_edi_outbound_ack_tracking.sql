-- EDI outbound document tracking and 997 acknowledgment state machine
--
-- Adds columns to edi_documents for tracking outbound documents through
-- the ack lifecycle: Sent -> AckPending -> Accepted/Rejected
-- Also adds retry and timeout configuration.

-- Outbound document tracking on edi_documents
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS middleware_id VARCHAR(100);
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS ack_retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS max_ack_retries INTEGER NOT NULL DEFAULT 3;
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS ack_timeout_at TIMESTAMPTZ;
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS last_ack_check_at TIMESTAMPTZ;
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS related_document_id UUID REFERENCES edi_documents(id);
ALTER TABLE edi_documents ADD COLUMN IF NOT EXISTS group_control VARCHAR(50);

-- Ack timeout configuration on edi_connections
ALTER TABLE edi_connections ADD COLUMN IF NOT EXISTS ack_timeout_hours INTEGER NOT NULL DEFAULT 24;
ALTER TABLE edi_connections ADD COLUMN IF NOT EXISTS auto_retry_on_reject BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE edi_connections ADD COLUMN IF NOT EXISTS auto_send_997 BOOLEAN NOT NULL DEFAULT true;

-- Index for finding outbound docs awaiting acks
CREATE INDEX IF NOT EXISTS idx_edi_documents_ack_pending
    ON edi_documents(tenant_id, ack_status)
    WHERE direction = 'outbound' AND ack_status = 'pending';

-- Index for finding docs by middleware_id
CREATE INDEX IF NOT EXISTS idx_edi_documents_middleware_id
    ON edi_documents(middleware_id)
    WHERE middleware_id IS NOT NULL;

-- Index for finding docs by group_control (for 997 matching)
CREATE INDEX IF NOT EXISTS idx_edi_documents_group_control
    ON edi_documents(tenant_id, group_control)
    WHERE group_control IS NOT NULL;
