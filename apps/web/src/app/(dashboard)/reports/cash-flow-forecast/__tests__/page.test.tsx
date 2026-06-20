import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import CashFlowForecastPage from '../page';
import type { ApCashFlowForecast, ApCashFlowSimulation } from '@/lib/api';

// Mock the API modules
vi.mock('@/lib/api', () => ({
  reportsApi: {
    apCashFlowForecast: vi.fn(),
    simulateApCashFlowForecast: vi.fn(),
    exportApCashFlowForecast: vi.fn(),
  },
}));

// Mock sonner toast (used by export handlers)
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock charts module
vi.mock('@/components/ui/charts', () => ({
  ChartContainer: ({ children, title }: { children: React.ReactNode; title?: string }) => (
    <div data-testid="chart-container" data-title={title}>
      {children}
    </div>
  ),
  BillForgeBarChart: ({ data, dataKey }: { data: unknown[]; dataKey: string[] }) => (
    <div data-testid="bar-chart" data-bar-count={data.length} data-keys={dataKey.join(',')} />
  ),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock lucide-react icons
vi.mock('lucide-react', () => ({
  DollarSign: () => <span>DollarSign</span>,
  ArrowRight: () => <span>ArrowRight</span>,
  AlertTriangle: () => <span>AlertTriangle</span>,
  Calendar: () => <span>Calendar</span>,
  TrendingUp: () => <span>TrendingUp</span>,
  ArrowLeft: () => <span>ArrowLeft</span>,
  FlaskConical: () => <span>FlaskConical</span>,
  Target: () => <span>Target</span>,
  CheckCircle2: () => <span>CheckCircle2</span>,
  XCircle: () => <span>XCircle</span>,
  Download: () => <span>Download</span>,
}));

import { reportsApi } from '@/lib/api';

const mockForecast: ApCashFlowForecast = {
  as_of_date: '2026-05-30',
  horizon_weeks: 13,
  daily: Array.from({ length: 91 }, (_, i) => ({
    date: `2026-05-${String(31 + (i % 30)).padStart(2, '0')}`,
    expected_amount: i === 5 ? 500_00_00 : i === 10 ? 300_00_00 : 10_00_00,
    low_band: i === 5 ? 425_00_00 : 7_00_00,
    high_band: i === 5 ? 575_00_00 : 13_00_00,
    vendor_breakdown: [
      { name: 'Vendor A', amount_cents: i === 5 ? 400_00_00 : 6_00_00 },
      { name: 'Vendor B', amount_cents: i === 5 ? 100_00_00 : 4_00_00 },
    ],
    gl_breakdown: [
      { name: '5000 - Supplies', amount_cents: i === 5 ? 400_00_00 : 6_00_00 },
      { name: '6000 - Services', amount_cents: i === 5 ? 100_00_00 : 4_00_00 },
    ],
    funding_required: i === 5 || i === 10,
  })),
  weekly: Array.from({ length: 13 }, (_, i) => ({
    week_start: `2026-05-${String(31 + i * 7).padStart(2, '0')}`,
    week_end: `2026-06-${String(6 + i * 7).padStart(2, '0')}`,
    expected_amount: i === 0 ? 580_00_00 : 70_00_00,
    low_band: i === 0 ? 500_00_00 : 49_00_00,
    high_band: i === 0 ? 660_00_00 : 91_00_00,
  })),
  monthly: [
    { month: '2026-05', expected_amount: 800_00_00, low_band: 600_00_00, high_band: 1000_00_00 },
    { month: '2026-06', expected_amount: 3000_00_00, low_band: 2500_00_00, high_band: 3500_00_00 },
    { month: '2026-07', expected_amount: 2500_00_00, low_band: 2000_00_00, high_band: 3000_00_00 },
  ],
};

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
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

describe('CashFlowForecastPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the 13-week forecast page with KPI cards', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('13-Week Cash Flow Forecast')).toBeInTheDocument();
    });

    // Check KPI card labels are present
    expect(screen.getByText('Total expected outflow')).toBeInTheDocument();
    expect(screen.getByText('Peak week')).toBeInTheDocument();
    expect(screen.getByText('Funding alert days')).toBeInTheDocument();
  });

  it('renders the weekly bar chart with 13 bars', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      const chart = screen.getByTestId('bar-chart');
      expect(chart).toBeInTheDocument();
      expect(chart.dataset.barCount).toBe('13');
    });
  });

  it('shows funding-required alert rows when days exceed threshold', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('Daily Funding Required')).toBeInTheDocument();
    });

    // The funding alert section should exist because our mock has funding_required days
    expect(screen.getByText('Days where expected outflow exceeds threshold')).toBeInTheDocument();
  });

  it('shows breakdown tabs for vendor and GL', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('By Vendor')).toBeInTheDocument();
      expect(screen.getByText('By GL Code')).toBeInTheDocument();
    });
  });

  it('displays error state when forecast fails', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockRejectedValue(new Error('Network error'));

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('Forecast unavailable')).toBeInTheDocument();
    });
  });

  it('runs simulation and shows scenario overlay with KPI deltas', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);

    const mockSimulation: ApCashFlowSimulation = {
      baseline: mockForecast,
      scenario: {
        ...mockForecast,
        weekly: mockForecast.weekly.map((w, i) => ({
          ...w,
          expected_amount: i === 0 ? 500_00_00 : w.expected_amount,
        })),
        daily: mockForecast.daily.map((d, i) => ({
          ...d,
          expected_amount: i === 5 ? 400_00_00 : d.expected_amount,
        })),
      },
      scenario_inputs: {
        pending_approval_delay_days: 7,
        capture_all_epd: true,
        vendor_term_shift_days: 0,
      },
    };
    vi.mocked(reportsApi.simulateApCashFlowForecast).mockResolvedValue(mockSimulation);

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('What-If Simulator')).toBeInTheDocument();
    });

    // Fill delay input
    const delayInput = screen.getByLabelText('Delay pending approvals by (days)');
    fireEvent.change(delayInput, { target: { value: '7' } });

    // Check the simulator's EPD checkbox (the Find Cash panel also has one)
    const epdCheckbox = screen.getByLabelText('Capture every early-payment discount');
    fireEvent.click(epdCheckbox);

    // Click Run simulation
    const runButton = screen.getByText('Run simulation');
    fireEvent.click(runButton);

    // Assert the simulate API was called
    await waitFor(() => {
      expect(reportsApi.simulateApCashFlowForecast).toHaveBeenCalledWith(
        expect.objectContaining({
          scenario: expect.objectContaining({
            pending_approval_delay_days: 7,
            capture_all_epd: true,
          }),
        }),
      );
    });

    // Assert scenario KPI deltas rendered
    await waitFor(() => {
      expect(screen.getByText('Total expected (baseline vs scenario)')).toBeInTheDocument();
      expect(screen.getByText('Funding-alert days (baseline vs scenario)')).toBeInTheDocument();
      expect(screen.getByText('EPD discount captured')).toBeInTheDocument();
    });

    // Assert chart now uses both data keys
    const chart = screen.getByTestId('bar-chart');
    expect(chart.dataset.keys).toContain('expected');
    expect(chart.dataset.keys).toContain('scenario');
  });

  it('invokes the export endpoint when the CSV menu action is clicked', async () => {
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(mockForecast);
    vi.mocked(reportsApi.exportApCashFlowForecast).mockResolvedValue({
      kind: 'csv',
      blob: new Blob(['week_start,week_end\n'], { type: 'text/csv' }),
    });

    // jsdom doesn't implement these by default; stub them so the download path
    // doesn't blow up while we assert on the API call.
    const createObjectURL = vi.fn(() => 'blob:mock');
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      value: createObjectURL,
      configurable: true,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      value: revokeObjectURL,
      configurable: true,
    });

    renderWithProviders(<CashFlowForecastPage />);

    await waitFor(() => {
      expect(screen.getByText('13-Week Cash Flow Forecast')).toBeInTheDocument();
    });

    // Open the export menu, then click "Download CSV".
    fireEvent.click(screen.getByRole('button', { name: /Export/i }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Download CSV/i }));

    await waitFor(() => {
      expect(reportsApi.exportApCashFlowForecast).toHaveBeenCalledWith(
        expect.objectContaining({
          format: 'csv',
          horizon_weeks: 13,
        }),
      );
    });
  });
});
