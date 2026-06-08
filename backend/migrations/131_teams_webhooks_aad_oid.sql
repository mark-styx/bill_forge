-- Bind Microsoft AAD object id to each Teams webhook registration so the
-- Teams Actionable Messages handler can attribute approvals to a real user
-- based on a validated JWT 'oid' claim instead of LIMIT 1 over the table.
-- Refs: issue #362.

ALTER TABLE teams_webhooks
    ADD COLUMN IF NOT EXISTS aad_object_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS teams_webhooks_tenant_aad_active_idx
    ON teams_webhooks (tenant_id, aad_object_id)
    WHERE is_active = true AND aad_object_id IS NOT NULL;

COMMENT ON COLUMN teams_webhooks.aad_object_id IS
    'Microsoft AAD object id (oid claim) of the Teams user. Required for #362 JWT-validated approvals: the /teams/actions handler resolves the actor via (tenant_id, aad_object_id) when TEAMS_ACTIONS_ENABLED=true. This value must be populated out-of-band by a trusted operator (e.g. by querying Microsoft Graph for the registered Teams user) — never accepted from an unverified user-facing endpoint, since trusting a self-reported oid would let any tenant user impersonate another in Teams callbacks.';
