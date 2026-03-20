#!/bin/bash
# Demo Predictive Analytics - Quick Start

set -e

export $(cat .env | grep -v '^#' | xargs)
BASE_URL="http://localhost:8001"
TENANT_ID="11111111-1111-1111-1111-111111111111"

echo "🔮 BillForge Predictive Analytics Demo"
echo "======================================="
echo ""

# Check services are running
echo "✅ Checking services..."
echo "   Server: $(curl -s $BASE_URL/health | jq -r '.status')"
echo "   Worker: $(ps aux | grep billforge-worker | grep -v grep | awk '{print "running (PID " $2 ")"}')"
echo "   Database: $(psql "$DATABASE_URL" -t -c "SELECT 'connected' LIMIT 1;" | tr -d ' \n')"
echo ""

# Show test data
echo "📊 Test Data Summary:"
psql "$DATABASE_URL" << EOF
SELECT
  v.name as vendor,
  COUNT(i.id) as invoice_count,
  MIN(i.invoice_date) as first_invoice,
  MAX(i.invoice_date) as last_invoice,
  AVG(i.total_amount_cents / 100.0)::numeric(10,2) as avg_amount
FROM invoices i
JOIN vendors v ON i.vendor_id = v.id
WHERE i.tenant_id = '$TENANT_ID'
GROUP BY v.name
ORDER BY v.name;
EOF
echo ""

# Trigger background jobs manually
echo "🔄 Running Forecast Generation Job..."
echo "   (This would normally run weekly via scheduler)"
echo ""

curl -s -X POST "$BASE_URL/api/v1/sandbox/trigger-job" \
  -H "Content-Type: application/json" \
  -d '{
    "job_type": "ForecastRefresh",
    "tenant_id": "'$TENANT_ID'"
  }' 2>&1 || echo "   Note: Manual trigger endpoint not available (jobs run on schedule)"

echo ""
echo "🔄 Running Anomaly Detection Job..."
echo "   (This would normally run daily via scheduler)"
echo ""

curl -s -X POST "$BASE_URL/api/v1/sandbox/trigger-job" \
  -H "Content-Type: application/json" \
  -d '{
    "job_type": "AnomalyDetection",
    "tenant_id": "'$TENANT_ID'"
  }' 2>&1 || echo "   Note: Manual trigger endpoint not available (jobs run on schedule)"

echo ""
echo "📝 Available Endpoints (require auth token):"
echo "   GET  /api/v1/analytics/predictive/forecasts"
echo "   POST /api/v1/analytics/predictive/forecasts/generate"
echo "   GET  /api/v1/analytics/predictive/anomalies"
echo "   POST /api/v1/analytics/predictive/anomalies/detect"
echo "   GET  /api/v1/analytics/predictive/alerts"
echo ""

echo "💡 To test the endpoints with authentication:"
echo "   1. Login: curl -X POST $BASE_URL/api/v1/auth/login ..."
echo "   2. Use the returned token: curl -H 'Authorization: Bearer <token>' ..."
echo ""

echo "📈 Background Jobs Status:"
echo "   Worker is polling Redis for jobs..."
echo "   Forecasts will be generated weekly"
echo "   Anomalies will be detected daily"
echo ""

echo "✨ The predictive analytics system is ready!"
echo "   Services: http://localhost:8001"
echo "   API Docs: http://localhost:8001/swagger (if enabled)"
