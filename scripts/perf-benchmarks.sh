#!/usr/bin/env bash
# Run lightweight pilot-readiness performance baselines.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT_FILE="$PROJECT_ROOT/performance-benchmark-report.md"
BENCH_DIR="$PROJECT_ROOT/target/readiness-benchmarks"

mkdir -p "$BENCH_DIR"

cat > "$REPORT_FILE" <<EOF
# Performance Benchmark Report

Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

EOF

echo "=== Frontend data-shaping benchmark ==="
FRONTEND_JSON="$BENCH_DIR/frontend-dashboard.json"
node "$PROJECT_ROOT/scripts/frontend-benchmarks.mjs" | tee "$FRONTEND_JSON"

{
  echo "## Frontend Dashboard Data Shaping"
  echo
  echo "\`\`\`json"
  cat "$FRONTEND_JSON"
  echo "\`\`\`"
  echo
} >> "$REPORT_FILE"

echo "=== Backend workflow evaluator benchmark ==="
BACKEND_LOG="$BENCH_DIR/backend-workflow-evaluator.txt"
(
  cd "$PROJECT_ROOT/backend"
  cargo bench -p billforge-core --bench workflow_evaluator -- --sample-size 10
) | tee "$BACKEND_LOG"

{
  echo "## Backend Workflow Evaluator"
  echo
  echo "- Command: \`cd backend && cargo bench -p billforge-core --bench workflow_evaluator -- --sample-size 10\`"
  echo "- Full output: \`target/readiness-benchmarks/backend-workflow-evaluator.txt\`"
  echo
  echo "\`\`\`text"
  tail -n 40 "$BACKEND_LOG"
  echo "\`\`\`"
  echo
} >> "$REPORT_FILE"

echo "Benchmark report written to $REPORT_FILE"
