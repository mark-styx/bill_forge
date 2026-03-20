#!/bin/bash
# Simple test data seeder for predictive analytics

export $(cat .env | grep -v '^#' | xargs)

echo "🌱 Creating minimal test data..."

# Get or create a user for created_by
USER_ID=$(psql "$DATABASE_URL" -t -c "SELECT id FROM users WHERE tenant_id = '11111111-1111-1111-1111-111111111111' LIMIT 1;" | tr -d ' ')

if [ -z "$USER_ID" ]; then
  echo "Creating test user..."
  psql "$DATABASE_URL" << EOF
INSERT INTO users (id, tenant_id, email, password_hash, name, roles, is_active)
VALUES (
  '00000000-0000-0000-0000-000000000001',
  '11111111-1111-1111-1111-111111111111',
  'demo@test.com',
  '\$argon2id\$v=19\$m=4096,t=3,p=1\$test\$test',
  'Demo User',
  '["admin"]',
  true
) ON CONFLICT DO NOTHING;
EOF
  USER_ID='00000000-0000-0000-0000-000000000001'
fi

echo "Using user ID: $USER_ID"

# Create vendors
psql "$DATABASE_URL" << EOF
INSERT INTO vendors (id, tenant_id, name, payment_terms, is_active) VALUES
  ('a0000000-0000-0000-0000-000000000001', '11111111-1111-1111-1111-111111111111', 'Acme Corp', 'Net 30', true),
  ('a0000000-0000-0000-0000-000000000002', '11111111-1111-1111-1111-111111111111', 'TechFlow Inc', 'Net 15', true),
  ('a0000000-0000-0000-0000-000000000003', '11111111-1111-1111-1111-111111111111', 'GreenLeaf Supplies', 'Net 30', true)
ON CONFLICT (tenant_id, name) DO NOTHING;
EOF

echo "Creating invoices with spend patterns (this may take a moment)..."

# Create invoices with patterns for forecasting (90 days)
for i in $(seq 1 90); do
  VENDOR_ID=$(printf "a0000000-0000-0000-0000-00000000000%d" $((1 + (i % 3))))
  AMOUNT=$((100000 + (i * 1000) + RANDOM % 10000))  # Growing trend with variance
  DATE_DAYS=$((90 - i))

  psql "$DATABASE_URL" -q << EOF
INSERT INTO invoices (
  tenant_id, vendor_id, vendor_name, invoice_number,
  total_amount_cents, currency, capture_status, processing_status,
  invoice_date, due_date, document_id, created_by, line_items
) VALUES (
  '11111111-1111-1111-1111-111111111111',
  '$VENDOR_ID',
  (SELECT name FROM vendors WHERE id = '$VENDOR_ID'),
  'TEST-INV-$i',
  $AMOUNT,
  'USD',
  'completed',
  'approved',
  CURRENT_DATE - $DATE_DAYS,
  CURRENT_DATE - $DATE_DAYS + 30,
  gen_random_uuid(),
  '$USER_ID',
  '[]'::jsonb
);
EOF
done

echo ""
echo "✅ Created test data:"
psql "$DATABASE_URL" -c "
SELECT
  (SELECT COUNT(*) FROM vendors WHERE tenant_id = '11111111-1111-1111-1111-111111111111') as vendors,
  (SELECT COUNT(*) FROM invoices WHERE tenant_id = '11111111-1111-1111-1111-111111111111') as invoices;
"
