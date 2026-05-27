#!/usr/bin/env bash
# Run Bill Forge coverage gates and write a compact local summary.
set -euo pipefail

MODE="${1:-all}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
COVERAGE_DIR="$PROJECT_ROOT/coverage"
SUMMARY_FILE="$PROJECT_ROOT/coverage-summary.md"

mkdir -p "$COVERAGE_DIR"

write_header() {
  cat > "$SUMMARY_FILE" <<EOF
# Coverage Summary

Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

EOF
}

run_web() {
  echo "=== Web coverage ==="
  local log_file="$COVERAGE_DIR/web-coverage.txt"
  (
    cd "$PROJECT_ROOT"
    pnpm --filter @billforge/web exec vitest run --coverage --coverage.reportsDirectory=../../coverage/web
  ) | tee "$log_file"

  {
    echo "## Web"
    echo
    echo "- Command: \`pnpm --filter @billforge/web exec vitest run --coverage --coverage.reportsDirectory=../../coverage/web\`"
    echo "- Report: \`coverage/web/index.html\`"
    echo "- Text output: \`coverage/web-coverage.txt\`"
    echo
  } >> "$SUMMARY_FILE"
}

run_backend() {
  echo "=== Backend coverage ==="
  mkdir -p "$COVERAGE_DIR/backend"
  if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    {
      echo "## Backend"
      echo
      echo "- Status: skipped; \`cargo-llvm-cov\` is not installed."
      echo "- Install: \`cargo install cargo-llvm-cov\`"
      echo
    } >> "$SUMMARY_FILE"
    echo "cargo-llvm-cov is required for backend coverage. Install with: cargo install cargo-llvm-cov" >&2
    return 1
  fi

  local log_file="$COVERAGE_DIR/backend-coverage.txt"
  local backend_args="${BF_BACKEND_COVERAGE_ARGS:--p billforge-core --lib}"
  (
    cd "$PROJECT_ROOT/backend"
    # shellcheck disable=SC2086
    cargo +stable llvm-cov $backend_args --lcov --output-path "$COVERAGE_DIR/backend/lcov.info"
    cargo +stable llvm-cov report --summary-only
  ) | tee "$log_file"

  {
    echo "## Backend"
    echo
    echo "- Command: \`cd backend && cargo +stable llvm-cov ${backend_args} --lcov --output-path ../coverage/backend/lcov.info\`"
    echo "- LCOV report: \`coverage/backend/lcov.info\`"
    echo "- Text output: \`coverage/backend-coverage.txt\`"
    echo "- Override scope with \`BF_BACKEND_COVERAGE_ARGS\` when a broader backend coverage pass is needed."
    echo
  } >> "$SUMMARY_FILE"
}

write_header

case "$MODE" in
  all)
    run_web
    run_backend
    ;;
  web)
    run_web
    ;;
  backend)
    run_backend
    ;;
  *)
    echo "Usage: $0 [all|web|backend]" >&2
    exit 2
    ;;
esac

echo "Coverage summary written to $SUMMARY_FILE"
