-- Migration 153: Per-invoice workflow stage cursor (refs #429)
--
-- Workflow templates are stateless: apply_workflow_template iterates the
-- template's stages from order 0 on every invocation and stops at the first
-- non-skipped stage. With nothing persisting which stage already captured the
-- invoice, re-entering process_invoice re-evaluates every prior stage's
-- skip / auto-advance conditions from scratch, collapsing multi-level approval
-- chains (e.g. dept approval -> finance approval) to a single stage.
--
-- workflow_stage_progress records the cursor per (tenant, invoice, template):
--   * current_stage_order        -- highest stage order already advanced past
--                                   (skipped, auto-advanced, or captured)
--   * last_captured_stage_order  -- stage order that returned a status on the
--                                   most recent capture; NULL when no stage
--                                   has captured yet
--   * last_captured_stage_name   -- human-readable name of that stage, for
--                                   audit/debugging
--
-- On re-entry, apply_workflow_template loads the cursor and resumes at
-- (last_captured_stage_order + 1) so the previously captured stage is not
-- re-evaluated. Skip / auto-advance decisions also advance the cursor so
-- they are not re-evaluated on later re-entries either.

CREATE TABLE IF NOT EXISTS workflow_stage_progress (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    invoice_id UUID NOT NULL,
    template_id UUID NOT NULL,
    current_stage_order INT NOT NULL DEFAULT 0,
    last_captured_stage_order INT NULL,
    last_captured_stage_name TEXT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, invoice_id, template_id)
);

CREATE INDEX IF NOT EXISTS idx_workflow_stage_progress_invoice
    ON workflow_stage_progress (tenant_id, invoice_id);
