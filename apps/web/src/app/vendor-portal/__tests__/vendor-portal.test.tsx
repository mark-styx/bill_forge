import { describe, it, expect, vi, beforeEach } from 'vitest';

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
let uploadInvoicePdfMock: ReturnType<typeof vi.fn>;
let listInvoiceMessagesMock: ReturnType<typeof vi.fn>;
let postInvoiceMessageMock: ReturnType<typeof vi.fn>;

vi.mock('@/lib/api', () => ({
  vendorPortalApi: {
    get submitInvoice() { return submitInvoiceMock; },
    get listInvoices() { return listInvoicesMock; },
    get uploadInvoicePdf() { return uploadInvoicePdfMock; },
    get listInvoiceMessages() { return listInvoiceMessagesMock; },
    get postInvoiceMessage() { return postInvoiceMessageMock; },
  },
}));


describe('Vendor Portal', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    submitInvoiceMock = vi.fn().mockResolvedValue({ id: 'inv-new', invoice_number: 'NEW-001' });
    listInvoicesMock = vi.fn().mockResolvedValue(mockInvoices);
    uploadInvoicePdfMock = vi.fn().mockResolvedValue({ id: 'inv-pdf-1', invoice_number: 'PDF-001' });
    listInvoiceMessagesMock = vi.fn().mockResolvedValue([]);
    postInvoiceMessageMock = vi.fn();
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

  it('vendorPortalApi.uploadInvoicePdf sends FormData with file and invoice_number', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    uploadInvoicePdfMock.mockResolvedValue({ id: 'inv-pdf-1', invoice_number: 'PDF-001' });

    const formData = new FormData();
    const file = new File(['%PDF-1.4 test'], 'invoice.pdf', { type: 'application/pdf' });
    formData.append('file', file);
    formData.append('invoice_number', 'PDF-001');

    const result = await vendorPortalApi.uploadInvoicePdf('test-token', formData);

    expect(uploadInvoicePdfMock).toHaveBeenCalledWith('test-token', formData);
    expect(result).toEqual({ id: 'inv-pdf-1', invoice_number: 'PDF-001' });
  });

  it('vendorPortalApi.listInvoiceMessages is callable with token and invoice id', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    const thread = [
      {
        id: 'msg-1',
        invoice_id: 'inv-1',
        sender_kind: 'ap_user' as const,
        sender_user_id: 'user-1',
        sender_vendor_contact_id: null,
        body: 'Can you confirm the PO number?',
        created_at: '2024-01-16T10:00:00Z',
      },
    ];
    listInvoiceMessagesMock.mockResolvedValue(thread);

    const result = await vendorPortalApi.listInvoiceMessages('test-token', 'inv-1');

    expect(listInvoiceMessagesMock).toHaveBeenCalledWith('test-token', 'inv-1');
    expect(result).toHaveLength(1);
    expect(result[0].sender_kind).toBe('ap_user');
  });

  it('vendorPortalApi.postInvoiceMessage posts body and returns saved message', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    postInvoiceMessageMock.mockResolvedValue({
      id: 'msg-2',
      invoice_id: 'inv-1',
      sender_kind: 'vendor' as const,
      sender_user_id: null,
      sender_vendor_contact_id: 'vendor-1',
      body: 'PO is 12345',
      created_at: '2024-01-16T11:00:00Z',
    });

    const result = await vendorPortalApi.postInvoiceMessage('test-token', 'inv-1', 'PO is 12345');

    expect(postInvoiceMessageMock).toHaveBeenCalledWith('test-token', 'inv-1', 'PO is 12345');
    expect(result.sender_kind).toBe('vendor');
    expect(result.body).toBe('PO is 12345');
  });
});
