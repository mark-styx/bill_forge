-- Migration 149: Invoice risk verdict store (refs #420)
--
-- Per-invoice combined duplicate + fraud risk verdict produced by the
-- InvoiceRiskScorer background service. One row per scoring pass. The
-- exception-queue UI reads the most recent verdict to render the evidence
-- panel; AP reviewers see the supporting signals (duplicate scores, fraud
-- breakdown, amount-spike z-score) inline with the queue item.
--
-- Follows the NULLIF-hardened RLS pattern from migrations 092/121/133/135 so
-- an unset/empty app.current_tenant_id denies rows instead of raising a UUID
-- cast error.

CREATE TABLE IF NOT EXISTS invoice_risk_verdicts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    invoice_id  UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    score       REAL NOT NULL,
    tier        TEXT NOT NULL
        CHECK (tier IN ('clear', 'review', 'block')),
    evidence    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_verdicts_tenant_tier
    ON invoice_risk_verdicts (tenant_id, tier, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_risk_verdicts_invoice
    ON invoice_risk_verdicts (invoice_id, created_at DESC);

ALTER TABLE invoice_risk_verdicts ENABLE ROW LEVEL SECURITY;
ALTER TABLE invoice_risk_verdicts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_invoice_risk_verdicts ON invoice_risk_verdicts;
CREATE POLICY rls_tenant_invoice_risk_verdicts ON invoice_risk_verdicts
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE invoice_risk_verdicts IS 'Per-invoice combined duplicate+fraud risk verdict produced by InvoiceRiskScorer (#420). Tier ''block'' invoices are routed to the exception work queue with evidence JSON.';
