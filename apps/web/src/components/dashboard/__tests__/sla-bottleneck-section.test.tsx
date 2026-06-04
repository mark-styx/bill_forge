import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { SlaBottleneckSection } from '../sla-bottleneck-section';
import { reportsApi, dashboardApi } from '@/lib/api';

// Mock the API module with happy-path defaults
vi.mock('@/lib/api', () => ({
  reportsApi: {
    approvalSla: vi.fn().mockResolvedValue({
      pending_count: 3,
      near_breach_count: 1,
      breached_count: 1,
      items: [
        {
          invoice_id: 'inv-1',
          invoice_number: 'INV-001',
          vendor_name: 'Acme Corp',
          amount_cents: 50000,
          currency: 'USD',
          approval_id: 'apr-1',
          hours_waiting: 25.0,
          sla_hours: 24,
          deadline_at: new Date('2026-05-30T17:00:00Z').toISOString(),
          percent_elapsed: 104.2,
          sla_state: 'breached',
          approver_name: 'Alice',
          approver_label: 'Alice',
        },
        {
          invoice_id: 'inv-2',
          invoice_number: 'INV-002',
          vendor_name: 'Beta LLC',
          amount_cents: 30000,
          currency: 'USD',
          approval_id: 'apr-2',
          hours_waiting: 20.0,
          sla_hours: 24,
          deadline_at: new Date('2026-05-30T20:00:00Z').toISOString(),
          percent_elapsed: 83.3,
          sla_state: 'near_breach',
          approver_name: 'Bob',
          approver_label: 'Bob',
        },
        {
          invoice_id: 'inv-3',
          invoice_number: 'INV-003',
          vendor_name: 'Gamma Inc',
          amount_cents: 10000,
          currency: 'USD',
          approval_id: 'apr-3',
          hours_waiting: 5.0,
          sla_hours: 24,
          deadline_at: new Date('2026-05-31T10:00:00Z').toISOString(),
          percent_elapsed: 20.8,
          sla_state: 'within_sla',
          approver_name: 'Carol',
          approver_label: 'Carol',
        },
      ],
    }),
  },
  dashboardApi: {
    getStageDwell: vi.fn().mockResolvedValue([
      { stage: 'capture', median_minutes: 5.0, p90_minutes: 10.0, count: 100 },
      { stage: 'approval', median_minutes: 45.0, p90_minutes: 120.0, count: 50 },
      { stage: 'payment', median_minutes: 10.0, p90_minutes: 30.0, count: 80 },
    ]),
    getApproverWorkload: vi.fn().mockResolvedValue([
      {
        approver_id: 'user-1',
        approver_name: 'Alice',
        pending_count: 8,
        near_breach_count: 2,
        breached_count: 1,
        avg_response_hours: 3.5,
      },
      {
        approver_id: 'user-2',
        approver_name: 'Bob',
        pending_count: 5,
        near_breach_count: 0,
        breached_count: 0,
        avg_response_hours: 1.2,
      },
    ]),
    getExceptionTrend: vi.fn().mockResolvedValue([
      { date: '2026-05-29', total_invoices: 20, exception_count: 2, exception_rate: 10.0 },
      { date: '2026-05-30', total_invoices: 15, exception_count: 0, exception_rate: 0.0 },
    ]),
  },
}));

function renderWithProviders(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe('SlaBottleneckSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the SLA & Bottlenecks section header', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    expect(await screen.findByText('SLA & Bottlenecks')).toBeInTheDocument();
  });

  it('renders at-risk invoices sorted by percent_elapsed desc', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    // Wait for data to load
    const items = await screen.findAllByText(/INV-00/);
    // INV-001 (104.2%) should come before INV-002 (83.3%) before INV-003 (20.8%)
    expect(items[0]).toHaveTextContent('INV-001');
    expect(items[1]).toHaveTextContent('INV-002');
    expect(items[2]).toHaveTextContent('INV-003');
  });

  it('shows breached styling on breached invoice', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    // The breached invoice shows 104% with text-error class
    const pct = await screen.findByText('104%');
    expect(pct).toHaveClass('text-error');
  });

  it('renders stage dwell heat map with stage rows', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    expect(await screen.findByTestId('stage-row-capture')).toBeInTheDocument();
    expect(screen.getByTestId('stage-row-approval')).toBeInTheDocument();
    expect(screen.getByTestId('stage-row-payment')).toBeInTheDocument();
  });

  it('renders approver workload rows sorted by pending_count desc', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    const alice = await screen.findByTestId('approver-row-user-1');
    const bob = screen.getByTestId('approver-row-user-2');
    // Alice (8 pending) should appear before Bob (5 pending)
    expect(alice.compareDocumentPosition(bob) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(alice).toHaveTextContent('8 pending');
    expect(bob).toHaveTextContent('5 pending');
  });

  it('renders exception trend chart with data bars', async () => {
    renderWithProviders(<SlaBottleneckSection />);
    expect(await screen.findByTestId('trend-bar-2026-05-29')).toBeInTheDocument();
    expect(screen.getByTestId('trend-bar-2026-05-30')).toBeInTheDocument();
  });
});

describe('loading and error states', () => {
  const loadingTestIds = [
    'at-risk-loading',
    'stage-dwell-loading',
    'approver-workload-loading',
    'exception-trend-loading',
  ];

  const errorTestIds = [
    'at-risk-error',
    'stage-dwell-error',
    'approver-workload-error',
    'exception-trend-error',
  ];

  const emptyStateTexts = [
    'No invoices at risk',
    'No stage data yet',
    'No pending approvals',
    'No data yet',
  ];

  it('shows loading placeholders while queries are pending', async () => {
    // Override all mocks to never resolve so queries stay in loading state
    vi.mocked(reportsApi.approvalSla).mockImplementation(() => new Promise(() => {}));
    vi.mocked(dashboardApi.getStageDwell).mockImplementation(() => new Promise(() => {}));
    vi.mocked(dashboardApi.getApproverWorkload).mockImplementation(() => new Promise(() => {}));
    vi.mocked(dashboardApi.getExceptionTrend).mockImplementation(() => new Promise(() => {}));

    renderWithProviders(<SlaBottleneckSection />);

    // All loading test IDs should be present immediately
    for (const tid of loadingTestIds) {
      expect(screen.getByTestId(tid)).toBeInTheDocument();
    }

    // Empty-state text must NOT appear while loading
    for (const text of emptyStateTexts) {
      expect(screen.queryByText(text)).not.toBeInTheDocument();
    }
  });

  it('shows error messages when queries reject', async () => {
    vi.mocked(reportsApi.approvalSla).mockRejectedValue(new Error('fail'));
    vi.mocked(dashboardApi.getStageDwell).mockRejectedValue(new Error('fail'));
    vi.mocked(dashboardApi.getApproverWorkload).mockRejectedValue(new Error('fail'));
    vi.mocked(dashboardApi.getExceptionTrend).mockRejectedValue(new Error('fail'));

    renderWithProviders(<SlaBottleneckSection />);

    // All error test IDs should appear after rejection
    for (const tid of errorTestIds) {
      expect(await screen.findByTestId(tid)).toBeInTheDocument();
    }

    // Empty-state text must NOT appear on error
    for (const text of emptyStateTexts) {
      expect(screen.queryByText(text)).not.toBeInTheDocument();
    }
  });
});
