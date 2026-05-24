-- Migration 087: AI action proposal status contract and failed execution errors
--
-- Moves proposals from the original approval_required/cancelled/expired status
-- set to the smaller action proposal status contract and enforces deterministic
-- execution error fields for failed proposals.

ALTER TABLE ai_action_proposals
    ADD COLUMN IF NOT EXISTS execution_error_code TEXT,
    ADD COLUMN IF NOT EXISTS execution_error_message TEXT;

ALTER TABLE ai_action_proposals
    DROP CONSTRAINT IF EXISTS ai_action_proposals_status_check,
    DROP CONSTRAINT IF EXISTS ai_action_proposals_failed_execution_error_check,
    DROP CONSTRAINT IF EXISTS ai_action_proposals_execution_error_code_format_check;

ALTER TABLE ai_action_proposals
    ALTER COLUMN status SET DEFAULT 'pending';

UPDATE ai_action_proposals
SET status = 'pending'
WHERE status = 'approval_required';

UPDATE ai_action_proposals
SET status = 'rejected'
WHERE status IN ('cancelled', 'expired');

ALTER TABLE ai_action_proposals
    ADD CONSTRAINT ai_action_proposals_status_check
        CHECK (status IN ('pending', 'approved', 'rejected', 'executed', 'failed')),
    ADD CONSTRAINT ai_action_proposals_failed_execution_error_check
        CHECK (
            (
                status = 'failed'
                AND execution_error_code IS NOT NULL
                AND execution_error_message IS NOT NULL
            )
            OR
            (
                status <> 'failed'
                AND execution_error_code IS NULL
                AND execution_error_message IS NULL
            )
        ),
    ADD CONSTRAINT ai_action_proposals_execution_error_code_format_check
        CHECK (
            execution_error_code IS NULL
            OR execution_error_code ~ '^[a-z0-9]+(_[a-z0-9]+)*$'
        );
