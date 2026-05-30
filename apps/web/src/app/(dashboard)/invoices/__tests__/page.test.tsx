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

vi.mock('@/lib/api', () => ({
  invoicesApi: {
    list: (...args: unknown[]) => mockListInvoices(...args),
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

    // Bulk action buttons should be visible
    expect(screen.getByText('Bulk Approve')).toBeInTheDocument();
    expect(screen.getByText('Bulk Export')).toBeInTheDocument();
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
});
