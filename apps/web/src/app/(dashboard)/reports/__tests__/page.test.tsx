import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import ReportsPage from '../page';

// Mock the API modules
vi.mock('@/lib/api', () => ({
  reportsApi: {
    workflowMetrics: vi.fn().mockResolvedValue({ avg_processing_time_hours: 4.2 }),
    invoiceAging: vi.fn().mockResolvedValue([]),
    invoicesByStatus: vi.fn().mockResolvedValue([]),
    spendTrends: vi.fn().mockResolvedValue([]),
    invoicesByVendor: vi.fn().mockResolvedValue([]),
    dashboardSummary: vi.fn().mockResolvedValue({
      vendors_active: 12,
      invoices_processed_today: 5,
      total_pending_amount: 50000,
      invoices_pending_review: 3,
    }),
    approvalSla: vi.fn().mockResolvedValue({ pending_count: 0, near_breach_count: 0, breached_count: 0, items: [] }),
    cashFlowObligations: vi.fn().mockResolvedValue([]),
  },
  predictiveApi: {
    getAnomalies: vi.fn(),
    getBudgetAlerts: vi.fn(),
    getAnomalyRules: vi.fn(),
    detectAnomalies: vi.fn(),
    dismissAlert: vi.fn(),
    acknowledgeAnomaly: vi.fn(),
  },
}));

// Mock charts module (avoids theme.ts localStorage access at module level)
vi.mock('@/components/ui/charts', () => ({
  ChartContainer: ({ children }: { children: React.ReactNode; title?: string }) => <div>{children}</div>,
  BillForgeAreaChart: () => <div />,
  BillForgeBarChart: () => <div />,
  BillForgeLineChart: () => <div />,
  BillForgeDonutChart: () => <div />,
  BillForgeProgressChart: () => <div />,
  BillForgeSparkline: () => <div />,
}));

// Mock the auth store
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    hasModule: () => true,
  })),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

import { predictiveApi, reportsApi } from '@/lib/api';

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

const mockAnomalies = [
  {
    id: 'anom-1',
    tenant_id: 'tenant-1',
    anomaly_type: 'duplicate_invoice',
    entity_id: 'vendor-1',
    entity_type: 'vendor' as const,
    severity: 'high' as const,
    detected_value: 50000,
    expected_range: [10000, 30000] as [number, number],
    deviation_score: 3.5,
    detected_at: '2026-05-27T10:00:00Z',
    metadata: {},
    acknowledged: false,
    acknowledged_at: null,
    acknowledged_by: null,
  },
  {
    id: 'anom-2',
    tenant_id: 'tenant-1',
    anomaly_type: 'invoice_amount_outlier',
    entity_id: 'vendor-2',
    entity_type: 'vendor' as const,
    severity: 'medium' as const,
    detected_value: 75000,
    expected_range: [20000, 60000] as [number, number],
    deviation_score: 2.1,
    detected_at: '2026-05-26T14:00:00Z',
    metadata: {},
    acknowledged: true,
    acknowledged_at: '2026-05-27T09:00:00Z',
    acknowledged_by: 'user-1',
  },
];

const mockAlerts = [
  {
    id: 'alert-1',
    alert_type: 'vendor_concentration',
    severity: 'high',
    entity_id: 'vendor-1',
    entity_type: 'vendor',
    title: 'High Vendor Concentration Risk',
    message: 'Vendor vendor-1 accounts for 65.0% of total spend (>50% threshold)',
    threshold_value: 50.0,
    current_value: 65.0,
    threshold_percentage: 65.0,
    recommended_action: 'Consider diversifying vendor base',
    triggered_at: '2026-05-27T12:00:00Z',
    dismissed: false,
  },
];

const mockRules = [
  {
    id: 'rule-1',
    entity_type: 'vendor',
    entity_id: null,
    anomaly_type: 'duplicate_invoice',
    zscore_threshold: 3.0,
    iqr_multiplier: 1.5,
    volume_spike_threshold: 2.0,
    notification_channels: ['email'],
    notify_on_severity: ['high', 'critical'],
    enabled: true,
  },
  {
    id: 'rule-2',
    entity_type: null,
    entity_id: null,
    anomaly_type: 'invoice_amount_outlier',
    zscore_threshold: 2.5,
    iqr_multiplier: null,
    volume_spike_threshold: null,
    notification_channels: null,
    notify_on_severity: null,
    enabled: false,
  },
];

describe('ReportsPage Predictive Insights', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(reportsApi.dashboardSummary).mockResolvedValue({
      vendors_active: 12,
      invoices_processed_today: 5,
      total_pending_amount: 50000,
      invoices_pending_review: 3,
    });
    vi.mocked(predictiveApi.getAnomalies).mockResolvedValue(mockAnomalies);
    vi.mocked(predictiveApi.getBudgetAlerts).mockResolvedValue(mockAlerts);
    vi.mocked(predictiveApi.getAnomalyRules).mockResolvedValue(mockRules);
  });

  it('renders the Predictive Insights section', async () => {
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Predictive Insights')).toBeInTheDocument();
    });

    expect(screen.getByText('Anomaly detection, budget alerts, and proactive recommendations')).toBeInTheDocument();
  });

  it('shows summary cards with anomaly and alert counts', async () => {
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Detected Anomalies')).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(screen.getByText('1 unacknowledged')).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(screen.getByText(/of 2 configured/)).toBeInTheDocument();
    });
    expect(screen.getByText('1 high severity')).toBeInTheDocument();
    expect(screen.getByText('reviewed by team')).toBeInTheDocument();
  });

  it('lists detected anomalies with severity badges', async () => {
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Duplicate Invoice')).toBeInTheDocument();
    });

    expect(screen.getByText('Invoice Amount Outlier')).toBeInTheDocument();
    expect(screen.getAllByText(/vendor-1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/vendor-2/).length).toBeGreaterThanOrEqual(1);
  });

  it('shows acknowledge button for unacknowledged anomalies', async () => {
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Acknowledge')).toBeInTheDocument();
    });

    // "Acknowledged" appears in the anomaly row (EyeOff icon text) and the summary card heading
    expect(screen.getAllByText(/Acknowledged/).length).toBeGreaterThanOrEqual(1);
  });

  it('calls acknowledgeAnomaly when acknowledge is clicked', async () => {
    vi.mocked(predictiveApi.acknowledgeAnomaly).mockResolvedValue(null);
    const user = userEvent.setup();
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Acknowledge')).toBeInTheDocument();
    });

    await user.click(screen.getByText('Acknowledge'));

    expect(predictiveApi.acknowledgeAnomaly).toHaveBeenCalledWith('anom-1');
  });

  it('lists budget alerts with title and dismiss button', async () => {
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('High Vendor Concentration Risk')).toBeInTheDocument();
    });

    expect(screen.getByText(/Vendor vendor-1 accounts for 65.0%/)).toBeInTheDocument();
    expect(screen.getByText('65.0%')).toBeInTheDocument();
    expect(screen.getByText('Dismiss')).toBeInTheDocument();
  });

  it('calls dismissAlert when dismiss is clicked', async () => {
    vi.mocked(predictiveApi.dismissAlert).mockResolvedValue(null);
    const user = userEvent.setup();
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Dismiss')).toBeInTheDocument();
    });

    await user.click(screen.getByText('Dismiss'));

    expect(predictiveApi.dismissAlert).toHaveBeenCalledWith('alert-1');
  });

  it('calls detectAnomalies when Run Detection is clicked', async () => {
    vi.mocked(predictiveApi.detectAnomalies).mockResolvedValue([]);
    const user = userEvent.setup();
    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Run Detection')).toBeInTheDocument();
    });

    await user.click(screen.getByText('Run Detection'));

    expect(predictiveApi.detectAnomalies).toHaveBeenCalled();
  });

  it('shows empty-state notices when no data is returned', async () => {
    vi.mocked(predictiveApi.getAnomalies).mockResolvedValue([]);
    vi.mocked(predictiveApi.getBudgetAlerts).mockResolvedValue([]);

    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('No anomalies detected')).toBeInTheDocument();
    });

    expect(screen.getByText('No active budget alerts')).toBeInTheDocument();
  });

  it('shows error notices when predictive endpoints fail', async () => {
    vi.mocked(predictiveApi.getAnomalies).mockRejectedValue(new Error('fail'));
    vi.mocked(predictiveApi.getBudgetAlerts).mockRejectedValue(new Error('fail'));

    renderWithProviders(<ReportsPage />);

    await waitFor(() => {
      expect(screen.getByText('Anomaly data unavailable')).toBeInTheDocument();
    });

    expect(screen.getByText('Budget alerts unavailable')).toBeInTheDocument();
  });
});
