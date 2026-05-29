#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

bash scripts/backend-static-check.sh
bash scripts/backend-unit-test.sh
pnpm --filter @billforge/web test:run
pnpm --filter @billforge/web typecheck
pnpm lint
pnpm build
