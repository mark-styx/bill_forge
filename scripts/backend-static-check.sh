#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR/backend"

export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export RUSTFLAGS="${RUSTFLAGS:--C debuginfo=0}"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
