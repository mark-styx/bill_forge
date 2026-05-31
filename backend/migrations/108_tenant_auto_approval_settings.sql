-- 108: Document and backfill auto-approval tenant settings.
--
-- The tenant touchless auto-approval lane reads two new JSONB keys from
-- tenants.settings:
--   auto_approval_threshold  FLOAT   — per-tenant categorization confidence
--                                      threshold (NULL → use the global
--                                      default of 0.95 defined in engine.rs).
--   auto_approval_enabled    BOOL    — allows a tenant to disable the
--                                      touchless lane entirely (default true).
-- Both keys are optional; the Rust TenantSettings struct provides safe
-- defaults via #[serde(default)].

-- Backfill auto_approval_enabled = true for every existing tenant that
-- hasn't set the key yet.
UPDATE tenants
SET settings = settings || '{"auto_approval_enabled": true}'::jsonb
WHERE NOT (settings ? 'auto_approval_enabled');
