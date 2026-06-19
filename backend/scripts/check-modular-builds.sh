#!/usr/bin/env bash
# Verifies that `billforge-api` compiles with each pillar enabled on its own
# (`--no-default-features --features <pillar>`), proving a single-pillar binary
# (e.g. Capture-only) is buildable. Issue #365.
#
# Run from the repository root or the backend/ workspace.
#
# Wiring this into CI is deferred (see issue #365 follow-up); this is the manual
# / local-dev invocation invoked by the PR description.

set -euo pipefail

# Resolve the backend workspace directory regardless of where the script is run from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${BACKEND_DIR}"

PILLARS=(
  "capture"
  "processing"
  "vendor-mgmt"
  "reporting"
  "billing"
  "ai-agent"
  "analytics"
)

echo "==> Checking default (all-pillar) build"
cargo check -p billforge-api

for pillar in "${PILLARS[@]}"; do
  echo "==> Checking single-pillar build: ${pillar}"
  cargo check -p billforge-api --no-default-features --features "${pillar}"
done

echo "==> All modular build checks passed"
