-- Migration 140: Persisted OFAC/SDN list versions (refs #395)
--
-- Replaces the hardcoded `list_version = 'seed-v1'` string with a real
-- persisted history of SDN list snapshots. The daily ofac_refresh worker
-- writes one row per content-hash change so screening_results carries the
-- actual version + load timestamp and a vendor sanctioned after the seed
-- was embedded stops passing screening once a refresh ingests them.
--
-- A NULL/empty table is allowed; `OfacScreener::load_latest` falls back to the
-- compiled-in seed so cold-start tenants still screen against the embedded
-- list while the first refresh tick is pending.

CREATE TABLE IF NOT EXISTS ofac_list_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Either 'seed-v1' (embedded) or 'sdn-<short hash>' for refreshed snapshots.
    list_version TEXT NOT NULL,
    loaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    entry_count INTEGER NOT NULL,
    entries_json JSONB NOT NULL,
    -- 'embedded' for the seed fallback, the resolved URL otherwise.
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ofac_list_versions_loaded_at
    ON ofac_list_versions (loaded_at DESC);

CREATE INDEX IF NOT EXISTS idx_ofac_list_versions_list_version
    ON ofac_list_versions (list_version);

COMMENT ON TABLE ofac_list_versions IS
    'Persisted OFAC/SDN list snapshots written by the daily worker refresh. Latest row is what OfacScreener::load_latest reads; empty table => fall back to embedded seed.';
