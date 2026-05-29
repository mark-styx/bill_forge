-- Queue item reassignment updates this column; older tenant migration paths
-- created queue_items without it.

ALTER TABLE IF EXISTS queue_items
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
