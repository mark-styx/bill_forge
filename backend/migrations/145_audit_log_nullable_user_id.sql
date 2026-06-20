-- 145: Allow audit_log.user_id to be NULL.
--
-- PAT (Personal Access Token) authenticated requests act under an API key
-- identity, not a user. To record audit entries for those mutations
-- (webhook subscription create/delete, API key create/revoke) we need
-- user_id to accept NULL; the API key identity is encoded in changes->>'user_email'
-- (e.g. "api-key:<api_key_id>").
ALTER TABLE audit_log ALTER COLUMN user_id DROP NOT NULL;
