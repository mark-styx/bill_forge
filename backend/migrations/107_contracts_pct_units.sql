-- Migration: Align contract percentage columns to use percentage units (e.g. 3.0 = 3%)
-- Follow-up to 106_contracts.sql which incorrectly used fractional defaults (0.0200 = 2%).
-- The matcher code always treated these as percentage values, so existing rows that
-- relied on the old fractional defaults (escalator_pct=0, tolerance_pct=0.0200) would
-- have been interpreted as 0% escalation (fine) and 0.02% tolerance (broken).
-- This migration rescales any rows still using the old fractional convention.

-- Fix tolerance_pct: multiply by 100 only if the value looks fractional (< 1).
-- A legitimate sub-1% tolerance (e.g. 0.5%) is possible, but the old default was 0.0200
-- which is clearly fractional. We target values <= 0.1 as the old fractional range.
UPDATE contracts
SET tolerance_pct = tolerance_pct * 100.0
WHERE tolerance_pct > 0 AND tolerance_pct <= 0.1;

-- Fix escalator_pct: same logic. Old default was 0, so most rows are unaffected,
-- but any hand-inserted fractional values (e.g. 0.0300 for 3%) need rescaling.
UPDATE contracts
SET escalator_pct = escalator_pct * 100.0
WHERE escalator_pct > 0 AND escalator_pct <= 0.1;

-- Rescale stored variance_pct in contract_matches that were computed against
-- the (incorrectly) interpreted tolerance. These were computed as
-- ((amount - expected) / expected) * 100 already, so they are in percentage units
-- regardless of the tolerance interpretation. No change needed for variance_pct.

-- Widen columns to NUMERIC(8,4) to comfortably hold values like 100.0000.
ALTER TABLE contracts ALTER COLUMN escalator_pct TYPE NUMERIC(8,4);
ALTER TABLE contracts ALTER COLUMN tolerance_pct TYPE NUMERIC(8,4);
