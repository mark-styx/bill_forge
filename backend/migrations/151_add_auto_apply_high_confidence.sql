-- Migration 151: Smart approver auto-routing opt-in (refs #440)
--
-- Adds an opt-in flag on `routing_configuration` so tenants can choose to
-- have the intelligent routing engine prefer a learned (vendor/department,
-- amount-bucket) -> approver pattern over the static rule output when the
-- pattern's confidence meets a high bar. Every override is audit-logged in
-- `routing_optimization_log` with `routing_strategy = 'pattern_learning'`
-- and the confidence score in `candidate_approvers`.

ALTER TABLE routing_configuration
    ADD COLUMN IF NOT EXISTS auto_apply_high_confidence_patterns BOOLEAN NOT NULL DEFAULT false;
