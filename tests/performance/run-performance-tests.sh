#!/bin/bash
#
# Performance Regression Test Runner
# Runs k6 performance tests and compares against baseline
#
# Usage:
#   ./run-performance-tests.sh [--baseline] [--compare baseline.json]
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
RESULTS_DIR="${SCRIPT_DIR}/results"
BASELINE_FILE="${RESULTS_DIR}/baseline.json"
LATEST_FILE="${RESULTS_DIR}/latest.json"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Parse arguments
BASELINE_MODE=false
COMPARE_FILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --baseline)
            BASELINE_MODE=true
            shift
            ;;
        --compare)
            COMPARE_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Ensure results directory exists
mkdir -p "$RESULTS_DIR"

# Check dependencies
if ! command -v k6 &> /dev/null; then
    echo -e "${RED}ERROR: k6 not found. Install with: brew install k6${NC}"
    exit 1
fi

# Generate API token for testing
echo -e "${YELLOW}Generating test API token...${NC}"
export API_TOKEN=$(cargo run -p billforge-auth --bin generate-token -- test-tenant 2>/dev/null | grep -o 'eyJ[^ ]*' | head -1)

if [[ -z "$API_TOKEN" ]]; then
    echo -e "${RED}ERROR: Failed to generate API token${NC}"
    echo "Make sure the API is running and accessible"
    exit 1
fi

export API_URL="${API_URL:-http://localhost:8000}"
export TENANT_ID="${TENANT_ID:-test-tenant}"

echo -e "${GREEN}API Token generated${NC}"
echo "API URL: $API_URL"
echo "Tenant ID: $TENANT_ID"

# Run performance tests
echo ""
echo -e "${YELLOW}╔══════════════════════════════════════════════╗${NC}"
echo -e "${YELLOW}║   Running Performance Regression Tests       ║${NC}"
echo -e "${YELLOW}╚══════════════════════════════════════════════╝${NC}"
echo ""

TEST_START=$(date +%s)

# Run k6 test
k6 run \
    --out json="${LATEST_FILE}" \
    --summary-export="${RESULTS_DIR}/summary.json" \
    "${SCRIPT_DIR}/api_load_test.js"

TEST_END=$(date +%s)
TEST_DURATION=$((TEST_END - TEST_START))

echo ""
echo -e "${YELLOW}Test completed in ${TEST_DURATION}s${NC}"

# If baseline mode, save as baseline
if [[ "$BASELINE_MODE" == "true" ]]; then
    echo -e "${YELLOW}Saving as baseline...${NC}"
    cp "$LATEST_FILE" "$BASELINE_FILE"
    echo -e "${GREEN}Baseline saved to: $BASELINE_FILE${NC}"
    exit 0
fi

# If compare mode, compare against specified file
if [[ -n "$COMPARE_FILE" ]]; then
    BASELINE_FILE="$COMPARE_FILE"
fi

# Compare against baseline if it exists
if [[ -f "$BASELINE_FILE" ]]; then
    echo ""
    echo -e "${YELLOW}Comparing against baseline...${NC}"

    # Run comparison script
    "${SCRIPT_DIR}/compare-performance.sh" "$BASELINE_FILE" "$LATEST_FILE"

    COMPARISON_EXIT_CODE=$?

    if [[ $COMPARISON_EXIT_CODE -eq 0 ]]; then
        echo -e "${GREEN}✓ Performance test passed - no significant regressions${NC}"
    else
        echo -e "${RED}✗ Performance test failed - regressions detected${NC}"
    fi

    exit $COMPARISON_EXIT_CODE
else
    echo ""
    echo -e "${YELLOW}No baseline found. Run with --baseline to create one.${NC}"
    echo "Results saved to: $LATEST_FILE"
    exit 0
fi
