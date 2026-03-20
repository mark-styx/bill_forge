#!/bin/bash
# Seed test data for predictive analytics demo

set -e

export $(cat .env | grep -v '^#' | xargs)

echo "🌱 Seeding test data for predictive analytics..."
echo ""

# Insert test vendors
psql "$DATABASE_URL" << 'EOF'
-- Insert test vendors
INSERT INTO vendors (id, tenant_id, name, category, payment_terms, created_at)
VALUES
  ('a0000000-0000-0000-0000-000000000001', '11111111-1111-1111-1111-111111111111', 'Acme Corp', 'Office Supplies', 'Net 30', NOW()),
  ('a0000000-0000-0000-0000-000000000002', '11111111-1111-1111-1111-111111111111', 'TechFlow Inc', 'Software', 'Net 15', NOW()),
  ('a0000000-0000-0000-0000-000000000003', '11111111-1111-1111-1111-111111111111', 'GreenLeaf Supplies', 'Office Supplies', 'Net 30', NOW())
ON CONFLICT (id) DO NOTHING;

-- Insert invoices with spend patterns (last 90 days)
-- This creates realistic patterns for forecasting
INSERT INTO invoices (id, tenant_id, vendor_id, invoice_number, amount, currency, status, invoice_date, due_date, created_at)
SELECT
  gen_random_uuid(),
  '11111111-1111-1111-1111-111111111111',
  (ARRAY['a0000000-0000-0000-0000-000000000001', 'a0000000-0000-0000-0000-000000000002', 'a0000000-0000-0000-0000-000000000003'])[1 + (i % 3)],
  'INV-' || i,
  1000 + (i * 10) + (random() * 100),  -- Growing trend with variance
  'USD',
  'approved',
  NOW() - (90 - i) * INTERVAL '1 day',
  NOW() - (90 - i) * INTERVAL '1 day' + INTERVAL '30 days',
  NOW() - (90 - i) * INTERVAL '1 day'
FROM generate_series(1, 90) AS i
ON CONFLICT DO NOTHING;

-- Insert some anomalies
-- 1. Duplicate invoice
INSERT INTO invoices (id, tenant_id, vendor_id, invoice_number, amount, currency, status, invoice_date, due_date, created_at)
VALUES
  (gen_random_uuid(), '11111111-1111-1111-1111-111111111111', 'a0000000-0000-0000-0000-000000000001', 'DUP-001', 5000, 'USD', 'pending', NOW(), NOW() + INTERVAL '30 days', NOW()),
  (gen_random_uuid(), '11111111-1111-1111-1111-111111111111', 'a0000000-0000-0000-0000-000000000001', 'DUP-001', 5000, 'USD', 'pending', NOW(), NOW() + INTERVAL '30 days', NOW());

-- 2. Unusually high amount (outlier)
INSERT INTO invoices (id, tenant_id, vendor_id, invoice_number, amount, currency, status, invoice_date, due_date, created_at)
VALUES
  (gen_random_uuid(), '11111111-1111-1111-1111-111111111111', 'a0000000-0000-0000-0000-000000000001', 'OUTLIER-001', 50000, 'USD', 'pending', NOW(), NOW() + INTERVAL '30 days', NOW());

-- Count what we created
SELECT 'Vendors' as table_name, COUNT(*) FROM vendors WHERE tenant_id = '11111111-1111-1111-1111-111111111111'
UNION ALL
SELECT 'Invoices', COUNT(*) FROM invoices WHERE tenant_id = '11111111-1111-1111-1111-111111111111';
EOF

echo ""
echo "✅ Test data seeded successfully!"
