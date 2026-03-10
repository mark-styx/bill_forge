-- Add categorization_confidence column to invoices table
-- Sprint 13 Feature #1: ML-Based Invoice Categorization

-- Add column for ML categorization confidence (0.0 to 1.0)
ALTER TABLE invoices
ADD COLUMN IF NOT EXISTS categorization_confidence REAL;

-- Add comment to document the field
COMMENT ON COLUMN invoices.categorization_confidence IS 'ML categorization confidence score (0.0-1.0). Invoices with confidence >= 0.95 may be auto-approved if all required fields are present.';

-- Create index for querying high-confidence invoices
CREATE INDEX IF NOT EXISTS idx_invoices_categorization_confidence
ON invoices (categorization_confidence)
WHERE categorization_confidence >= 0.95;
