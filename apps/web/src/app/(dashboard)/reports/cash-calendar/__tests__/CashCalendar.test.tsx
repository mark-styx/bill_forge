import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import CashCalendarPage from '../page';

// Ensure localStorage exists (Node.js 22+ may not have it without --localstorage-file)
vi.hoisted(() => {
  if (typeof globalThis.localStorage === 'undefined') {
    const store: Record<string, string> = {};
    globalThis.localStorage = {
      getItem: (k: string) => store[k] ?? null,
      setItem: (k: string, v: string) => { store[k] = v; },
      removeItem: (k: string) => { delete store[k]; },
      clear: () => { for (const k of Object.keys(store)) delete store[k]; },
      get length() { return Object.keys(store).length; },
      key: (i: number) => Object.keys(store)[i] ?? null,
    };
  }
});

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

const mockForecast = vi.fn();

vi.mock('@/lib/api', () => ({
  reportsApi: {
    apCashFlowForecast: (params?: { horizon_weeks?: number }) => mockForecast(params),
  },
}));

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn((selector: (s: { tenant: { id: string } | null }) => unknown) =>
    selector({ tenant: { id: 'tenant-test-1' } }),
  ),
}));

// Build a forecast with 5 days: 2026-06-01 through 2026-06-05
function makeForecast() {
  const daily = [
    {
      date: '2026-06-01',
      expected_amount: 50_00, // $50
      low_band: 40_00,
      high_band: 60_00,
      vendor_breakdown: [{ name: 'Vendor A', amount_cents: 50_00 }],
      gl_breakdown: [],
      funding_required: false,
    },
    {
      date: '2026-06-02',
      expected_amount: 30_00, // $30
      low_band: 25_00,
      high_band: 35_00,
      vendor_breakdown: [{ name: 'Vendor B', amount_cents: 30_00 }],
      gl_breakdown: [],
      funding_required: false,
    },
    {
      date: '2026-06-03',
      expected_amount: 200_00, // $200
      low_band: 180_00,
      high_band: 220_00,
      vendor_breakdown: [
        { name: 'Vendor C', amount_cents: 150_00 },
        { name: 'Vendor D', amount_cents: 50_00 },
      ],
      gl_breakdown: [],
      funding_required: true,
    },
    {
      date: '2026-06-04',
      expected_amount: 0,
      low_band: 0,
      high_band: 0,
      vendor_breakdown: [],
      gl_breakdown: [],
      funding_required: false,
    },
    {
      date: '2026-06-05',
      expected_amount: 80_00, // $80
      low_band: 70_00,
      high_band: 90_00,
      vendor_breakdown: [{ name: 'Vendor E', amount_cents: 80_00 }],
      gl_breakdown: [],
      funding_required: false,
    },
  ];

  return {
    as_of_date: '2026-06-01',
    horizon_weeks: 13,
    daily,
    weekly: [
      {
        week_start: '2026-06-01',
        week_end: '2026-06-07',
        expected_amount: 360_00,
        low_band: 315_00,
        high_band: 405_00,
      },
    ],
    monthly: [
      {
        month: '2026-06',
        expected_amount: 360_00,
        low_band: 315_00,
        high_band: 405_00,
      },
    ],
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
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('CashCalendarPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockForecast.mockResolvedValue(makeForecast());
    // Reset localStorage
    localStorage.clear();
  });

  it('renders calendar with forecast outflows and projected balance', async () => {
    renderWithProviders(<CashCalendarPage />);

    // Wait for forecast to load
    await waitFor(() => {
      expect(screen.getByText('Cash Calendar')).toBeInTheDocument();
    });

    // Day cell for 2026-06-03 should show $200 outflow
    const cell3 = screen.getByTestId('day-cell-2026-06-03');
    expect(cell3).toBeInTheDocument();
    expect(cell3.textContent).toContain('$200');
    // It should show 2 bills
    expect(cell3.textContent).toContain('2 bills');

    // Day cell for 2026-06-01 should show $50
    const cell1 = screen.getByTestId('day-cell-2026-06-01');
    expect(cell1.textContent).toContain('$50');
    expect(cell1.textContent).toContain('1 bill');
  });

  it('computes projected balance from starting balance', async () => {
    // Set starting balance before render
    localStorage.setItem(
      'billforge:cash-calendar:balance:tenant-test-1',
      '100000',
    ); // $1,000.00

    renderWithProviders(<CashCalendarPage />);

    await waitFor(() => {
      expect(screen.getByTestId('lowest-balance-badge')).toBeInTheDocument();
    });

    // After $50 + $30 + $200 + $80 = $360 outflow from $1000 = $640 remaining
    // Lowest balance is June 5 (last day with outflow): $1000 - $360 = $640
    const badge = screen.getByTestId('lowest-balance-badge');
    expect(badge.textContent).toContain('2026-06-05');

    // Check that a day cell shows its balance
    const cell1 = screen.getByTestId('day-cell-2026-06-01');
    expect(cell1.textContent).toContain('Bal:');
  });

  it('simulates drag from one day to another and updates outflows', async () => {
    localStorage.setItem(
      'billforge:cash-calendar:balance:tenant-test-1',
      '50000',
    ); // $500

    renderWithProviders(<CashCalendarPage />);

    await waitFor(() => {
      expect(screen.getByTestId('day-cell-2026-06-03')).toBeInTheDocument();
    });

    // Drag June 3 ($200) to June 4 ($0)
    const sourceCell = screen.getByTestId('day-cell-2026-06-03');
    const targetCell = screen.getByTestId('day-cell-2026-06-04');

    fireEvent.dragStart(sourceCell);
    fireEvent.drop(targetCell);

    // After drop: June 3 should be $0, June 4 should show $200
    await waitFor(() => {
      // June 3 no longer has outflow (dragged away)
      expect(sourceCell.textContent).not.toContain('$200');
      // June 4 now has the $200 outflow
      expect(targetCell.textContent).toContain('$200');
    });

    // Scenario badge should appear
    expect(screen.getByText(/1 local projection/)).toBeInTheDocument();
  });

  it('shows lowest-projected-balance badge with negative balance in red', async () => {
    // Set a small balance that will go negative
    localStorage.setItem(
      'billforge:cash-calendar:balance:tenant-test-1',
      '10000',
    ); // $100

    renderWithProviders(<CashCalendarPage />);

    await waitFor(() => {
      expect(screen.getByTestId('lowest-balance-badge')).toBeInTheDocument();
    });

    // After $50 + $30 + $200 + $80 = $360 outflow from $100 => -$260 on June 5
    const badge = screen.getByTestId('lowest-balance-badge');
    expect(badge.textContent).toContain('-$');
    expect(badge.textContent).toContain('2026-06-05');

    // Badge parent should have error styling
    const badgeParent = badge.closest('[class*="text-error"]') ?? badge.parentElement;
    expect(badgeParent?.className).toContain('text-error');
  });

  it('resets scenario on button click', async () => {
    renderWithProviders(<CashCalendarPage />);

    await waitFor(() => {
      expect(screen.getByTestId('day-cell-2026-06-03')).toBeInTheDocument();
    });

    // Drag
    const sourceCell = screen.getByTestId('day-cell-2026-06-03');
    const targetCell = screen.getByTestId('day-cell-2026-06-04');
    fireEvent.dragStart(sourceCell);
    fireEvent.drop(targetCell);

    await waitFor(() => {
      expect(screen.getByText(/1 local projection/)).toBeInTheDocument();
    });

    // Reset
    fireEvent.click(screen.getByText('Reset scenario'));

    // June 3 should show $200 again
    await waitFor(() => {
      expect(sourceCell.textContent).toContain('$200');
    });

    // Scenario badge should be gone
    expect(screen.queryByText(/local projection/)).not.toBeInTheDocument();
  });

  it('shows error state when forecast fails', async () => {
    mockForecast.mockRejectedValue(new Error('Network error'));

    renderWithProviders(<CashCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('Forecast unavailable')).toBeInTheDocument();
    });
  });
});
