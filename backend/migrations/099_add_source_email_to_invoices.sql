-- Add source_email_id column to invoices (TENANT DATABASE).
-- Traces OCR jobs back to their source inbound email.
-- No FK constraint because inbound_email_messages lives in the metadata DB.

ALTER TABLE invoices
    ADD COLUMN IF NOT EXISTS source_email_id UUID;
