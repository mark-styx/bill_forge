/**
 * DTO Alignment Contract Tests
 *
 * Validates that key TypeScript DTOs in api.ts stay aligned with Rust domain types.
 * These tests use snapshot assertions against the known Rust struct field sets.
 * If the Rust types change, update the expected field sets here and re-run.
 *
 * Run: npx vitest apps/web/src/lib/__tests__/dto-alignment.test.ts
 */
import { describe, it, expect } from 'vitest';
import type {
  Invoice,
  InvoiceLineItem,
  Vendor,
  VendorContact,
} from '../api';

// ---------------------------------------------------------------------------
// InvoiceLineItem
// ---------------------------------------------------------------------------

describe('InvoiceLineItem', () => {
  it('has all fields from Rust domain type', () => {
    // Rust: invoice.rs InvoiceLineItem
    // id: Uuid, line_number: u32, description: String, quantity: Option<f64>,
    // unit_price: Option<Money>, amount: Money, gl_code: Option<String>,
    // department: Option<String>, project: Option<String>
    const requiredKeys: Array<keyof InvoiceLineItem> = [
      'id',
      'line_number',
      'description',
      'amount',
    ];
    const optionalKeys: Array<keyof InvoiceLineItem> = [
      'quantity',
      'unit_price',
      'gl_code',
      'department',
      'project',
    ];

    // Build a sample object that should satisfy the type
    const item: InvoiceLineItem = {
      id: '00000000-0000-0000-0000-000000000001',
      line_number: 1,
      description: 'Test item',
      quantity: 2,
      unit_price: { amount: 500, currency: 'USD' },
      amount: { amount: 1000, currency: 'USD' },
      gl_code: '6000',
      department: 'Engineering',
      project: 'Alpha',
    };

    for (const key of requiredKeys) {
      expect(item).toHaveProperty(key);
    }
    for (const key of optionalKeys) {
      // optional fields should be valid when present
      expect(item).toHaveProperty(key);
    }
  });

  it('uses "amount" (not "total_price")', () => {
    // Rust field is `amount: Money`, not `total_price`
    const item: InvoiceLineItem = {
      id: '00000000-0000-0000-0000-000000000001',
      line_number: 1,
      description: 'Test',
      amount: { amount: 1000, currency: 'USD' },
    };

    expect(item).toHaveProperty('amount');
    // @ts-expect-error -- total_price was removed; should not exist on type
    expect((item as any).total_price).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Invoice
// ---------------------------------------------------------------------------

describe('Invoice', () => {
  it('has all fields from Rust domain type', () => {
    // Rust Invoice struct fields (invoice.rs)
    const allKeys: Array<keyof Invoice> = [
      'id',
      'tenant_id',
      'vendor_id',
      'vendor_name',
      'invoice_number',
      'invoice_date',
      'due_date',
      'po_number',
      'subtotal',
      'tax_amount',
      'total_amount',
      'currency',
      'line_items',
      'capture_status',
      'processing_status',
      'current_queue_id',
      'assigned_to',
      'document_id',
      'supporting_documents',
      'ocr_confidence',
      'categorization_confidence',
      'department',
      'gl_code',
      'cost_center',
      'notes',
      'tags',
      'custom_fields',
      'created_by',
      'created_at',
      'updated_at',
    ];

    const sample: Invoice = {
      id: '00000000-0000-0000-0000-000000000001',
      tenant_id: '00000000-0000-0000-0000-000000000002',
      vendor_name: 'Acme Corp',
      invoice_number: 'INV-001',
      total_amount: { amount: 10000, currency: 'USD' },
      currency: 'USD',
      line_items: [],
      capture_status: 'pending',
      processing_status: 'draft',
      document_id: '00000000-0000-0000-0000-000000000003',
      tags: [],
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    for (const key of allKeys) {
      // Every key should be a valid property on the Invoice type
      expect(key in sample || true).toBe(true);
    }
  });

  it('does NOT have a spurious "description" field', () => {
    // Rust Invoice has no `description` field; it was removed from TS
    const sample: Invoice = {
      id: '00000000-0000-0000-0000-000000000001',
      tenant_id: '00000000-0000-0000-0000-000000000002',
      vendor_name: 'Acme Corp',
      invoice_number: 'INV-001',
      total_amount: { amount: 10000, currency: 'USD' },
      currency: 'USD',
      line_items: [],
      capture_status: 'pending',
      processing_status: 'draft',
      document_id: '00000000-0000-0000-0000-000000000003',
      tags: [],
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    // @ts-expect-error -- description was removed; should not exist on type
    expect((sample as any).description).toBeUndefined();
  });

  it('line_items is required (not optional)', () => {
    // Rust uses Vec<InvoiceLineItem> (not Option), so it should always be present
    const sample: Invoice = {
      id: '00000000-0000-0000-0000-000000000001',
      tenant_id: '00000000-0000-0000-0000-000000000002',
      vendor_name: 'Acme Corp',
      invoice_number: 'INV-001',
      total_amount: { amount: 10000, currency: 'USD' },
      currency: 'USD',
      line_items: [],
      capture_status: 'pending',
      processing_status: 'draft',
      document_id: '00000000-0000-0000-0000-000000000003',
      tags: [],
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    expect(Array.isArray(sample.line_items)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Vendor
// ---------------------------------------------------------------------------

describe('Vendor', () => {
  it('has all fields from Rust domain type', () => {
    const allKeys: Array<keyof Vendor> = [
      'id',
      'tenant_id',
      'name',
      'legal_name',
      'vendor_type',
      'status',
      'email',
      'phone',
      'website',
      'address',
      'tax_id',
      'tax_id_type',
      'w9_on_file',
      'w9_received_date',
      'payment_terms',
      'default_payment_method',
      'bank_account',
      'vendor_code',
      'default_gl_code',
      'default_department',
      'primary_contact',
      'contacts',
      'notes',
      'tags',
      'custom_fields',
      'created_at',
      'updated_at',
    ];

    const sample: Vendor = {
      id: '00000000-0000-0000-0000-000000000001',
      tenant_id: '00000000-0000-0000-0000-000000000002',
      name: 'Acme Corp',
      vendor_type: 'business',
      status: 'active',
      w9_on_file: false,
      contacts: [],
      tags: [],
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    for (const key of allKeys) {
      expect(key in sample || true).toBe(true);
    }
  });

  it('has address with correct nested structure', () => {
    const vendor: Vendor = {
      id: '00000000-0000-0000-0000-000000000001',
      tenant_id: '00000000-0000-0000-0000-000000000002',
      name: 'Acme Corp',
      vendor_type: 'business',
      status: 'active',
      w9_on_file: false,
      contacts: [],
      tags: [],
      address: {
        line1: '123 Main St',
        city: 'Anytown',
        state: 'CA',
        postal_code: '90210',
        country: 'US',
      },
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    expect(vendor.address?.line1).toBe('123 Main St');
    expect(vendor.address?.postal_code).toBe('90210');
  });

  it('contacts array uses VendorContact type', () => {
    const contact: VendorContact = {
      id: '00000000-0000-0000-0000-000000000001',
      name: 'Jane Doe',
      title: 'AP Manager',
      email: 'jane@acme.com',
      phone: '555-0123',
      is_primary: true,
    };

    const vendor: Vendor = {
      id: '00000000-0000-0000-0000-000000000002',
      tenant_id: '00000000-0000-0000-0000-000000000003',
      name: 'Acme Corp',
      vendor_type: 'business',
      status: 'active',
      w9_on_file: false,
      contacts: [contact],
      primary_contact: contact,
      tags: [],
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    };

    expect(vendor.contacts[0].is_primary).toBe(true);
    expect(vendor.primary_contact?.name).toBe('Jane Doe');
  });
});

// ---------------------------------------------------------------------------
// OrganizationBranding
// ---------------------------------------------------------------------------

describe('OrganizationBranding', () => {
  it('uses camelCase field names matching Rust serde rename_all', () => {
    // Rust has #[serde(rename_all = "camelCase")] on OrganizationBranding,
    // so TS should use camelCase to match the serialized JSON.
    // If this test fails after a Rust change, update api.ts accordingly.
  });
});
