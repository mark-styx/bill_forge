import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import DashboardPage from '../page';
import { reportsApi, dashboardApi } from '@/lib/api';

// Mock the API module
vi.mock('@/lib/api', () => ({
  reportsApi: {
    dashboardSummary: vi.fn(),
  },
  dashboardApi: {
    metrics: vi.fn().mockResolvedValue({
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
        approved_today: 0,
        rejected_today: 0,
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
    }),
  },
}));

// Mock the stores to provide required module data
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    hasModule: (module: string) =>
      ['invoice_capture', 'invoice_processing', 'vendor_management', 'reporting'].includes(module),
    user: { name: 'Test User', id: '1', tenant_id: 't1', email: 'test@test.com', roles: [] },
    tenant: {
      id: 't1',
      name: 'Test Org',
      enabled_modules: ['invoice_capture', 'invoice_processing', 'vendor_management', 'reporting'],
      settings: { company_name: 'Test' },
    },
  })),
}));

vi.mock('@/stores/theme', () => ({
  useThemeStore: vi.fn(() => ({
    getCurrentColors: () => ({
      primary: '220 90% 56%',
      accent: '280 70% 50%',
      capture: '180 70% 40%',
      processing: '30 90% 50%',
      vendor: '150 60% 40%',
      reporting: '260 60% 55%',
    }),
  })),
}));

vi.mock('@/components/organization-theme-provider', () => ({
  useOrganizationTheme: () => ({
    getBrandGradient: () => 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
  }),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock AnimatedCounter to just render the value
vi.mock('@/components/ui/animated-counter', () => ({
  AnimatedCounter: ({ value }: { value: number }) => <span>{value}</span>,
}));

// Mock glass-card components
vi.mock('@/components/ui/glass-card', () => ({
  GlassCard: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SpotlightCard: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

// Mock gradient-card components
vi.mock('@/components/ui/gradient-card', () => ({
  GradientButton: ({ children }: { children: React.ReactNode }) => <button>{children}</button>,
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
    </QueryClientProvider>
  );
}

const mockSummaryData = {
  invoices_pending_review: 5,
  invoices_pending_approval: 3,
  invoices_ready_for_payment: 2,
  total_amount_pending: 15000,
  vendors_active: 10,
  invoices_this_month: 25,
};

describe('DashboardPage error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows error banner and dashes when API fails, not zeros', async () => {
    vi.mocked(reportsApi.dashboardSummary).mockRejectedValueOnce(
      new Error('API error 500')
    );

    renderWithProviders(<DashboardPage />);

    // Wait for the error state to appear
    await waitFor(() => {
      expect(screen.getByText(/unable to load dashboard metrics/i)).toBeInTheDocument();
    });

    // Error detail should be visible
    expect(screen.getByText('API error 500')).toBeInTheDocument();

    // Retry button should be present
    expect(screen.getByText('Retry')).toBeInTheDocument();

    // Stat values should show '--' not '0'
    const dashElements = screen.getAllByText('--');
    expect(dashElements.length).toBeGreaterThanOrEqual(4); // 4 numeric stats in grid + module cards

    // The currency stat should show '$--'
    expect(screen.getByText('$--')).toBeInTheDocument();
  });

  it('recovers after clicking Retry when API succeeds on second call', async () => {
    const user = userEvent.setup();

    // Fail first, succeed second
    vi.mocked(reportsApi.dashboardSummary)
      .mockRejectedValueOnce(new Error('API error 500'))
      .mockResolvedValueOnce(mockSummaryData);

    renderWithProviders(<DashboardPage />);

    // Wait for error state
    await waitFor(() => {
      expect(screen.getByText(/unable to load dashboard metrics/i)).toBeInTheDocument();
    });

    // Click Retry
    await user.click(screen.getByText('Retry'));

    // Wait for recovery - stats should show real values (use getAllByText since values
    // appear in both the stats grid and the module overview cards)
    await waitFor(() => {
      const pendingReview = screen.getAllByText('5');
      expect(pendingReview.length).toBeGreaterThanOrEqual(1);
    });

    // Error banner should be gone
    expect(screen.queryByText(/unable to load dashboard metrics/i)).not.toBeInTheDocument();

    // Real values should be visible
    expect(screen.getAllByText('3').length).toBeGreaterThanOrEqual(1); // Awaiting Approval
    expect(screen.getAllByText('10').length).toBeGreaterThanOrEqual(1); // Active Vendors
  });

  it('renders rich metrics (avg processing time, approval rate, top vendor) when dashboardApi.metrics() succeeds', async () => {
    vi.mocked(reportsApi.dashboardSummary).mockResolvedValueOnce(mockSummaryData);
    vi.mocked(dashboardApi.metrics).mockResolvedValueOnce({
      invoices: {
        total_invoices: 120,
        pending_ocr: 5,
        ready_for_review: 12,
        submitted: 8,
        approved: 80,
        rejected: 10,
        paid: 60,
        avg_processing_time_hours: 4.5,
        total_value: 2500000,
        this_month: 35,
        trend_vs_last_month: 12.3,
      },
      approvals: {
        pending_approvals: 8,
        approved_today: 5,
        rejected_today: 1,
        avg_approval_time_hours: 3.2,
        approval_rate: 87.5,
        escalated: 2,
        overdue: 3,
      },
      vendors: {
        total_vendors: 42,
        new_this_month: 3,
        top_vendors: [
          { vendor_id: 'v-001', vendor_name: 'Acme Corp', invoice_count: 28, total_amount: 450000 },
          { vendor_id: 'v-002', vendor_name: 'Globex Inc', invoice_count: 15, total_amount: 225000 },
        ],
        concentration_percentage: 34.2,
      },
      team: {
        members: [],
        avg_approvals_per_member: 11.0,
        total_pending_actions: 7,
      },
    });

    renderWithProviders(<DashboardPage />);

    await waitFor(() => {
      expect(screen.getByText('4.5h')).toBeInTheDocument();
    });

    expect(screen.getByText('87.5%')).toBeInTheDocument();
    expect(screen.getByText('Acme Corp')).toBeInTheDocument();
  });
});
