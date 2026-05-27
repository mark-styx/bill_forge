# Readiness Gates

Use these commands before promoting pilot-facing work.

## Coverage

- Web: `pnpm coverage:web`
- Backend: `pnpm coverage:backend`
- Full local coverage: `pnpm coverage`

Backend coverage requires the stable Rust toolchain and `cargo-llvm-cov`:

```bash
rustup toolchain install stable
cargo install cargo-llvm-cov
```

Coverage artifacts are written under `coverage/`, and the local summary is written to `coverage-summary.md`.
Backend coverage defaults to `billforge-core` library tests so the local gate stays fast and does not require PostgreSQL, Redis, S3, or third-party integration credentials. For broader coverage, set `BF_BACKEND_COVERAGE_ARGS`, for example:

```bash
BF_BACKEND_COVERAGE_ARGS="--workspace --all-features --lib" pnpm coverage:backend
```

## Performance

Run:

```bash
pnpm benchmarks
```

The benchmark suite records:

- frontend dashboard data-shaping latency
- backend workflow condition evaluation latency

The local report is written to `performance-benchmark-report.md`.

## Security

Run:

```bash
pnpm security:audit
```

The gate fails on P1 critical/high dependency findings and writes `security-audit-report.json`.
