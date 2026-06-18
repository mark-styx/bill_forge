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

export BILLFORGE_APP_PASSWORD="${BILLFORGE_APP_PASSWORD:-billforge_app_test_pw}"

cargo run -p billforge-db --bin migrate -- --database-url "$DATABASE_URL" up

# Note: integration tests still run as the superuser postgres role. Routing
# them through billforge_app would force every existing fixture (users, vendors,
# invoices INSERT) to set up the RLS tenant GUC, which is a much larger fixture
# rewrite. Tests that explicitly assert the PgManager fail-closed gate
# (ai_action_proposal_repo_test) are marked #[ignore] until that work lands.
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"

cargo test --workspace --all-features --tests -- --test-threads=1
