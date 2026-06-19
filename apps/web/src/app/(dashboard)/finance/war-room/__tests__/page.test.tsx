import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import WarRoomPage from '../page';
import type { ApCashFlowForecast, ApCashFlowSimulation } from '@/lib/api';

// Mock next/navigation so router.push can be observed.
const pushMock = vi.fn();
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: pushMock }),
  usePathname: () => '/finance/war-room',
}));

// Mock the API modules.
vi.mock('@/lib/api', () => ({
  reportsApi: {
    apCashFlowForecast: vi.fn(),
    simulateApCashFlowForecast: vi.fn(),
  },
}));

// Mock charts module.
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

// Mock next/link.
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock lucide-react icons. Each renders to nothing so button text is the
// only visible label.
vi.mock('lucide-react', () => ({
  ArrowLeft: () => null,
  Radar: () => null,
  RotateCcw: () => null,
  DollarSign: () => null,
  AlertTriangle: () => null,
  Percent: () => null,
  Clock: () => null,
  Sparkles: () => null,
}));

import { reportsApi } from '@/lib/api';

// 13-week baseline with a single funding-alert day.
function makeBaseline(): ApCashFlowForecast {
  const daily = Array.from({ length: 91 }, (_, i) => ({
    date: `2026-06-${String((i % 28) + 1).padStart(2, '0')}`,
    expected_amount: i === 5 ? 500_00_00 : 10_00_00,
    low_band: i === 5 ? 425_00_00 : 7_00_00,
    high_band: i === 5 ? 575_00_00 : 13_00_00,
    vendor_breakdown: [{ name: 'Vendor A', amount_cents: i === 5 ? 500_00_00 : 10_00_00 }],
    gl_breakdown: [],
    funding_required: i === 5,
  }));
  const weekly = Array.from({ length: 13 }, (_, i) => ({
    week_start: `2026-06-${String((i * 7 % 28) + 1).padStart(2, '0')}`,
    week_end: `2026-06-${String(((i + 1) * 7 % 28) + 1).padStart(2, '0')}`,
    expected_amount: i === 0 ? 580_00_00 : 70_00_00,
    low_band: i === 0 ? 500_00_00 : 49_00_00,
    high_band: i === 0 ? 660_00_00 : 91_00_00,
  }));
  return {
    as_of_date: '2026-06-01',
    horizon_weeks: 13,
    daily,
    weekly,
    monthly: [],
  };
}

// Scenario with EPD savings (day 5 amount reduced) and fewer funding-alert days.
function makeSimulation(): ApCashFlowSimulation {
  const baseline = makeBaseline();
  return {
    baseline,
    scenario: {
      ...baseline,
      daily: baseline.daily.map((d, i) =>
        i === 5
          ? { ...d, expected_amount: 400_00_00, funding_required: false }
          : d,
      ),
      weekly: baseline.weekly.map((w, i) =>
        i === 0 ? { ...w, expected_amount: 480_00_00 } : w,
      ),
    },
    scenario_inputs: {
      pending_approval_delay_days: 7,
      capture_all_epd: true,
    },
  };
}

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
  return render(
    <QueryClientProvider client={createQueryClient()}>{ui}</QueryClientProvider>,
  );
}

describe('WarRoomPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(reportsApi.apCashFlowForecast).mockResolvedValue(makeBaseline());
    vi.mocked(reportsApi.simulateApCashFlowForecast).mockResolvedValue(makeSimulation());
  });

  it('renders baseline KPIs after the initial query resolves', async () => {
    renderWithProviders(<WarRoomPage />);

    await waitFor(() => {
      expect(screen.getByText('Cash Position War Room')).toBeInTheDocument();
    });

    // Baseline KPI labels are present.
    expect(screen.getByText('Total AP')).toBeInTheDocument();
    expect(screen.getByText('Funding-Alert Days')).toBeInTheDocument();
    expect(screen.getByText('EPD Savings')).toBeInTheDocument();
    expect(screen.getByText('Late-Fee Exposure')).toBeInTheDocument();

    // No recommended actions before a scenario runs.
    expect(
      screen.getByText('Adjust a scenario control to surface commitable actions.'),
    ).toBeInTheDocument();
  });

  it('toggles Capture all EPD, runs simulation, and surfaces the EPD action with a Review in Discounts commit', async () => {
    renderWithProviders(<WarRoomPage />);

    await waitFor(() => {
      expect(screen.getByText('Cash Position War Room')).toBeInTheDocument();
    });

    const epdToggle = screen.getByLabelText('Capture all EPD');
    fireEvent.click(epdToggle);

    // The debounced (300 ms) simulation fires.
    await waitFor(() => {
      expect(reportsApi.simulateApCashFlowForecast).toHaveBeenCalledWith(
        expect.objectContaining({
          scenario: expect.objectContaining({
            capture_all_epd: true,
          }),
        }),
      );
    });

    await waitFor(() => {
      expect(screen.getByText(/Capture EPD on/)).toBeInTheDocument();
      expect(screen.getByText('Review in Discounts')).toBeInTheDocument();
    });
  });

  it('changes pending_approval_delay_days, debounces, and surfaces the delay recommendation', async () => {
    renderWithProviders(<WarRoomPage />);

    await waitFor(() => {
      expect(screen.getByText('Cash Position War Room')).toBeInTheDocument();
    });

    const delayInput = screen.getByLabelText('Delay pending approvals (days)');
    fireEvent.change(delayInput, { target: { value: '7' } });

    await waitFor(() => {
      expect(reportsApi.simulateApCashFlowForecast).toHaveBeenCalledWith(
        expect.objectContaining({
          scenario: expect.objectContaining({
            pending_approval_delay_days: 7,
          }),
        }),
      );
    });

    await waitFor(() => {
      expect(screen.getByText('Review approval queue')).toBeInTheDocument();
    });
  });

  it('Reset Scenario clears controls and removes recommended-action cards', async () => {
    renderWithProviders(<WarRoomPage />);

    await waitFor(() => {
      expect(screen.getByText('Cash Position War Room')).toBeInTheDocument();
    });

    const epdToggle = screen.getByLabelText('Capture all EPD');
    fireEvent.click(epdToggle);

    await waitFor(() => {
      expect(screen.getByText('Review in Discounts')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Reset Scenario' }));

    await waitFor(() => {
      expect(screen.queryByText('Review in Discounts')).not.toBeInTheDocument();
    });

    // Empty-state copy returns.
    expect(
      screen.getByText('Adjust a scenario control to surface commitable actions.'),
    ).toBeInTheDocument();
  });
});
