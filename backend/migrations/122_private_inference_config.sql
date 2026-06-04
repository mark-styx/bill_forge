-- 122: Per-tenant private-inference configuration (refs #334)
--
-- Stores endpoint URLs, health status, and KMS key references for tenants
-- running OCR / embeddings / categorization inside their own VPC or on-prem
-- cluster.  Only the OCR endpoint is wired in the initial slice; the
-- categorization and embeddings columns are reserved for follow-up work.

CREATE TABLE IF NOT EXISTS tenant_private_inference (
    tenant_id                      UUID PRIMARY KEY REFERENCES tenants(id),
    enabled                        BOOLEAN NOT NULL DEFAULT FALSE,
    ocr_endpoint_url               TEXT,
    categorization_endpoint_url    TEXT,
    embeddings_endpoint_url        TEXT,
    kms_key_ref                    TEXT,
    compliance_attestation_text    TEXT,
    health_status                  TEXT NOT NULL DEFAULT 'unknown'
                                   CHECK (health_status IN ('healthy', 'unhealthy', 'unknown')),
    last_health_check_at           TIMESTAMPTZ,
    last_health_error              TEXT,
    created_at                     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tenant_private_inference
    IS 'Per-tenant private-inference mode configuration.  When enabled AND healthy, OCR / embeddings / categorization traffic is routed to customer-managed endpoints inside the tenant network.';

-- RLS: tenant isolation (mirrors migration 121 NULLIF-hardened pattern)
ALTER TABLE tenant_private_inference ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_private_inference FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_private_inference ON tenant_private_inference;
CREATE POLICY rls_tenant_private_inference ON tenant_private_inference
    USING  (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

-- Grant DML to the app role (matches migration 120 / 121 pattern)
GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_private_inference TO billforge_app;
