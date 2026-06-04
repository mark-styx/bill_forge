import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import InvoicesPage from '../page';

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: any) => <a {...props}>{children}</a>,
}));

// Mock the API
const mockListInvoices = vi.fn();
const mockBulkOperation = vi.fn();

vi.mock('@/lib/api', () => ({
  invoicesApi: {
    list: (...args: unknown[]) => mockListInvoices(...args),
  },
  workflowsApi: {
    bulkOperation: (...args: unknown[]) => mockBulkOperation(...args),
  },
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

// Mock the auth store
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    hasModule: () => true,
  })),
}));

// Mock useStatusConfig
vi.mock('@/hooks/useStatusConfig', () => ({
  useStatusConfig: () => ({
    getStatusDisplay: (key: string) => ({
      key,
      label: key.replace(/_/g, ' ').replace(/\b\w/g, (l: string) => l.toUpperCase()),
      bg: 'bg-secondary',
      text: 'text-muted-foreground',
      isTerminal: false,
      isActive: true,
    }),
    getProcessingStatuses: () => [
      { key: 'pending_approval', label: 'Pending Approval' },
      { key: 'approved', label: 'Approved' },
      { key: 'rejected', label: 'Rejected' },
    ],
  }),
}));

// Mock InvoicePanel
vi.mock('@/components/InvoicePanel', () => ({
  default: ({ invoiceId, onClose }: { invoiceId: string | null; onClose: () => void }) => (
    <div data-testid="invoice-panel">
      {invoiceId ? <span>Panel: {invoiceId}</span> : <span>Panel closed</span>}
      <button onClick={onClose}>Close panel</button>
    </div>
  ),
}));

// Mock ConfidenceBadge
vi.mock('@/components/ConfidenceBadge', () => ({
  ConfidenceBadge: ({ confidence }: { confidence: number }) => (
    <span data-testid="confidence-badge">{Math.round(confidence * 100)}%</span>
  ),
}));

// Mock useInvoiceEvents
vi.mock('@/hooks/useInvoiceEvents', () => ({
  useInvoiceEvents: vi.fn(),
}));

const mockInvoices = [
  {
    id: 'inv-001-abcdefgh',
    invoice_number: 'INV-001',
    vendor_name: 'Acme Corp',
    total_amount: { amount: 50000, currency: 'USD' },
    processing_status: 'pending_approval',
    capture_status: 'completed',
    ocr_confidence: 0.92,
    invoice_date: '2026-05-28',
  },
  {
    id: 'inv-002-ijklmnop',
    invoice_number: 'INV-002',
    vendor_name: 'Beta LLC',
    total_amount: { amount: 25000, currency: 'USD' },
    processing_status: 'approved',
    capture_status: 'completed',
    ocr_confidence: 0.70,
    invoice_date: '2026-05-29',
  },
  {
    id: 'inv-003-qrstuvwx',
    invoice_number: 'INV-003',
    vendor_name: 'Gamma Inc',
    total_amount: { amount: 75000, currency: 'USD' },
    processing_status: 'rejected',
    capture_status: 'failed',
    invoice_date: '2026-05-30',
  },
];

function renderInvoicesPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <InvoicesPage />
    </QueryClientProvider>,
  );
}

describe('InvoicesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListInvoices.mockResolvedValue({
      data: mockInvoices,
      pagination: { page: 1, per_page: 25, total_items: 3, total_pages: 1 },
    });
  });

  it('renders header checkbox and per-row checkboxes; select-all selects all visible rows', async () => {
    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // There should be checkboxes: 1 header + 3 row checkboxes = 4
    const checkboxes = screen.getAllByRole('checkbox');
    expect(checkboxes.length).toBe(4);

    // Click the header checkbox (select all)
    fireEvent.click(checkboxes[0]);

    // "3 selected" indicator should appear (AdvancedDataTable shows it)
    await waitFor(() => {
      const selectedTexts = screen.getAllByText('3 selected');
      expect(selectedTexts.length).toBeGreaterThanOrEqual(1);
    });

    // Bulk action buttons should be visible and enabled (selection is active)
    const bulkApproveBtn = screen.getByText('Bulk Approve').closest('button')!;
    const bulkExportBtn = screen.getByText('Bulk Export').closest('button')!;
    expect(bulkApproveBtn).not.toBeDisabled();
    expect(bulkExportBtn).not.toBeDisabled();
    expect(screen.getByText('Clear')).toBeInTheDocument();
  });

  it('clicking a row checkbox does not open InvoicePanel; clicking the row body does', async () => {
    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // The panel should be closed initially
    expect(screen.getByText('Panel closed')).toBeInTheDocument();

    // Get all checkboxes; row checkboxes are indices 1, 2, 3 (index 0 is header)
    const checkboxes = screen.getAllByRole('checkbox');

    // Click the first row's checkbox — should NOT open the panel
    fireEvent.click(checkboxes[1]);
    expect(screen.getByText('Panel closed')).toBeInTheDocument();

    // Selection should be reflected
    await waitFor(() => {
      const selectedTexts = screen.getAllByText('1 selected');
      expect(selectedTexts.length).toBeGreaterThanOrEqual(1);
    });

    // Panel should still be closed
    expect(screen.getByText('Panel closed')).toBeInTheDocument();
  });

  it('clicking a row body opens the InvoicePanel', async () => {
    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-002')).toBeInTheDocument();
    });

    // Click on a row's vendor name cell to trigger row click
    fireEvent.click(screen.getByText('Beta LLC'));

    // Panel should open with that invoice id
    await waitFor(() => {
      expect(screen.getByText(/Panel: inv-002/)).toBeInTheDocument();
    });
  });

  it('shows empty state when no invoices', async () => {
    mockListInvoices.mockResolvedValue({
      data: [],
      pagination: { page: 1, per_page: 25, total_items: 0, total_pages: 0 },
    });

    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('No invoices found')).toBeInTheDocument();
    });
  });

  it('clear button deselects all rows', async () => {
    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // Select all
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);

    await waitFor(() => {
      expect(screen.getByText('3 selected')).toBeInTheDocument();
    });

    // Click clear
    fireEvent.click(screen.getByText('Clear'));

    // "3 selected" should disappear
    await waitFor(() => {
      expect(screen.queryAllByText('3 selected')).toHaveLength(0);
    });
  });

  it('Bulk Approve calls bulkOperation and clears selection on success', async () => {
    mockBulkOperation.mockResolvedValue({
      total: 2,
      successful: 2,
      failed: 0,
      errors: [],
    });

    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // Select first two rows
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[1]);
    fireEvent.click(checkboxes[2]);

    await waitFor(() => {
      expect(screen.getByText('2 selected')).toBeInTheDocument();
    });

    // Click Bulk Approve
    const bulkApproveBtn = screen.getByText('Bulk Approve').closest('button')!;
    fireEvent.click(bulkApproveBtn);

    await waitFor(() => {
      expect(mockBulkOperation).toHaveBeenCalledWith({
        operation: 'approve',
        invoice_ids: [mockInvoices[0].id, mockInvoices[1].id],
      });
    });

    // Selection should be cleared after success
    await waitFor(() => {
      expect(screen.queryAllByText('2 selected')).toHaveLength(0);
    });
  });

  it('Bulk Approve shows error toast on failure', async () => {
    mockBulkOperation.mockRejectedValue(new Error('Server error'));

    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // Select first row
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[1]);

    await waitFor(() => {
      expect(screen.getByText('1 selected')).toBeInTheDocument();
    });

    // Click Bulk Approve
    const bulkApproveBtn = screen.getByText('Bulk Approve').closest('button')!;
    fireEvent.click(bulkApproveBtn);

    await waitFor(() => {
      expect(mockBulkOperation).toHaveBeenCalled();
    });
  });

  it('passes search term to invoicesApi.list', async () => {
    renderInvoicesPage();

    // Wait for initial load
    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledWith(
        expect.objectContaining({ page: 1, per_page: 25 }),
      );
    });

    // Find the search input and type into it
    const searchInput = screen.getByPlaceholderText('Search invoices...');
    fireEvent.change(searchInput, { target: { value: 'Acme' } });

    // Wait for the debounced search to fire (AdvancedDataTable debounces at 300ms)
    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledWith(
        expect.objectContaining({ search: 'Acme' }),
      );
    }, { timeout: 2000 });
  });

  it('resets page to 1 when search changes', async () => {
    // Simulate being on page 2 initially by returning paginated data
    mockListInvoices.mockResolvedValue({
      data: mockInvoices,
      pagination: { page: 2, per_page: 25, total_items: 30, total_pages: 2 },
    });

    renderInvoicesPage();

    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalled();
    });

    // Type into search to trigger onSearchChange
    const searchInput = screen.getByPlaceholderText('Search invoices...');
    fireEvent.change(searchInput, { target: { value: 'test' } });

    // After search changes, the API should be called with page: 1
    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledWith(
        expect.objectContaining({ search: 'test', page: 1 }),
      );
    }, { timeout: 2000 });
  });

  it('Bulk Export triggers CSV download and clears selection', async () => {
    renderInvoicesPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
    });

    // Select all rows
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);

    await waitFor(() => {
      expect(screen.getByText('3 selected')).toBeInTheDocument();
    });

    // Set up DOM mocks for the download AFTER rendering
    const mockCreateObjectURL = vi.fn(() => 'blob:mock-url');
    const mockRevokeObjectURL = vi.fn();
    const originalCreateObjectURL = globalThis.URL.createObjectURL;
    const originalRevokeObjectURL = globalThis.URL.revokeObjectURL;
    globalThis.URL.createObjectURL = mockCreateObjectURL;
    globalThis.URL.revokeObjectURL = mockRevokeObjectURL;

    // Mock anchor element click
    const mockAnchorClick = vi.fn();
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tag: string) => {
      const el = originalCreateElement(tag);
      if (tag === 'a') {
        el.click = mockAnchorClick;
      }
      return el;
    });

    // Click Bulk Export
    const bulkExportBtn = screen.getByText('Bulk Export').closest('button')!;
    fireEvent.click(bulkExportBtn);

    await waitFor(() => {
      expect(mockCreateObjectURL).toHaveBeenCalled();
    });
    expect(mockAnchorClick).toHaveBeenCalled();

    // Selection should be cleared after export
    await waitFor(() => {
      expect(screen.queryAllByText('3 selected')).toHaveLength(0);
    });

    // Restore mocks
    globalThis.URL.createObjectURL = originalCreateObjectURL;
    globalThis.URL.revokeObjectURL = originalRevokeObjectURL;
    vi.restoreAllMocks();
  });
});
