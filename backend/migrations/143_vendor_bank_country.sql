-- Migration 143: Persist vendor bank_country for fraud-guard country_mismatch (refs #394)
--
-- verify_banking was passing None for bank_country because the column was never
-- stored on the vendor row. That made check_country_mismatch return Unknown on the
-- dual-approval flow, structurally disabling the country_mismatch signal - the
-- most predictive BEC/banking-fraud indicator - on the exact code path designed
-- to catch banking fraud.
--
-- This migration adds a nullable bank_country column so update_banking can persist
-- the ISO-3166 alpha-2 code captured at change time, and verify_banking can read
-- it back when running fraud guard. Pre-existing rows stay NULL, which preserves
-- today's Unknown behavior on legacy bank accounts.

ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_country TEXT;

COMMENT ON COLUMN vendors.bank_country IS
    'ISO-3166 alpha-2 country code for the bank account. Used by fraud_guard country_mismatch on the verify_banking dual-approval flow.';
