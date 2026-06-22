import { describe, expect, it } from 'vitest';
import type { components } from '../src/generated';
import type { CreateInvoiceInput } from '../src';

describe('CreateInvoiceInput', () => {
  it('is generated from the OpenAPI CreateInvoiceRequest schema', () => {
    const invoice: CreateInvoiceInput = {
      document_id: '11111111-1111-1111-1111-111111111111',
      vendor_name: 'Acme Corporation',
      invoice_number: 'INV-2024-0001',
      total_amount: { amount: 125000, currency: 'USD' },
      currency: 'USD',
      line_items: [
        {
          description: 'Professional services',
          amount: { amount: 125000, currency: 'USD' },
        },
      ],
      tags: [],
    };

    const generated: components['schemas']['CreateInvoiceRequest'] = invoice;
    expect(generated.document_id).toBe(invoice.document_id);
  });

  it('requires document_id from the backend schema', () => {
    // @ts-expect-error document_id is required by CreateInvoiceRequest.
    const missingDocumentId: CreateInvoiceInput = {
      vendor_name: 'Acme Corporation',
      invoice_number: 'INV-2024-0001',
      total_amount: { amount: 125000, currency: 'USD' },
      currency: 'USD',
      line_items: [],
      tags: [],
    };

    expect(missingDocumentId).toBeDefined();
  });
});
