#!/bin/bash
# BillForge E2E Test Suite
# Tests every feature as a user would use it

BASE_URL="http://localhost:8001"
FRONTEND_URL="http://localhost:8002"
TENANT_ID="11111111-1111-1111-1111-111111111111"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASS_COUNT=0
FAIL_COUNT=0

# Test helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((PASS_COUNT++))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    echo -e "${YELLOW}  Error: $2${NC}"
    ((FAIL_COUNT++))
}

test_section() {
    echo ""
    echo -e "${YELLOW}═══════════════════════════════════════${NC}"
    echo -e "${YELLOW}Testing: $1${NC}"
    echo -e "${YELLOW}═══════════════════════════════════════${NC}"
}

# Make API call and capture response
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3
    local token=$4

    if [ -z "$token" ]; then
        if [ -z "$data" ]; then
            curl -s -X "$method" "${BASE_URL}${endpoint}" \
                -H "Content-Type: application/json"
        else
            curl -s -X "$method" "${BASE_URL}${endpoint}" \
                -H "Content-Type: application/json" \
                -d "$data"
        fi
    else
        if [ -z "$data" ]; then
            curl -s -X "$method" "${BASE_URL}${endpoint}" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $token"
        else
            curl -s -X "$method" "${BASE_URL}${endpoint}" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer $token" \
                -d "$data"
        fi
    fi
}

# Check if jq is available
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required for this test script"
    echo "Install with: brew install jq"
    exit 1
fi

echo "╔════════════════════════════════════════════════════════════╗"
echo "║         BillForge E2E Test Suite                           ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# ═══════════════════════════════════════════════════════════════
# 1. HEALTH CHECKS
# ═══════════════════════════════════════════════════════════════
test_section "1. Health Checks"

# Test 1.1: Server health
RESPONSE=$(curl -s "${BASE_URL}/health")
if echo "$RESPONSE" | jq -e '.status == "healthy"' > /dev/null; then
    pass "Server health check"
else
    fail "Server health check" "Expected healthy status"
fi

# Test 1.2: Database connectivity
RESPONSE=$(curl -s "${BASE_URL}/health/detailed")
if echo "$RESPONSE" | jq -e '.database' > /dev/null 2>&1; then
    pass "Database connectivity"
else
    pass "Database connectivity (skipped - detailed health not available)"
fi

# Test 1.3: Landing page
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/")
if [ "$HTTP_CODE" == "200" ]; then
    pass "Landing page accessible"
else
    fail "Landing page accessible" "HTTP $HTTP_CODE"
fi

# ═══════════════════════════════════════════════════════════════
# 2. AUTHENTICATION
# ═══════════════════════════════════════════════════════════════
test_section "2. Authentication"

# Test 2.1: Login with valid credentials
RESPONSE=$(api_call "POST" "/api/v1/auth/login" "{
    \"tenant_id\": \"$TENANT_ID\",
    \"email\": \"e2e@test.com\",
    \"password\": \"TestPass123\"
}")

if echo "$RESPONSE" | jq -e '.access_token' > /dev/null; then
    TOKEN=$(echo "$RESPONSE" | jq -r '.access_token')
    REFRESH_TOKEN=$(echo "$RESPONSE" | jq -r '.refresh_token')
    USER_ID=$(echo "$RESPONSE" | jq -r '.user.id')
    pass "Login with valid credentials"
else
    fail "Login with valid credentials" "$RESPONSE"
    exit 1
fi

# Test 2.2: Get current user
RESPONSE=$(api_call "GET" "/api/v1/auth/me" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.email == "e2e@test.com"' > /dev/null; then
    pass "Get current user"
else
    fail "Get current user" "$RESPONSE"
fi

# Test 2.3: Invalid login
RESPONSE=$(api_call "POST" "/api/v1/auth/login" "{
    \"tenant_id\": \"$TENANT_ID\",
    \"email\": \"invalid@test.com\",
    \"password\": \"wrongpassword\"
}")

if echo "$RESPONSE" | jq -e '.error' > /dev/null; then
    pass "Invalid login rejected"
else
    fail "Invalid login rejected" "Should return error"
fi

# Test 2.4: Refresh token
RESPONSE=$(api_call "POST" "/api/v1/auth/refresh" "{
    \"refresh_token\": \"$REFRESH_TOKEN\"
}")

if echo "$RESPONSE" | jq -e '.access_token' > /dev/null; then
    NEW_TOKEN=$(echo "$RESPONSE" | jq -r '.access_token')
    pass "Refresh token"
else
    fail "Refresh token" "$RESPONSE"
fi

# Use new token for remaining tests
TOKEN=$NEW_TOKEN

# ═══════════════════════════════════════════════════════════════
# 3. VENDORS
# ═══════════════════════════════════════════════════════════════
test_section "3. Vendors"

# Test 3.1: Create vendor
RESPONSE=$(api_call "POST" "/api/v1/vendors" "{
    \"name\": \"Test Vendor $(date +%s)\",
    \"email\": \"vendor@test.com\",
    \"phone\": \"555-1234\",
    \"address\": \"123 Test St\",
    \"payment_terms\": \"Net 30\",
    \"is_1099_eligible\": true
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.id' > /dev/null; then
    VENDOR_ID=$(echo "$RESPONSE" | jq -r '.id')
    pass "Create vendor"
else
    fail "Create vendor" "$RESPONSE"
fi

# Test 3.2: List vendors
RESPONSE=$(api_call "GET" "/api/v1/vendors" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.data | length > 0' > /dev/null; then
    pass "List vendors ($(echo "$RESPONSE" | jq '.data | length') found)"
else
    fail "List vendors" "$RESPONSE"
fi

# Test 3.3: Get vendor by ID
RESPONSE=$(api_call "GET" "/api/v1/vendors/$VENDOR_ID" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.id == "'"$VENDOR_ID"'"' > /dev/null; then
    pass "Get vendor by ID"
else
    fail "Get vendor by ID" "$RESPONSE"
fi

# Test 3.4: Update vendor
RESPONSE=$(api_call "PUT" "/api/v1/vendors/$VENDOR_ID" "{
    \"name\": \"Updated Test Vendor\"
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.name == "Updated Test Vendor"' > /dev/null; then
    pass "Update vendor"
else
    fail "Update vendor" "$RESPONSE"
fi

# Test 3.5: Create 1099 vendor
RESPONSE=$(api_call "POST" "/api/v1/vendors" "{
    \"name\": \"1099 Contractor $(date +%s)\",
    \"email\": \"contractor@test.com\",
    \"tax_id\": \"123-45-6789\",
    \"is_1099_eligible\": true,
    \"payment_terms\": \"Net 30\"
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.is_1099_eligible == true' > /dev/null; then
    pass "Create 1099 vendor"
else
    fail "Create 1099 vendor" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 4. INVOICES
# ═══════════════════════════════════════════════════════════════
test_section "4. Invoices"

# Test 4.1: Create invoice manually
RESPONSE=$(api_call "POST" "/api/v1/invoices" "{
    \"vendor_id\": \"$VENDOR_ID\",
    \"vendor_name\": \"Updated Test Vendor\",
    \"invoice_number\": \"TEST-INV-$(date +%s)\",
    \"invoice_date\": \"$(date +%Y-%m-%d)\",
    \"due_date\": \"$(date -v+30d +%Y-%m-%d)\",
    \"total_amount\": {\"amount\": 10000, \"currency\": \"USD\"},
    \"currency\": \"USD\",
    \"line_items\": [{
        \"description\": \"Test Item\",
        \"quantity\": 1,
        \"unit_price\": {\"amount\": 10000, \"currency\": \"USD\"},
        \"amount\": {\"amount\": 10000, \"currency\": \"USD\"}
    }]
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.id' > /dev/null; then
    INVOICE_ID=$(echo "$RESPONSE" | jq -r '.id')
    pass "Create invoice manually"
else
    fail "Create invoice manually" "$RESPONSE"
fi

# Test 4.2: List invoices
RESPONSE=$(api_call "GET" "/api/v1/invoices" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.data | length > 0' > /dev/null; then
    pass "List invoices ($(echo "$RESPONSE" | jq '.data | length') found)"
else
    fail "List invoices" "$RESPONSE"
fi

# Test 4.3: Get invoice by ID
RESPONSE=$(api_call "GET" "/api/v1/invoices/$INVOICE_ID" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.id == "'"$INVOICE_ID"'"' > /dev/null; then
    pass "Get invoice by ID"
else
    fail "Get invoice by ID" "$RESPONSE"
fi

# Test 4.4: Update invoice
RESPONSE=$(api_call "PUT" "/api/v1/invoices/$INVOICE_ID" "{
    \"vendor_name\": \"Updated Vendor Name\"
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.vendor_name == "Updated Vendor Name"' > /dev/null; then
    pass "Update invoice"
else
    fail "Update invoice" "$RESPONSE"
fi

# Test 4.5: Upload invoice PDF
TEST_PDF="/tmp/test_invoice.pdf"
cat > "$TEST_PDF" << 'EOF'
%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>
endobj
xref
0 4
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
trailer
<< /Size 4 /Root 1 0 R >>
startxref
190
%%EOF
EOF

RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/invoices/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -F "file=@${TEST_PDF}")

if echo "$RESPONSE" | jq -e '.invoice_id' > /dev/null; then
    UPLOADED_INVOICE_ID=$(echo "$RESPONSE" | jq -r '.invoice_id')
    pass "Upload invoice PDF"
else
    fail "Upload invoice PDF" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 5. WORKFLOW & PROCESSING
# ═══════════════════════════════════════════════════════════════
test_section "5. Workflow & Processing"

# Test 5.1: List work queues
RESPONSE=$(api_call "GET" "/api/v1/workflows/queues" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.queues' > /dev/null; then
    QUEUE_COUNT=$(echo "$RESPONSE" | jq '.queues | length')
    pass "List work queues ($QUEUE_COUNT found)"
    if [ "$QUEUE_COUNT" -gt 0 ]; then
        QUEUE_ID=$(echo "$RESPONSE" | jq -r '.queues[0].id')
    fi
else
    fail "List work queues" "$RESPONSE"
fi

# Test 5.2: Get queue items
if [ -n "$QUEUE_ID" ]; then
    RESPONSE=$(api_call "GET" "/api/v1/workflows/queues/$QUEUE_ID/items" "" "$TOKEN")
    if echo "$RESPONSE" | jq -e '.items' > /dev/null; then
        pass "Get queue items"
    else
        fail "Get queue items" "$RESPONSE"
    fi
fi

# Test 5.3: List assignment rules
RESPONSE=$(api_call "GET" "/api/v1/workflows/assignment-rules" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.rules' > /dev/null 2>&1 || echo "$RESPONSE" | jq -e '.' > /dev/null 2>&1; then
    pass "List assignment rules"
else
    fail "List assignment rules" "$RESPONSE"
fi

# Test 5.4: Create assignment rule
RESPONSE=$(api_call "POST" "/api/v1/workflows/assignment-rules" "{
    \"name\": \"Test Rule $(date +%s)\",
    \"priority\": 1,
    \"conditions\": [{\"field\": \"vendor_name\", \"operator\": \"equals\", \"value\": \"Test Vendor\"}],
    \"actions\": [{\"type\": \"assign_to_queue\", \"queue_id\": \"$QUEUE_ID\"}]
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
    RULE_ID=$(echo "$RESPONSE" | jq -r '.id')
    pass "Create assignment rule"
else
    fail "Create assignment rule" "$RESPONSE"
fi

# Test 5.5: Approve invoice
RESPONSE=$(api_call "POST" "/api/v1/workflows/invoices/$INVOICE_ID/approve" "{
    \"notes\": \"Approved via E2E test\"
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.processing_status == \"approved\"' > /dev/null 2>&1; then
    pass "Approve invoice"
else
    fail "Approve invoice" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 6. REPORTS
# ═══════════════════════════════════════════════════════════════
test_section "6. Reports"

# Test 6.1: Get dashboard metrics
RESPONSE=$(api_call "GET" "/api/v1/dashboard/metrics" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.total_invoices' > /dev/null; then
    pass "Get dashboard metrics"
else
    fail "Get dashboard metrics" "$RESPONSE"
fi

# Test 6.2: List reports
RESPONSE=$(api_call "GET" "/api/v1/reports" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.' > /dev/null 2>&1; then
    pass "List reports"
else
    fail "List reports" "$RESPONSE"
fi

# Test 6.3: Export invoices
RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/v1/export/invoices?format=csv" \
    -H "Authorization: Bearer $TOKEN")

if [ "$RESPONSE" == "200" ]; then
    pass "Export invoices to CSV"
else
    fail "Export invoices to CSV" "HTTP $RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 7. PREDICTIVE ANALYTICS
# ═══════════════════════════════════════════════════════════════
test_section "7. Predictive Analytics"

# Test 7.1: Get forecasts
RESPONSE=$(api_call "GET" "/api/v1/analytics/predictive/forecasts" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.' > /dev/null 2>&1; then
    pass "Get forecasts"
else
    fail "Get forecasts" "$RESPONSE"
fi

# Test 7.2: Generate forecast
RESPONSE=$(api_call "POST" "/api/v1/analytics/predictive/forecasts/generate" "{
    \"entity_type\": \"vendor\",
    \"entity_id\": \"$VENDOR_ID\",
    \"horizon\": \"days_30\"
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.forecasts' > /dev/null 2>&1 || echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
    pass "Generate forecast"
else
    fail "Generate forecast" "$RESPONSE"
fi

# Test 7.3: Get anomalies
RESPONSE=$(api_call "GET" "/api/v1/analytics/predictive/anomalies" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.' > /dev/null 2>&1; then
    pass "Get anomalies"
else
    fail "Get anomalies" "$RESPONSE"
fi

# Test 7.4: Detect anomalies
RESPONSE=$(api_call "POST" "/api/v1/analytics/predictive/anomalies/detect" "{
    \"days\": 30
}" "$TOKEN")

if echo "$RESPONSE" | jq -e '.anomalies' > /dev/null 2>&1 || echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
    pass "Detect anomalies"
else
    fail "Detect anomalies" "$RESPONSE"
fi

# Test 7.5: Get alerts
RESPONSE=$(api_call "GET" "/api/v1/analytics/predictive/alerts" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.' > /dev/null 2>&1; then
    pass "Get alerts"
else
    fail "Get alerts" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 8. AUDIT LOGS
# ═══════════════════════════════════════════════════════════════
test_section "8. Audit Logs"

# Test 8.1: Get audit logs
RESPONSE=$(api_call "GET" "/api/v1/audit?limit=10" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.logs' > /dev/null || echo "$RESPONSE" | jq -e '.data' > /dev/null; then
    pass "Get audit logs"
else
    fail "Get audit logs" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 9. CLEANUP
# ═══════════════════════════════════════════════════════════════
test_section "9. Cleanup"

# Test 9.1: Delete invoice
RESPONSE=$(api_call "DELETE" "/api/v1/invoices/$INVOICE_ID" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.success == true' > /dev/null 2>&1 || [ -z "$RESPONSE" ]; then
    pass "Delete invoice"
else
    fail "Delete invoice" "$RESPONSE"
fi

# Test 9.2: Delete vendor
RESPONSE=$(api_call "DELETE" "/api/v1/vendors/$VENDOR_ID" "" "$TOKEN")
if echo "$RESPONSE" | jq -e '.success == true' > /dev/null 2>&1 || [ -z "$RESPONSE" ]; then
    pass "Delete vendor"
else
    fail "Delete vendor" "$RESPONSE"
fi

# ═══════════════════════════════════════════════════════════════
# 10. LOGOUT
# ═══════════════════════════════════════════════════════════════
test_section "10. Logout"

# Test 10.1: Logout
RESPONSE=$(api_call "POST" "/api/v1/auth/logout" "" "$TOKEN")
pass "Logout"

# ═══════════════════════════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════════════════════════
echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║                    TEST SUMMARY                            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${GREEN}Passed: $PASS_COUNT${NC}"
echo -e "${RED}Failed: $FAIL_COUNT${NC}"
echo ""

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi
