-- Migration 071: Add foreign key constraints on tenant_id for core tables
-- These three foundational tables were created without FK references to tenants(id).
-- All newer tables (quickbooks, xero, notifications, predictive analytics, etc.)
-- already have this constraint. This brings the core tables into alignment.

-- Users table
ALTER TABLE users
  ADD CONSTRAINT fk_users_tenant_id
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

-- Vendors table
ALTER TABLE vendors
  ADD CONSTRAINT fk_vendors_tenant_id
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

-- Invoices table
ALTER TABLE invoices
  ADD CONSTRAINT fk_invoices_tenant_id
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
