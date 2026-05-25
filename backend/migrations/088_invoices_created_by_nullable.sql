-- Make invoices.created_by nullable so vendor-portal submissions (which have
-- no internal user) can insert with NULL instead of a nil UUID that violates
-- the foreign-key constraint to users(id).

ALTER TABLE invoices
  ALTER COLUMN created_by DROP NOT NULL;
