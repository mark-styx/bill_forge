-- Migration: Add line-item signature and tolerance for recurring patterns
-- Feature: #332 - Tolerant line-item matching for recurring invoice eligibility
--
-- Adds a structured JSONB signature (normalized description → aggregated cents)
-- and a tolerance knob so that harmless OCR wording, punctuation, line splits,
-- and sub-cent rounding no longer cause avoidable ineligible verdicts.

-- Aggregated line-item signature: [{ "description": "rent", "amount_cents": 10000 }, ...]
-- Sorted by normalized description key. NULL means legacy row; first sample will backfill.
ALTER TABLE recurring_patterns
    ADD COLUMN last_line_items_signature JSONB;

-- Percentage tolerance for line-item amount comparison (default 5%, mirrors amount_tolerance_pct).
ALTER TABLE recurring_patterns
    ADD COLUMN line_item_tolerance_pct REAL NOT NULL DEFAULT 0.05;
