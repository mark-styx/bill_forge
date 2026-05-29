#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

bash scripts/verify-fast.sh
bash scripts/verify-db.sh

docker build -f docker/Dockerfile.backend -t billforge-api:test .
docker build -f docker/Dockerfile.frontend -t billforge-web:test .
