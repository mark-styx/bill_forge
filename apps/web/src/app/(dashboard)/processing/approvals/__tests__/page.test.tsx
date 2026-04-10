import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import ApprovalsPage from '../page';
import { workflowsApi } from '@/lib/api';

// Mock the API module
vi.mock('@/lib/api', () => ({
  workflowsApi: {
    listPendingApprovals: vi.fn(),
    approve: vi.fn(),
    reject: vi.fn(),
  },
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock sonner - vi.hoisted to ensure availability during hoisted vi.mock
const { mockToast } = vi.hoisted(() => ({
  mockToast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));
vi.mock('sonner', () => ({
  toast: mockToast,
}));

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
    logger: {
      log: console.log,
      warn: console.warn,
      error: () => {},
    },
  });
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      {ui}
    </QueryClientProvider>,
  );
}

const mockApprovals = [
  {
    id: 'appr-1',
    invoice_id: 'inv-001',
    invoice_number: 'INV-001',
    vendor_name: 'Acme Corp',
    amount: 50000,
    currency: 'USD',
    created_at: '2026-04-09T10:00:00Z',
  },
  {
    id: 'appr-2',
    invoice_id: 'inv-002',
    invoice_number: 'INV-002',
    vendor_name: 'Globex Inc',
    amount: 75000,
    currency: 'USD',
    created_at: '2026-04-08T14:00:00Z',
  },
  {
    id: 'appr-3',
    invoice_id: 'inv-003',
    invoice_number: 'INV-003',
    vendor_name: 'Initech',
    amount: 120000,
    currency: 'USD',
    created_at: '2026-04-07T09:00:00Z',
  },
];

describe('ApprovalsPage bulk actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(workflowsApi.listPendingApprovals).mockResolvedValue(mockApprovals as any);
    vi.mocked(workflowsApi.approve).mockResolvedValue({} as any);
    vi.mocked(workflowsApi.reject).mockResolvedValue({} as any);
  });

  it('renders checkboxes for each row plus a master checkbox', async () => {
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    // Master checkbox + 3 row checkboxes = 4 total
    const checkboxes = screen.getAllByRole('checkbox');
    expect(checkboxes).toHaveLength(4);

    // Master checkbox has specific aria-label
    expect(screen.getByLabelText('Select all approvals')).toBeInTheDocument();

    // Row checkboxes
    expect(screen.getByLabelText('Select approval INV-001')).toBeInTheDocument();
    expect(screen.getByLabelText('Select approval INV-002')).toBeInTheDocument();
    expect(screen.getByLabelText('Select approval INV-003')).toBeInTheDocument();
  });

  it('select-all toggles all rows and shows selection count', async () => {
    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    // Click master checkbox to select all
    await user.click(screen.getByLabelText('Select all approvals'));

    // All row checkboxes should be checked
    const checkboxes = screen.getAllByRole('checkbox');
    expect(checkboxes.filter((cb) => (cb as HTMLInputElement).checked)).toHaveLength(4);

    // Selection count text
    expect(screen.getByText('3 of 3 selected')).toBeInTheDocument();

    // Click again to deselect all
    await user.click(screen.getByLabelText('Select all approvals'));
    expect(screen.getByText('Select all')).toBeInTheDocument();
  });

  it('bulk approve calls approve() for each selected id', async () => {
    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    // Select two rows individually
    await user.click(screen.getByLabelText('Select approval INV-001'));
    await user.click(screen.getByLabelText('Select approval INV-002'));

    // Click bulk approve
    await user.click(screen.getByText('Approve Selected'));

    await waitFor(() => {
      expect(workflowsApi.approve).toHaveBeenCalledTimes(2);
    });

    expect(workflowsApi.approve).toHaveBeenCalledWith('appr-1');
    expect(workflowsApi.approve).toHaveBeenCalledWith('appr-2');
    expect(workflowsApi.approve).not.toHaveBeenCalledWith('appr-3');
  });

  it('bulk reject calls reject() with reason for each selected id', async () => {
    const user = userEvent.setup();
    const promptSpy = vi.spyOn(window, 'prompt').mockReturnValue('test reason');
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    // Select all via master checkbox
    await user.click(screen.getByLabelText('Select all approvals'));

    // Click bulk reject
    await user.click(screen.getByText('Reject Selected'));

    await waitFor(() => {
      expect(workflowsApi.reject).toHaveBeenCalledTimes(3);
    });

    expect(workflowsApi.reject).toHaveBeenCalledWith('appr-1', 'test reason');
    expect(workflowsApi.reject).toHaveBeenCalledWith('appr-2', 'test reason');
    expect(workflowsApi.reject).toHaveBeenCalledWith('appr-3', 'test reason');

    promptSpy.mockRestore();
  });

  it('partial failure shows warning toast', async () => {
    const user = userEvent.setup();
    // Make the second approve call fail
    vi.mocked(workflowsApi.approve)
      .mockResolvedValueOnce({} as any)
      .mockRejectedValueOnce(new Error('Server error'))
      .mockResolvedValueOnce({} as any);

    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    // Select all
    await user.click(screen.getByLabelText('Select all approvals'));

    // Click bulk approve
    await user.click(screen.getByText('Approve Selected'));

    await waitFor(() => {
      expect(mockToast.warning).toHaveBeenCalledWith('2 succeeded, 1 failed');
    });
  });
});
