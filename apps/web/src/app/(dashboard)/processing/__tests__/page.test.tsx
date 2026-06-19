import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import ProcessingPage from '../page';
import { workflowsApi, dashboardApi } from '@/lib/api';

vi.mock('@/hooks/useInvoiceEvents', () => ({
  useInvoiceEvents: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  workflowsApi: {
    listPendingApprovals: vi.fn(),
    listQueues: vi.fn(),
  },
  dashboardApi: {
    getMetrics: vi.fn(),
    getKpis: vi.fn(),
  },
}));

vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
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

describe('ProcessingPage stat tiles', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(workflowsApi.listPendingApprovals).mockResolvedValue([]);
    vi.mocked(workflowsApi.listQueues).mockResolvedValue([]);
    vi.mocked(dashboardApi.getMetrics).mockResolvedValue({
      invoices: {
        total_invoices: 0,
        pending_ocr: 0,
        ready_for_review: 0,
        submitted: 0,
        approved: 0,
        rejected: 0,
        paid: 0,
        avg_processing_time_hours: 0,
        total_value: 0,
        this_month: 0,
        trend_vs_last_month: 0,
      },
      approvals: {
        pending_approvals: 0,
        approved_today: 7,
        rejected_today: 3,
        avg_approval_time_hours: 0,
        approval_rate: 0,
        escalated: 0,
        overdue: 0,
      },
      vendors: {
        total_vendors: 0,
        new_this_month: 0,
        top_vendors: [],
        concentration_percentage: 0,
      },
      team: {
        members: [],
        avg_approvals_per_member: 0,
        total_pending_actions: 0,
      },
    });
    vi.mocked(dashboardApi.getKpis).mockResolvedValue({
      queue_count: 42,
      approved_count: 0,
      paid_count: 0,
      rejected_count: 0,
      aging: {
        aging_0_7: 0,
        aging_0_7_amount: 0,
        aging_8_14: 0,
        aging_8_14_amount: 0,
        aging_15_30: 0,
        aging_15_30_amount: 0,
        aging_30_plus: 0,
        aging_30_plus_amount: 0,
      },
      spend_by_vendor: [],
      total_spend_30d: 0,
      avg_processing_hours: 0,
    });
  });

  it('displays live API values for Approved Today, Rejected Today, and In Queues', async () => {
    renderWithProviders(<ProcessingPage />);

    await waitFor(() => {
      // Approved Today = 7, Rejected Today = 3, In Queues = 42
      expect(screen.getByText('7')).toBeInTheDocument();
      expect(screen.getByText('3')).toBeInTheDocument();
      expect(screen.getByText('42')).toBeInTheDocument();
    });

    // Verify the labels are present
    expect(screen.getByText('Approved Today')).toBeInTheDocument();
    expect(screen.getByText('Rejected Today')).toBeInTheDocument();
    expect(screen.getByText('In Queues')).toBeInTheDocument();
  });

  it('renders 0 for approval and queue tiles when API calls fail', async () => {
    vi.mocked(dashboardApi.getMetrics).mockRejectedValue(new Error('metrics unavailable'));
    vi.mocked(dashboardApi.getKpis).mockRejectedValue(new Error('kpis unavailable'));

    renderWithProviders(<ProcessingPage />);

    // Wait for the component to settle (pending approvals list resolves immediately with [])
    await waitFor(() => {
      expect(screen.getByText('All caught up!')).toBeInTheDocument();
    });

    // All stat tiles should show 0 (Pending Approval = 0 from empty approvals,
    // Approved Today = 0 from error fallback, Rejected Today = 0, In Queues = 0)
    const zeros = screen.getAllByText('0');
    expect(zeros.length).toBeGreaterThanOrEqual(3);
  });
});
