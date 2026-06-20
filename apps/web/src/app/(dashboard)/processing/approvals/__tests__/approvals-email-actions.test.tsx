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
    getApprovalLink: vi.fn(),
    resendApprovalEmail: vi.fn(),
  },
  dashboardApi: {
    getApprovalMetrics: vi.fn().mockResolvedValue({
      pending_approvals: 0,
      approved_today: 0,
      rejected_today: 0,
      avg_approval_time_hours: 0,
      approval_rate: 0,
      escalated: 0,
      overdue: 0,
    }),
  },
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock sonner
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
];

describe('ApprovalsPage email actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(workflowsApi.listPendingApprovals).mockResolvedValue(mockApprovals as any);
    vi.mocked(workflowsApi.approve).mockResolvedValue({} as any);
    vi.mocked(workflowsApi.reject).mockResolvedValue({} as any);
    vi.mocked(workflowsApi.getApprovalLink).mockResolvedValue({
      approve_url: 'https://example.com/api/v1/actions/approve?t=token-123',
      reject_url: 'https://example.com/api/v1/actions/reject?t=token-456',
      hold_url: 'https://example.com/api/v1/actions/hold?t=token-789',
      view_url: 'https://example.com/api/v1/actions/view?t=token-view',
      expires_at: '2026-06-06T10:00:00Z',
    } as any);
    vi.mocked(workflowsApi.resendApprovalEmail).mockResolvedValue({
      sent_to: 'approver@example.com',
      expires_at: '2026-06-06T10:00:00Z',
    } as any);
  });

  it('clicking Copy approval link calls the API and shows success toast', async () => {
    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    const copyBtn = screen.getByTitle('Copy approval link');
    await user.click(copyBtn);

    await waitFor(() => {
      expect(workflowsApi.getApprovalLink).toHaveBeenCalledWith('appr-1');
    });

    await waitFor(() => {
      expect(mockToast.success).toHaveBeenCalledWith('Approval link copied to clipboard');
    });
  });

  it('clicking Resend approval email calls the API and surfaces a success toast', async () => {
    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    const resendBtn = screen.getByTitle('Resend approval email');
    await user.click(resendBtn);

    await waitFor(() => {
      expect(workflowsApi.resendApprovalEmail).toHaveBeenCalledWith('appr-1');
    });

    await waitFor(() => {
      expect(mockToast.success).toHaveBeenCalledWith(
        'Approval email resent to approver@example.com',
      );
    });
  });

  it('shows error toast when copy link fails', async () => {
    vi.mocked(workflowsApi.getApprovalLink).mockRejectedValueOnce(
      new Error('Network error'),
    );

    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    await user.click(screen.getByTitle('Copy approval link'));

    await waitFor(() => {
      expect(mockToast.error).toHaveBeenCalledWith('Network error');
    });
  });

  it('shows error toast when resend email fails', async () => {
    vi.mocked(workflowsApi.resendApprovalEmail).mockRejectedValueOnce(
      new Error('Email service unavailable'),
    );

    const user = userEvent.setup();
    renderWithProviders(<ApprovalsPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice #INV-001')).toBeInTheDocument();
    });

    await user.click(screen.getByTitle('Resend approval email'));

    await waitFor(() => {
      expect(mockToast.error).toHaveBeenCalledWith('Email service unavailable');
    });
  });
});
