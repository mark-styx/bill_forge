import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import InboxPage from '../page';

// Mock next/navigation
const mockPush = vi.fn();
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush }),
  usePathname: () => '/processing/queues/inbox',
}));

// Mock the API
const mockCompleteQueueItem = vi.fn().mockResolvedValue({ data: { success: true } });
const mockClaimQueueItem = vi.fn().mockResolvedValue({ data: {} });
const mockListInboxItems = vi.fn();

vi.mock('@/lib/api', () => ({
  workflowsApi: {
    listInboxItems: (...args: unknown[]) => mockListInboxItems(...args),
    completeQueueItem: (...args: unknown[]) => mockCompleteQueueItem(...args),
    claimQueueItem: (...args: unknown[]) => mockClaimQueueItem(...args),
  },
}));

const mockItems = [
  {
    id: 'item-1',
    queue_id: 'queue-1',
    invoice_id: 'inv-1',
    assigned_to: 'user-1',
    priority: 10,
    entered_at: '2026-05-30T10:00:00Z',
    queue_name: 'Approval Queue',
    queue_type: 'approval',
    invoice_number: 'INV-001',
    vendor_name: 'Acme Corp',
    total_amount_cents: 50000,
    currency: 'USD',
    invoice_status: 'pending_approval',
  },
  {
    id: 'item-2',
    queue_id: 'queue-2',
    invoice_id: 'inv-2',
    assigned_to: 'user-1',
    priority: 5,
    entered_at: '2026-05-30T11:00:00Z',
    queue_name: 'Review Queue',
    queue_type: 'review',
    invoice_number: 'INV-002',
    vendor_name: 'Beta LLC',
    total_amount_cents: 25000,
    currency: 'USD',
    invoice_status: 'submitted',
  },
  {
    id: 'item-3',
    queue_id: 'queue-1',
    invoice_id: 'inv-3',
    assigned_to: 'user-1',
    priority: 5,
    entered_at: '2026-05-30T12:00:00Z',
    queue_name: 'Approval Queue',
    queue_type: 'approval',
    invoice_number: 'INV-003',
    vendor_name: 'Gamma Inc',
    total_amount_cents: 75000,
    currency: 'USD',
    invoice_status: 'pending_approval',
  },
];

function renderInboxPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <InboxPage />
    </QueryClientProvider>,
  );
}

function getVendorListButton(vendorName: string) {
  return screen.getAllByText(vendorName)[0].closest('button')!;
}

describe('InboxPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListInboxItems.mockResolvedValue({
      data: mockItems,
      pagination: { page: 1, per_page: 100, total_items: mockItems.length, total_pages: 1 },
    });
    mockCompleteQueueItem.mockResolvedValue({ data: { success: true } });
  });

  it('renders inbox items from API', async () => {
    renderInboxPage();

    expect(mockListInboxItems).toHaveBeenCalledWith({ per_page: 100 });

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
      expect(screen.getByText('Beta LLC')).toBeInTheDocument();
      expect(screen.getByText('Gamma Inc')).toBeInTheDocument();
    });
  });

  it('navigates items with j/k keyboard shortcuts', async () => {
    renderInboxPage();

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
    });

    // First item should be focused by default
    const acmeButton = getVendorListButton('Acme Corp');
    expect(acmeButton).toHaveClass('ring-2');

    // Press j to move to next
    fireEvent.keyDown(window, { key: 'j' });

    await waitFor(() => {
      const betaButton = getVendorListButton('Beta LLC');
      expect(betaButton).toHaveClass('ring-2');
    });

    // Press k to go back up
    fireEvent.keyDown(window, { key: 'k' });

    await waitFor(() => {
      expect(getVendorListButton('Acme Corp')).toHaveClass('ring-2');
    });
  });

  it('approves focused item with "a" key', async () => {
    renderInboxPage();

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
    });

    // First item focused by default (Acme Corp, item-1, queue-1)
    fireEvent.keyDown(window, { key: 'a' });

    await waitFor(() => {
      expect(mockCompleteQueueItem).toHaveBeenCalledWith('queue-1', 'item-1', 'approve');
    });
  });

  it('rejects focused item with "r" key', async () => {
    renderInboxPage();

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'r' });

    await waitFor(() => {
      expect(mockCompleteQueueItem).toHaveBeenCalledWith('queue-1', 'item-1', 'reject');
    });
  });

  it('claims focused item with "c" key', async () => {
    renderInboxPage();

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'c' });

    await waitFor(() => {
      expect(mockClaimQueueItem).toHaveBeenCalledWith('queue-1', 'item-1');
    });
  });

  it('navigates to invoice on Enter/o key', async () => {
    renderInboxPage();

    await waitFor(() => {
      expect(screen.getAllByText('Acme Corp')[0]).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'o' });

    expect(mockPush).toHaveBeenCalledWith('/processing/invoices/inv-1');
  });

  it('shows empty state when no items', async () => {
    mockListInboxItems.mockResolvedValue({
      data: [],
      pagination: { page: 1, per_page: 100, total_items: 0, total_pages: 0 },
    });

    renderInboxPage();

    await waitFor(() => {
      expect(screen.getByText('Nothing assigned to you right now.')).toBeInTheDocument();
    });
  });
});
