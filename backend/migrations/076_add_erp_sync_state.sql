-- ERP sync orchestration: persist per-connector/entity sync cursors and conflict log.
-- Refs #117

CREATE TABLE IF NOT EXISTS erp_sync_state (
    tenant_id          UUID        NOT NULL,
    connector          TEXT        NOT NULL,
    entity_type        TEXT        NOT NULL,
    cursor             JSONB       NOT NULL DEFAULT '{}',
    last_sync_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_remote_version TEXT,
    conflict_count     INT         NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, connector, entity_type)
);

CREATE INDEX IF NOT EXISTS idx_erp_sync_state_tenant_connector
    ON erp_sync_state (tenant_id, connector);

CREATE TABLE IF NOT EXISTS erp_sync_conflicts (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    connector   TEXT        NOT NULL,
    entity_type TEXT        NOT NULL,
    local_id    TEXT        NOT NULL,
    remote_id   TEXT        NOT NULL,
    reason      TEXT        NOT NULL,
    resolution  TEXT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_erp_sync_conflicts_tenant_connector
    ON erp_sync_conflicts (tenant_id, connector);
