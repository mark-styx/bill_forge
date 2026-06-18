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

export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export RUSTFLAGS="${RUSTFLAGS:--C debuginfo=0}"

# Migration 120 creates the billforge_app NOSUPERUSER NOBYPASSRLS role that
# PgManager's fail-closed gate requires. Provision a password and pass it via
# BILLFORGE_APP_PASSWORD so migrate can set the role's password.
export BILLFORGE_APP_PASSWORD="${BILLFORGE_APP_PASSWORD:-billforge_app_test_pw}"

cargo run -p billforge-db --bin migrate -- --database-url "$DATABASE_URL" up

# After migrations, route tests through the restricted role so PgManager does
# not refuse to start. Unconditionally overwrite TEST_DATABASE_URL: workflows
# pre-set it to the superuser handle in their env block, which would otherwise
# defeat the :- fallback below.
DB_HOST_PORT_PATH="${DATABASE_URL#postgres://postgres:postgres@}"
export TEST_DATABASE_URL="postgres://billforge_app:${BILLFORGE_APP_PASSWORD}@${DB_HOST_PORT_PATH}"

cargo test --workspace --all-features --tests -- --test-threads=1
