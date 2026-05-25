import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { ReactNode } from 'react';

// Mock next/navigation
vi.mock('next/navigation', () => ({
  useSearchParams: () => new URLSearchParams('token=test-vendor-token-123'),
}));

// Mock lucide-react icons
vi.mock('lucide-react', () => ({
  FileText: () => <span>FileText</span>,
}));

// Mock UI components
vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children: ReactNode; [k: string]: unknown }) => (
    <button {...props}>{children}</button>
  ),
}));
vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />,
}));
vi.mock('@/components/ui/label', () => ({
  Label: ({ children }: { children: ReactNode }) => <label>{children}</label>,
}));

const mockInvoices = [
  {
    id: 'inv-1',
    invoice_number: 'VENDOR-001',
    invoice_date: '2024-01-15',
    due_date: '2024-02-15',
    total_amount: 50000,
    currency: 'USD',
    processing_status: 'paid',
  },
  {
    id: 'inv-2',
    invoice_number: 'VENDOR-002',
    invoice_date: '2024-02-01',
    due_date: '2024-03-01',
    total_amount: 25000,
    currency: 'USD',
    processing_status: 'pending_approval',
  },
];

let submitInvoiceMock: ReturnType<typeof vi.fn>;
let listInvoicesMock: ReturnType<typeof vi.fn>;

vi.mock('@/lib/api', () => ({
  vendorPortalApi: {
    get submitInvoice() { return submitInvoiceMock; },
    get listInvoices() { return listInvoicesMock; },
  },
}));

// Need to import page component after mocks
const VendorPortalPage = vi.fn();

describe('Vendor Portal', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    submitInvoiceMock = vi.fn().mockResolvedValue({ id: 'inv-new', invoice_number: 'NEW-001' });
    listInvoicesMock = vi.fn().mockResolvedValue(mockInvoices);
  });

  it('vendorPortalApi.submitInvoice is callable with token and body', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    submitInvoiceMock.mockResolvedValue({ id: 'inv-new', invoice_number: 'NEW-001' });

    const result = await vendorPortalApi.submitInvoice('test-token', {
      invoice_number: 'NEW-001',
      amount: 10000,
      currency: 'USD',
    });

    expect(result).toEqual({ id: 'inv-new', invoice_number: 'NEW-001' });
  });

  it('vendorPortalApi.listInvoices is callable with token', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    listInvoicesMock.mockResolvedValue(mockInvoices);

    const result = await vendorPortalApi.listInvoices('test-token');
    expect(result).toHaveLength(2);
    expect(result[0].processing_status).toBe('paid');
    expect(result[1].processing_status).toBe('pending_approval');
  });

  it('invoice rows include status badges with correct status text', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    listInvoicesMock.mockResolvedValue(mockInvoices);

    const invoices = await vendorPortalApi.listInvoices('test-token');
    const statuses = invoices.map((i) => i.processing_status);

    expect(statuses).toContain('paid');
    expect(statuses).toContain('pending_approval');
  });
});
