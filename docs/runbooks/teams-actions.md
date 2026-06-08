# Teams Actionable Messages Runbook

Operator guide for safely enabling and operating the Microsoft Teams approval callback endpoint at `POST /integrations/teams/actions`. The endpoint is default-disabled because incorrect configuration silently grants anyone with a non-empty bearer token the ability to approve invoices on behalf of any user (see issue #362).

## Purpose

`/integrations/teams/actions` accepts approve, reject, and request-changes callbacks from Microsoft Teams Actionable Message cards. When `TEAMS_ACTIONS_ENABLED=true`, every inbound request is validated by `crate::teams_jwt::TeamsJwtValidator`: RS256 signature against the configured JWKS, `iss` and `aud` enforced against env config, expiry and not-before enforced with 60 second leeway. The BillForge actor is then resolved by joining the validated JWT `oid` claim against `teams_webhooks.aad_object_id` for the supplied tenant. There is no fallback to user-supplied identity.

## Prerequisites

1. A Microsoft Entra (Azure AD) tenant.
2. An app registration in that tenant with:
   - An exposed API (Expose an API -> Add a scope) so Teams can mint tokens with an `aud` claim that BillForge can pin.
   - The redirect/callback URI set to the BillForge `/integrations/teams/actions` URL (the same value as `TEAMS_ACTIONS_URL` below).
3. At least one BillForge user already registered via `POST /api/v1/notifications/teams/configure` (this creates the `teams_webhooks` row that the JWT validation will later join against).
4. The AAD object id (`oid`) of each user who will approve via Teams. Look it up with:
   ```bash
   az ad user show --id <user-principal-name> --query id -o tsv
   ```
   You can also pull it from the Entra portal under Users -> Profile -> Object ID.

## Configuration

The six environment variables documented in `.env.example`:

| Variable | Required? | Notes |
|---|---|---|
| `TEAMS_ACTIONS_ENABLED` | Optional | Default `false`. Must be `true` to mount the route. |
| `TEAMS_ACTIONS_URL` | Optional | Full callback URL embedded in outgoing Teams cards. Defaults to `https://api.billforge.io/integrations/teams/actions`. Override per environment (staging, self-hosted, local) so cards point at the right host. |
| `TEAMS_OIDC_JWKS_URL` | Required when enabled | e.g. `https://substrate.office.com/sts/common/discovery/keys`. Confirm the current URL in Microsoft's docs at deploy time. |
| `TEAMS_OIDC_EXPECTED_ISSUER` | Required when enabled | The exact `iss` claim Microsoft signs. Confirm in the Entra portal under App Registrations -> Endpoints. |
| `TEAMS_OIDC_EXPECTED_AUDIENCE` | Required when enabled | Usually the application id URI you set in Expose an API, e.g. `api://billforge-prod`. |
| `TEAMS_JWKS_CACHE_TTL_SECS` | Optional | Default `3600`. Tune down if you are rotating signing keys aggressively. |
| `TEAMS_SKIP_JWT_VALIDATION` | Dev only | Default `false`. Setting `true` falls back to a `LIMIT 1` actor lookup and is intended only for local development against a tenant you do not control. |

`AppState::build_teams_jwt_validator()` enforces this contract at startup. If `TEAMS_ACTIONS_ENABLED=true` and any of the three required vars are unset, the server refuses to start with a clear error. This is intentional, fail loudly at boot beats failing 401 at runtime.

## Backfilling `aad_object_id`

The `teams_webhooks.aad_object_id` column is the binding between the JWT identity and the BillForge user. It is deliberately not populated by `configure_teams` or any other user-facing endpoint, because trusting a self-reported `oid` would let any tenant user impersonate another in Teams callbacks. Population is a trusted-operator step.

Use the `teams_oid_backfill` CLI shipped with the `billforge-db` crate. Always run with `--dry-run` first to confirm the target row:

```bash
cd backend
cargo run -p billforge-db --bin teams_oid_backfill -- \
    --tenant <tenant-uuid> \
    --user <user-uuid> \
    --oid <aad-object-id-uuid> \
    --dry-run
```

If the printed row matches what you expect, re-run without `--dry-run`:

```bash
cargo run -p billforge-db --bin teams_oid_backfill -- \
    --tenant <tenant-uuid> \
    --user <user-uuid> \
    --oid <aad-object-id-uuid>
```

Notes:
- The CLI accepts UUIDs only. Microsoft `oid` claims are always UUIDs; rejecting free-form text catches typos that would silently leave a row unmatchable.
- The pre-write `SELECT` fails loudly if no row matches the tenant+user pair. If you see "No teams_webhooks row for tenant=... user=...", the user has not yet run `configure_teams`.
- The CLI uses `DATABASE_URL_MIGRATIONS` if set, otherwise `DATABASE_URL`. It needs write access to the metadata database; the read-only app role will not work.

## Enabling the flag

Once every user who will approve via Teams has `aad_object_id` populated:

1. Set `TEAMS_ACTIONS_ENABLED=true` in the environment.
2. Restart the API server.
3. In the log output, confirm the boot warning `Teams actions endpoint is ENABLED.` appears. Note: the JWKS is fetched lazily on the first inbound token, so a bad `TEAMS_OIDC_JWKS_URL` will not be discovered at startup, it will surface as a 401 on the first real callback. After flipping the flag, trigger a synthetic callback before declaring the environment healthy.
4. Trigger a test Actionable Message from Teams and confirm the approval lands. A 403 here means the `oid` does not match any registered row; recheck the backfill.

## Dev bypass

For local development against a workstation that has no Entra tenant, set `TEAMS_SKIP_JWT_VALIDATION=true`. The handler then skips JWT validation entirely and resolves the actor via the legacy `LIMIT 1` lookup over `teams_webhooks` for the supplied tenant. This is the exact behavior #362 was filed against and must not run anywhere users can reach. The route logs a clear warning when this combination is active.

## Failure modes

| Symptom | Likely cause | Resolution |
|---|---|---|
| Server refuses to start with "TEAMS_OIDC_* required when TEAMS_ACTIONS_ENABLED=true" | One of the three required env vars is unset | Populate the missing variable per the table above. |
| 401 with "Token rejected: MissingKid" | JWT has no `kid` header (unsigned or non-RS256) | Confirm Teams is configured to sign Actionable Messages with the registered app. |
| 401 with "Token rejected: KeyNotFound" | `kid` in the JWT is not in the JWKS at `TEAMS_OIDC_JWKS_URL` | Confirm the JWKS URL is correct for the tenant. If you just rotated keys, wait for cache TTL or restart the server. The validator has a 5-minute minimum gap between miss-driven JWKS refetches (DoS guard); legitimate rotations are tolerated but a flood of garbage `kid` values will not trigger one outbound fetch per token. |
| 401 with "Token rejected: InvalidToken" then `iss` or `aud` | Issuer or audience mismatch | Compare the JWT (decode at jwt.io) against `TEAMS_OIDC_EXPECTED_ISSUER` and `TEAMS_OIDC_EXPECTED_AUDIENCE`. Both must match exactly. |
| 401 with "Token rejected: InvalidToken" then `exp` or `nbf` | Clock skew or stale token | Confirm server clock is in sync. The validator allows 60 seconds of leeway already. |
| 403 with "AAD principal not registered for this tenant" | Validated `oid` has no `teams_webhooks` row for this tenant | Run the backfill CLI for that user. |
| 400 with "Teams actions endpoint is disabled" | `TEAMS_ACTIONS_ENABLED` is not `true` | Set the flag and restart. |

## Rollback

To disable the endpoint immediately, set `TEAMS_ACTIONS_ENABLED=false` (or unset it) and restart. The route reverts to `teams_actions_disabled`, which returns 400 to every caller and logs a warning. No data migration is required, the `aad_object_id` column stays populated and ready for re-enablement.

To remove a single user's binding without disabling the endpoint for everyone else, clear that row's column with the CLI or directly via SQL:

```sql
UPDATE teams_webhooks
   SET aad_object_id = NULL, updated_at = NOW()
 WHERE tenant_id = '<tenant-uuid>' AND user_id = '<user-uuid>';
```

The user's future callbacks will then 403 (fail closed) until you re-backfill.
