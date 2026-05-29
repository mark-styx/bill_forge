#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR/backend"

CREATED_DATABASE=""
ADMIN_DATABASE_URL="${ADMIN_DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"

if [[ -z "${DATABASE_URL:-}" ]]; then
  CREATED_DATABASE="billforge_test_${USER:-local}_$$"
  psql "$ADMIN_DATABASE_URL" -v ON_ERROR_STOP=1 -c "CREATE DATABASE \"$CREATED_DATABASE\""
  trap 'psql "$ADMIN_DATABASE_URL" -v ON_ERROR_STOP=1 -c "DROP DATABASE IF EXISTS \"'"$CREATED_DATABASE"'\" WITH (FORCE)"' EXIT
  export DATABASE_URL="postgres://postgres:postgres@localhost:5432/$CREATED_DATABASE"
fi

export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export RUSTFLAGS="${RUSTFLAGS:--C debuginfo=0}"

cargo run -p billforge-db --bin migrate -- --database-url "$DATABASE_URL" up
cargo test --workspace --all-features --tests -- --test-threads=1
