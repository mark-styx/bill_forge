-- Migration 134: In-app notification inbox (refs #375)
--
-- The dashboard NotificationCenter bell was decorative: layout.tsx held an
-- empty `useState<Notification[]>([])` and notificationsApi only exposed
-- Slack/Teams config endpoints. This table is the per-user read feed that
-- finally gives the bell real data, with a primary producer wired at
-- approval-request creation time.
--
-- Mirrors the NULLIF-hardened RLS pattern from migrations 092/121/133 so an
-- unset/empty app.current_tenant_id denies rows instead of raising a UUID
-- cast error, matching the rest of the tenant-isolation surface (#368).

-- ---------------------------------------------------------------------------
-- 1. in_app_notifications table
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS in_app_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT,
    link TEXT NULL,
    read_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ---------------------------------------------------------------------------
-- 2. Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_in_app_notifications_tenant_user_created
    ON in_app_notifications (tenant_id, user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_in_app_notifications_unread
    ON in_app_notifications (tenant_id, user_id)
    WHERE read_at IS NULL;

-- ---------------------------------------------------------------------------
-- 3. RLS (follows pattern from migrations 092/121/133)
-- ---------------------------------------------------------------------------
ALTER TABLE in_app_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE in_app_notifications FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_in_app_notifications ON in_app_notifications;
CREATE POLICY rls_tenant_in_app_notifications ON in_app_notifications
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

COMMENT ON TABLE in_app_notifications IS 'Per-user in-app notification inbox; tenant-isolated via RLS, populated by approval-request creation and other producers.';
