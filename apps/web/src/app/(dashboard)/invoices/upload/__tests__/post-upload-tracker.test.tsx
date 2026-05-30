import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import UploadInvoicePage from '../page';
import { invoicesApi } from '@/lib/api';
import type { Invoice } from '@billforge/shared-types';

// ---------------------------------------------------------------------------
// Shared mock state — tests mutate this before rendering
// ---------------------------------------------------------------------------

let mockQueryResultsByInvoiceId: Record<
  string,
  { data?: Invoice; error?: Error; isLoading?: boolean }
> = {};

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

vi.mock('@/lib/api', () => ({
  invoicesApi: {
    upload: vi.fn(),
    get: vi.fn(() => Promise.resolve(null)),
    submitForProcessing: vi.fn(() => Promise.resolve()),
  },
}));

const mockPush = vi.fn();
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush }),
}));

vi.mock('next/link', () => ({
  default: ({
    children,
    ...props
  }: {
    children: React.ReactNode;
    href: string;
  }) => <a {...props}>{children}</a>,
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

let capturedOnDrop: (files: File[]) => void;
vi.mock('react-dropzone', () => ({
  useDropzone: (opts: { onDrop: (files: File[]) => void }) => {
    capturedOnDrop = opts.onDrop;
    return {
      getRootProps: () => ({ className: 'border-dashed' }),
      getInputProps: () => ({ type: 'file' }),
      isDragActive: false,
      isDragReject: false,
    };
  },
}));

// Mock useQueries to read from the module-level mock state
vi.mock('@tanstack/react-query', () => ({
  useQueries: ({ queries }: { queries: Array<{ queryKey: string[] }> }) => {
    return queries.map((q) => {
      const id = q.queryKey[1];
      const result = mockQueryResultsByInvoiceId[id];
      if (!result) {
        return {
          data: undefined,
          error: undefined,
          isLoading: true,
          refetch: vi.fn(() => Promise.resolve({ data: undefined })),
        };
      }
      return {
        data: result.data,
        error: result.error,
        isLoading: result.isLoading ?? false,
        refetch: vi.fn(async () => {
          const updated = mockQueryResultsByInvoiceId[id];
          return { data: updated?.data };
        }),
      };
    });
  },
}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createFile(name: string, type: string, size = 1024): File {
  const file = new File(['x'.repeat(size)], name, { type });
  Object.defineProperty(file, 'size', { value: size });
  return file;
}

function dropFiles(files: File[]) {
  capturedOnDrop(files);
}

function makeInvoice(overrides: Partial<Invoice> & { id: string }): Invoice {
  return {
    tenant_id: 't1',
    vendor_name: 'Test Vendor',
    invoice_number: 'INV-001',
    total_amount: { amount: 100, currency: 'USD' },
    currency: 'USD',
    line_items: [],
    capture_status: 'pending',
    processing_status: 'pending',
    document_id: 'doc-1',
    supporting_documents: [],
    tags: [],
    created_by: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('PostUploadTracker', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockQueryResultsByInvoiceId = {};
  });

  it('after single-file upload, does NOT redirect and renders "Extracting data…" row', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload).mockResolvedValueOnce({
      invoice_id: 'inv-1',
      document_id: 'd1',
      message: 'ok',
    });

    // Invoice still loading — useQueries returns no data
    mockQueryResultsByInvoiceId['inv-1'] = { isLoading: true };

    render(<UploadInvoicePage />);

    dropFiles([createFile('invoice.pdf', 'application/pdf')]);

    await waitFor(() => {
      expect(screen.getByText('invoice.pdf')).toBeInTheDocument();
    });

    await user.click(screen.getByText(/Upload & Process/));

    // Tracker should render with loading spinner
    await waitFor(() => {
      expect(screen.getByText('Extracting data…')).toBeInTheDocument();
    });

    // Dead-end redirect is removed
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('shows "Submit to Approval Queue" when OCR resolves extracted + high confidence', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload).mockResolvedValueOnce({
      invoice_id: 'inv-2',
      document_id: 'd2',
      message: 'ok',
    });

    // Pre-resolve the query data so PostUploadTracker sees it immediately
    mockQueryResultsByInvoiceId['inv-2'] = {
      data: makeInvoice({
        id: 'inv-2',
        capture_status: 'extracted',
        ocr_confidence: 0.95,
      }),
    };

    render(<UploadInvoicePage />);

    dropFiles([createFile('invoice.pdf', 'application/pdf')]);

    await waitFor(() => {
      expect(screen.getByText('invoice.pdf')).toBeInTheDocument();
    });

    await user.click(screen.getByText(/Upload & Process/));

    // Should show the submit CTA (not "Extracting data…")
    await waitFor(() => {
      expect(
        screen.getByText('Submit to Approval Queue')
      ).toBeInTheDocument();
    });

    // Click submit
    await user.click(screen.getByText('Submit to Approval Queue'));

    expect(invoicesApi.submitForProcessing).toHaveBeenCalledWith('inv-2');
  });

  it('shows "Go to Approval Queue" link after submit returns current_queue_id', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload).mockResolvedValueOnce({
      invoice_id: 'inv-3',
      document_id: 'd3',
      message: 'ok',
    });

    // Invoice already submitted and has a queue assignment
    mockQueryResultsByInvoiceId['inv-3'] = {
      data: makeInvoice({
        id: 'inv-3',
        capture_status: 'extracted',
        ocr_confidence: 0.95,
        current_queue_id: 'queue-42',
      }),
    };

    render(<UploadInvoicePage />);

    dropFiles([createFile('invoice.pdf', 'application/pdf')]);

    await waitFor(() => {
      expect(screen.getByText('invoice.pdf')).toBeInTheDocument();
    });

    await user.click(screen.getByText(/Upload & Process/));

    await waitFor(() => {
      expect(screen.getByText('Go to Approval Queue')).toBeInTheDocument();
    });

    const link = screen.getByText('Go to Approval Queue').closest('a');
    expect(link?.getAttribute('href')).toBe('/processing/queues/queue-42');
  });

  it('shows "View all uploaded invoices" link after upload', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload).mockResolvedValueOnce({
      invoice_id: 'inv-4',
      document_id: 'd4',
      message: 'ok',
    });

    mockQueryResultsByInvoiceId['inv-4'] = { isLoading: true };

    render(<UploadInvoicePage />);

    dropFiles([createFile('test.pdf', 'application/pdf')]);

    await waitFor(() => {
      expect(screen.getByText('test.pdf')).toBeInTheDocument();
    });

    await user.click(screen.getByText(/Upload & Process/));

    await waitFor(() => {
      expect(
        screen.getByText('View all uploaded invoices')
      ).toBeInTheDocument();
    });

    const link = screen.getByText('View all uploaded invoices').closest('a');
    expect(link?.getAttribute('href')).toBe('/invoices');
  });
});
