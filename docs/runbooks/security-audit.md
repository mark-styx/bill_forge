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

## Current P2 Exceptions

Reviewed on May 27, 2026. Owner: Engineering. Expiry: June 30, 2026.

| Package | Advisory | Path | Rationale | Next action |
|---------|----------|------|-----------|-------------|
| `rsa` | RUSTSEC-2023-0071 | Optional `sqlx-mysql` lockfile dependency | Bill Forge enables PostgreSQL-only `sqlx` features. `cargo tree --workspace --all-features --target all -i rsa@0.9.10` reports no reachable package path, but `cargo-audit` still scans the full lockfile. | Re-check after the next `sqlx` update; remove if `cargo-audit` adds feature-aware filtering or the lockfile no longer includes `sqlx-mysql`. |
| `rustls-webpki` | RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104 | AWS SDK HTTP/TLS stack | Direct `reqwest` usage has been moved to native TLS. Remaining occurrences are pulled by `aws-config`, `aws-sdk-s3`, and `aws-sdk-textract`. No stable patched `rustls-webpki` release is available in the resolved AWS SDK chain at this review date. | Upgrade AWS SDK/rustls stack when a stable patched chain is available. |

## Resolved P2 Findings

Reviewed on May 27, 2026. The mobile tooling P2 findings for `fast-xml-parser` (GHSA-gh4j-gqv2-49f6) and `uuid` (GHSA-w5hq-g745-h8pq) are remediated with root `pnpm.overrides`. `pnpm audit --json` now reports only workspace-name `sandbox` advisories with no concrete dependency paths, which `scripts/security-audit.sh` intentionally excludes from P1/P2 counts.

## Evidence

- CI uploads `security-audit-report.json` for each run.
- Local reports are ignored by git.
- A clean pilot release has `p1_count = 0`.
