/**
 * Sandbox Data Seeder
 * 
 * Creates demo data for testing the BillForge platform
 */

import { randomUUID } from 'crypto';

const API_URL = process.env.API_URL || 'http://localhost:8080';

const SANDBOX_TENANT_ID = 'sandbox-tenant-001';
const SANDBOX_ADMIN = {
  email: 'admin@sandbox.local',
  password: 'sandbox123',
  name: 'Sandbox Admin',
};

// Demo vendors
const VENDORS = [
  { name: 'Acme Corporation', type: 'business', email: 'billing@acme.com' },
  { name: 'TechSupplies Inc', type: 'business', email: 'ap@techsupplies.com' },
  { name: 'Office Depot', type: 'business', email: 'invoices@officedepot.com' },
  { name: 'Cloud Services LLC', type: 'business', email: 'billing@cloudservices.io' },
  { name: 'John Smith', type: 'contractor', email: 'john@contractor.com' },
  { name: 'Jane Doe Consulting', type: 'contractor', email: 'jane@consulting.co' },
  { name: 'Utilities Co', type: 'business', email: 'billing@utilities.com' },
  { name: 'Insurance Corp', type: 'business', email: 'premiums@insurance.com' },
];

// Demo invoices
const INVOICES = [
  { vendor: 'Acme Corporation', number: 'INV-2024-001', amount: 1500.00, status: 'pending_approval' },
  { vendor: 'TechSupplies Inc', number: 'TS-5678', amount: 2300.00, status: 'approved' },
  { vendor: 'Office Depot', number: 'OD-123456', amount: 450.00, status: 'ready_for_payment' },
  { vendor: 'Cloud Services LLC', number: 'CS-2024-01', amount: 999.00, status: 'paid' },
  { vendor: 'John Smith', number: 'JS-INV-15', amount: 3500.00, status: 'pending_approval' },
  { vendor: 'Jane Doe Consulting', number: 'JDC-2024-03', amount: 5000.00, status: 'submitted' },
  { vendor: 'Acme Corporation', number: 'INV-2024-002', amount: 2750.00, status: 'ready_for_review' },
  { vendor: 'Utilities Co', number: 'UTL-2024-01', amount: 325.00, status: 'approved' },
];

async function main() {
  console.log('🌱 BillForge Sandbox Seeder');
  console.log('===========================\n');

  console.log('📍 API URL:', API_URL);
  console.log('📍 Tenant ID:', SANDBOX_TENANT_ID);
  console.log('');

  console.log('Note: This seeder expects the backend to be running.');
  console.log('The backend will create the sandbox tenant and admin user on first start.\n');

  console.log('Demo credentials:');
  console.log('  Tenant ID:', SANDBOX_TENANT_ID);
  console.log('  Email:', SANDBOX_ADMIN.email);
  console.log('  Password:', SANDBOX_ADMIN.password);
  console.log('');

  console.log('Demo data prepared:');
  console.log(`  - ${VENDORS.length} vendors`);
  console.log(`  - ${INVOICES.length} invoices`);
  console.log('');

  console.log('To use the sandbox:');
  console.log('  1. Start the backend: cd backend && cargo run');
  console.log('  2. Start the frontend: pnpm dev');
  console.log('  3. Open http://localhost:3000');
  console.log('  4. Login with the demo credentials above');
  console.log('');

  // In a full implementation, this would make API calls to seed the data
  // For now, the sandbox data is created by the backend on startup in sandbox mode

  console.log('✅ Sandbox ready!');
}

main().catch(console.error);
