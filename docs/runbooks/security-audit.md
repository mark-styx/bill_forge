# Security Audit Runbook

Bill Forge uses `scripts/security-audit.sh` as the dependency security gate for Rust and Node.js packages.

## Runbook

1. Install prerequisites:
   - `cargo install cargo-audit`
   - `pnpm install --frozen-lockfile`
2. Run the audit:
   - `pnpm security:audit`
3. Review `security-audit-report.json`.
4. Treat critical and high findings as P1. These block CI and must be remediated or explicitly documented before release.
5. Treat medium findings as P2. They should be reviewed in the next hardening pass and tracked if not immediately remediated.

## Review Rules

- Prefer dependency upgrades or lockfile overrides when an advisory has a patched version.
- If an advisory is not reachable in Bill Forge runtime paths, document the rationale in the ticket or release notes.
- Do not suppress P1 findings in CI without an owner, expiry date, and compensating control.
- Re-run `pnpm security:audit` after changing dependencies or overrides.

## Evidence

- CI uploads `security-audit-report.json` for each run.
- Local reports are ignored by git.
- A clean pilot release has `p1_count = 0`.
