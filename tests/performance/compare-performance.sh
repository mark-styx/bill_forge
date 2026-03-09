#!/bin/bash
#
# Performance Comparison Script
# Compares two k6 performance test results and identifies regressions
#
# Usage: ./compare-performance.sh baseline.json current.json
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Configuration
REGRESSION_THRESHOLD_PCT=10  # Alert if > 10% regression
BLOCK_THRESHOLD_PCT=20       # Block PR if > 20% regression

# Check arguments
if [[ $# -lt 2 ]]; then
    echo "Usage: $0 baseline.json current.json"
    exit 1
fi

BASELINE_FILE="$1"
CURRENT_FILE="$2"

# Verify files exist
if [[ ! -f "$BASELINE_FILE" ]]; then
    echo -e "${RED}ERROR: Baseline file not found: $BASELINE_FILE${NC}"
    exit 1
fi

if [[ ! -f "$CURRENT_FILE" ]]; then
    echo -e "${RED}ERROR: Current results file not found: $CURRENT_FILE${NC}"
    exit 1
fi

echo -e "${YELLOW}Comparing performance results...${NC}"
echo "Baseline: $BASELINE_FILE"
echo "Current:  $CURRENT_FILE"
echo ""

# Function to extract metric from k6 JSON
extract_metric() {
    local file="$1"
    local metric_name="$2"
    local stat="$3"  # avg, p(95), p(99), etc

    # k6 JSON output has one JSON object per line
    # We need to aggregate metrics
    jq -s \
        --arg metric "$metric_name" \
        --arg stat "$stat" \
        '
        map(select(.type == "Point" and .metric == $metric)) |
        map(.data.value) |
        if length == 0 then null
        elif $stat == "avg" then (add / length)
        elif $stat == "min" then min
        elif $stat == "max" then max
        elif $stat | startswith("p(") then
            # Calculate percentile
            . as $values |
            ($values | length) as $len |
            ($stat | gsub("[^0-9]"; "") | tonumber / 100) as $p |
            ($len * $p | floor) as $idx |
            ($values | sort)[$idx]
        else null
        end
        ' "$file"
}

# Metrics to compare
METRICS=(
    "http_req_duration:avg:HTTP Request Duration (avg)"
    "http_req_duration:p(95):HTTP Request Duration (P95)"
    "http_req_duration:p(99):HTTP Request Duration (P99)"
    "api_latency:avg:API Latency (avg)"
    "api_latency:p(95):API Latency (P95)"
    "invoice_upload_time:p(95):Invoice Upload Time (P95)"
    "approval_time:p(95):Approval Time (P95)"
    "dashboard_load_time:p(95):Dashboard Load Time (P95)"
    "iteration_duration:avg:Iteration Duration (avg)"
)

REGRESSIONS=()
CRITICAL_REGRESSIONS=false

echo -e "${YELLOW}╔════════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${YELLOW}║                      Performance Comparison                        ║${NC}"
echo -e "${YELLOW}╚════════════════════════════════════════════════════════════════════╝${NC}"
echo ""

printf "%-40s %10s %10s %10s %6s\n" "Metric" "Baseline" "Current" "Change" "Status"
printf "%-40s %10s %10s %10s %6s\n" "------" "--------" "-------" "------" "------"

for METRIC_SPEC in "${METRICS[@]}"; do
    IFS=':' read -r METRIC_NAME STAT METRIC_LABEL <<< "$METRIC_SPEC"

    BASELINE_VALUE=$(extract_metric "$BASELINE_FILE" "$METRIC_NAME" "$STAT")
    CURRENT_VALUE=$(extract_metric "$CURRENT_FILE" "$METRIC_NAME" "$STAT")

    # Skip if metric not found
    if [[ "$BASELINE_VALUE" == "null" || "$CURRENT_VALUE" == "null" ]]; then
        printf "%-40s %10s %10s %10s %6s\n" "$METRIC_LABEL" "N/A" "N/A" "N/A" "SKIP"
        continue
    fi

    # Calculate percentage change
    CHANGE=$(echo "scale=2; (($CURRENT_VALUE - $BASELINE_VALUE) / $BASELINE_VALUE) * 100" | bc)

    # Format values
    BASELINE_FMT=$(printf "%.2f" "$BASELINE_VALUE")
    CURRENT_FMT=$(printf "%.2f" "$CURRENT_VALUE")
    CHANGE_FMT=$(printf "%+.2f%%" "$CHANGE")

    # Determine status
    if (( $(echo "$CHANGE > $BLOCK_THRESHOLD_PCT" | bc -l) )); then
        STATUS="${RED}FAIL${NC}"
        CRITICAL_REGRESSIONS=true
        REGRESSIONS+=("$METRIC_LABEL: ${CHANGE_FMT} (threshold: ${BLOCK_THRESHOLD_PCT}%)")
    elif (( $(echo "$CHANGE > $REGRESSION_THRESHOLD_PCT" | bc -l) )); then
        STATUS="${YELLOW}WARN${NC}"
        REGRESSIONS+=("$METRIC_LABEL: ${CHANGE_FMT} (threshold: ${REGRESSION_THRESHOLD_PCT}%)")
    elif (( $(echo "$CHANGE < -20" | bc -l) )); then
        STATUS="${GREEN}IMPROV${NC}"
    else
        STATUS="${GREEN}OK${NC}"
    fi

    printf "%-40s %10s %10s %10s %b\n" "$METRIC_LABEL" "${BASELINE_FMT}ms" "${CURRENT_FMT}ms" "$CHANGE_FMT" "$STATUS"
done

echo ""

# Summary
if [[ "$CRITICAL_REGRESSIONS" == "true" ]]; then
    echo -e "${RED}╔════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║                    CRITICAL REGRESSIONS DETECTED                  ║${NC}"
    echo -e "${RED}╚════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "The following metrics exceeded the ${BLOCK_THRESHOLD_PCT}% regression threshold:"
    echo ""
    for regression in "${REGRESSIONS[@]}"; do
        echo -e "  ${RED}✗${NC} $regression"
    done
    echo ""
    echo -e "${RED}Performance test FAILED - blocking merge${NC}"
    exit 1
elif [[ ${#REGRESSIONS[@]} -gt 0 ]]; then
    echo -e "${YELLOW}╔════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${YELLOW}║              PERFORMANCE REGRESSIONS DETECTED (NON-CRITICAL)      ║${NC}"
    echo -e "${YELLOW}╚════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "The following metrics exceeded the ${REGRESSION_THRESHOLD_PCT}% regression threshold:"
    echo ""
    for regression in "${REGRESSIONS[@]}"; do
        echo -e "  ${YELLOW}⚠${NC} $regression"
    done
    echo ""
    echo -e "${YELLOW}Performance test PASSED with warnings${NC}"
    exit 0
else
    echo -e "${GREEN}╔════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║                  NO PERFORMANCE REGRESSIONS                       ║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${GREEN}Performance test PASSED - all metrics within acceptable range${NC}"
    exit 0
fi
