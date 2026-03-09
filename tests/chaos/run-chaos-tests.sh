#!/bin/bash
#
# Chaos Engineering Test Runner
# Runs all chaos tests against staging environment
#
# Usage: ./run-chaos-tests.sh [test-name]
#
# WARNING: Only run in STAGING environment!
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
CHAOS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="${CHAOS_DIR}/logs"
REPORT_FILE="${LOG_DIR}/chaos-test-report-$(date +%Y%m%d_%H%M%S).json"

# Environment checks
check_environment() {
    echo -e "${YELLOW}Checking environment...${NC}"

    # Ensure we're not in production
    if [[ "${ENVIRONMENT:-}" == "production" ]]; then
        echo -e "${RED}ERROR: Chaos tests must NOT run in production!${NC}"
        exit 1
    fi

    # Check required tools
    local tools=("docker" "kubectl" "pumba" "toxiproxy-cli")
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            echo -e "${YELLOW}Warning: $tool not found. Some tests may be skipped.${NC}"
        fi
    done

    echo -e "${GREEN}Environment check passed${NC}"
}

# Create log directory
mkdir -p "$LOG_DIR"

# Initialize report
initialize_report() {
    cat > "$REPORT_FILE" <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "environment": "${ENVIRONMENT:-staging}",
  "tests": []
}
EOF
}

# Add test result to report
add_test_result() {
    local test_name="$1"
    local status="$2"
    local duration="$3"
    local details="$4"

    # Use jq to append to JSON array
    jq --arg name "$test_name" \
       --arg status "$status" \
       --arg duration "$duration" \
       --arg details "$details" \
       '.tests += [{
         "name": $name,
         "status": $status,
         "duration": $duration,
         "details": $details,
         "timestamp": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'"
       }]' "$REPORT_FILE" > "${REPORT_FILE}.tmp" && mv "${REPORT_FILE}.tmp" "$REPORT_FILE"
}

# Run a single chaos test
run_test() {
    local test_script="$1"
    local test_name=$(basename "$test_script" .sh)

    echo ""
    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}Running: ${test_name}${NC}"
    echo -e "${YELLOW}========================================${NC}"

    local start_time=$(date +%s)

    if bash "$test_script"; then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        echo -e "${GREEN}✓ ${test_name} PASSED (${duration}s)${NC}"
        add_test_result "$test_name" "passed" "${duration}s" ""
    else
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        echo -e "${RED}✗ ${test_name} FAILED (${duration}s)${NC}"
        add_test_result "$test_name" "failed" "${duration}s" "Check logs for details"
        return 1
    fi
}

# Main test runner
main() {
    echo -e "${YELLOW}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${YELLOW}║     BillForge Chaos Engineering Tests       ║${NC}"
    echo -e "${YELLOW}╚══════════════════════════════════════════════╝${NC}"
    echo ""

    check_environment
    initialize_report

    # Get test to run (or all tests)
    local test_filter="${1:-}"
    local failed_tests=0
    local total_tests=0

    # Find and run tests
    for test_script in "$CHAOS_DIR"/test-*.sh; do
        [[ -x "$test_script" ]] || continue

        local test_name=$(basename "$test_script" .sh)

        # Filter tests if specified
        if [[ -n "$test_filter" && "$test_name" != *"$test_filter"* ]]; then
            continue
        fi

        ((total_tests++))

        if ! run_test "$test_script"; then
            ((failed_tests++))
        fi
    done

    # Summary
    echo ""
    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}Test Summary${NC}"
    echo -e "${YELLOW}========================================${NC}"
    echo "Total tests: $total_tests"
    echo -e "Passed: ${GREEN}$((total_tests - failed_tests))${NC}"
    echo -e "Failed: ${RED}${failed_tests}${NC}"
    echo ""
    echo "Report saved to: $REPORT_FILE"

    if [[ $failed_tests -gt 0 ]]; then
        echo -e "${RED}Some tests failed. Check the report for details.${NC}"
        exit 1
    else
        echo -e "${GREEN}All chaos tests passed!${NC}"
        exit 0
    fi
}

# Run main
main "$@"
