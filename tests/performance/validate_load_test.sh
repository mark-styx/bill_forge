#!/bin/bash
#
# Validates load test and alert rule configuration for CI.
# Checks:
#   1. K6 load test script parses correctly (k6 inspect)
#   2. Prometheus alert rules are syntactically valid (promtool)
#   3. Every alert in alerts.yml has a matching runbook anchor
#
# Usage:
#   ./validate_load_test.sh [--skip-k6] [--skip-promtool]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

SKIP_K6=false
SKIP_PROMTOOL=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-k6) SKIP_K6=true; shift ;;
        --skip-promtool) SKIP_PROMTOOL=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

ERRORS=0

# ---------------------------------------------------------------------------
# 1. Validate K6 load test script
# ---------------------------------------------------------------------------
if [[ "$SKIP_K6" == "false" ]]; then
    if command -v k6 &> /dev/null; then
        echo -e "${YELLOW}[1/3] Validating K6 load test script...${NC}"
        if k6 inspect "${SCRIPT_DIR}/api_load_test.js" > /dev/null 2>&1; then
            echo -e "${GREEN}  ✓ K6 script syntax valid${NC}"

            # Verify scenarios are defined
            SCENARIOS=$(k6 inspect "${SCRIPT_DIR}/api_load_test.js" 2>/dev/null | grep -c '"monthly_5k_throughput"\|"ocr_pipeline_stress"\|"vu_ramp"' || true)
            if [[ "$SCENARIOS" -ge 3 ]]; then
                echo -e "${GREEN}  ✓ All 3 scenarios found (monthly_5k_throughput, ocr_pipeline_stress, vu_ramp)${NC}"
            else
                echo -e "${RED}  ✗ Expected 3 scenarios, found $SCENARIOS${NC}"
                ERRORS=$((ERRORS + 1))
            fi

            # Verify thresholds
            THRESHOLDS=$(k6 inspect "${SCRIPT_DIR}/api_load_test.js" 2>/dev/null | grep -c 'thresholds' || true)
            if [[ "$THRESHOLDS" -ge 1 ]]; then
                echo -e "${GREEN}  ✓ Thresholds defined${NC}"
            else
                echo -e "${RED}  ✗ No thresholds found in K6 script${NC}"
                ERRORS=$((ERRORS + 1))
            fi
        else
            echo -e "${RED}  ✗ K6 script syntax invalid${NC}"
            k6 inspect "${SCRIPT_DIR}/api_load_test.js" 2>&1 || true
            ERRORS=$((ERRORS + 1))
        fi
    else
        echo -e "${YELLOW}[1/3] SKIP: k6 not installed (install with: brew install k6)${NC}"
    fi
else
    echo -e "${YELLOW}[1/3] SKIP: K6 validation disabled${NC}"
fi

# ---------------------------------------------------------------------------
# 2. Validate Prometheus alert rules
# ---------------------------------------------------------------------------
if [[ "$SKIP_PROMTOOL" == "false" ]]; then
    ALERTS_FILE="${PROJECT_ROOT}/config/prometheus/alerts.yml"
    if command -v promtool &> /dev/null; then
        echo -e "${YELLOW}[2/3] Validating Prometheus alert rules...${NC}"
        if promtool check rules "$ALERTS_FILE" 2>&1; then
            echo -e "${GREEN}  ✓ Prometheus alert rules valid${NC}"
        else
            echo -e "${RED}  ✗ Prometheus alert rules invalid${NC}"
            ERRORS=$((ERRORS + 1))
        fi
    else
        echo -e "${YELLOW}[2/3] SKIP: promtool not installed (part of Prometheus distribution)${NC}"
        # Basic YAML syntax check as fallback
        if command -v python3 &> /dev/null; then
            echo "  Running basic YAML syntax check..."
            if python3 -c "import yaml; yaml.safe_load(open('$ALERTS_FILE'))" 2>&1; then
                echo -e "${GREEN}  ✓ YAML syntax valid (basic check)${NC}"
            else
                echo -e "${RED}  ✗ YAML syntax invalid${NC}"
                ERRORS=$((ERRORS + 1))
            fi
        fi
    fi
else
    echo -e "${YELLOW}[2/3] SKIP: Prometheus validation disabled${NC}"
fi

# ---------------------------------------------------------------------------
# 3. Cross-check: every alert has a runbook anchor
# ---------------------------------------------------------------------------
echo -e "${YELLOW}[3/3] Cross-checking alerts against runbook...${NC}"
ALERTS_FILE="${PROJECT_ROOT}/config/prometheus/alerts.yml"
RUNBOOK_FILE="${PROJECT_ROOT}/docs/runbooks/on-call-operations.md"

if [[ ! -f "$ALERTS_FILE" ]]; then
    echo -e "${RED}  ✗ alerts.yml not found at $ALERTS_FILE${NC}"
    ERRORS=$((ERRORS + 1))
elif [[ ! -f "$RUNBOOK_FILE" ]]; then
    echo -e "${RED}  ✗ Runbook not found at $RUNBOOK_FILE${NC}"
    ERRORS=$((ERRORS + 1))
else
    # Extract unique alert names
    ALERT_NAMES=$(grep -o '\- alert: [A-Za-z]*' "$ALERTS_FILE" | sed 's/- alert: //' | sort -u)

    MISSING=0
    FOUND=0
    while IFS= read -r alert_name; do
        ANCHOR=$(echo "$alert_name" | tr '[:upper:]' '[:lower:]')
        if grep -q "{#${ANCHOR}}" "$RUNBOOK_FILE"; then
            FOUND=$((FOUND + 1))
        else
            echo -e "${RED}  ✗ Missing runbook section for alert: ${alert_name} (expected anchor: #${ANCHOR})${NC}"
            MISSING=$((MISSING + 1))
            ERRORS=$((ERRORS + 1))
        fi
    done <<< "$ALERT_NAMES"

    if [[ "$MISSING" -eq 0 ]]; then
        echo -e "${GREEN}  ✓ All ${FOUND} alert(s) have matching runbook sections${NC}"
    else
        echo -e "${RED}  ✗ ${MISSING} alert(s) missing runbook sections, ${FOUND} found${NC}"
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
if [[ "$ERRORS" -eq 0 ]]; then
    echo -e "${GREEN}✓ All validations passed${NC}"
    exit 0
else
    echo -e "${RED}✗ ${ERRORS} validation error(s) found${NC}"
    exit 1
fi
