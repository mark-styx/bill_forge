import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import PeerBenchmarkPanel from './PeerBenchmarkPanel';

// Mock the benchmark API
const mockGet = vi.fn();
const mockOptIn = vi.fn();

vi.mock('@/lib/api', () => ({
  benchmarkApi: {
    get: () => mockGet(),
    optIn: (body: unknown) => mockOptIn(body),
  },
}));

function renderWithProviders() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <PeerBenchmarkPanel />
    </QueryClientProvider>,
  );
}

const NOT_OPTED_IN = { opted_in: false };

const OPTED_IN_NO_DATA = {
  opted_in: true,
  cohort: { industry: 'manufacturing', headcount_band: '50-200', volume_band: '500-2000' },
};

const OPTED_IN_WITH_DATA = {
  opted_in: true,
  cohort: { industry: 'manufacturing', headcount_band: '50-200', volume_band: '500-2000' },
  tenant_kpis: {
    dpo_days: 30.0,
    avg_approval_cycle_hours: 24.5,
    ocr_straight_through_rate: 0.85,
    exception_rate: 0.12,
    discount_capture_rate: 0.67,
    cost_per_invoice: 3.5,
  },
  cohort_kpis: {
    p25: {
      dpo_days: 20.0,
      avg_approval_cycle_hours: 12.0,
      ocr_straight_through_rate: 0.7,
      exception_rate: 0.05,
      discount_capture_rate: 0.5,
      cost_per_invoice: 2.0,
    },
    p50: {
      dpo_days: 30.0,
      avg_approval_cycle_hours: 24.0,
      ocr_straight_through_rate: 0.8,
      exception_rate: 0.1,
      discount_capture_rate: 0.6,
      cost_per_invoice: 3.0,
    },
    p75: {
      dpo_days: 45.0,
      avg_approval_cycle_hours: 48.0,
      ocr_straight_through_rate: 0.9,
      exception_rate: 0.2,
      discount_capture_rate: 0.8,
      cost_per_invoice: 5.0,
    },
  },
  cohort_size: 12,
};

describe('PeerBenchmarkPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders opt-in form when not opted in', async () => {
    mockGet.mockResolvedValue(NOT_OPTED_IN);
    renderWithProviders();

    await waitFor(() => {
      expect(screen.getByText('Peer Benchmark')).toBeInTheDocument();
    });

    expect(screen.getByText('Opt In')).toBeInTheDocument();
    expect(screen.getByLabelText('Industry')).toBeInTheDocument();
    expect(screen.getByLabelText('Headcount band')).toBeInTheDocument();
    expect(screen.getByLabelText('Monthly invoice volume band')).toBeInTheDocument();
  });

  it('renders cohort-too-small notice when opted in without cohort data', async () => {
    mockGet.mockResolvedValue(OPTED_IN_NO_DATA);
    renderWithProviders();

    await waitFor(() => {
      expect(screen.getByText(/does not yet have enough members/i)).toBeInTheDocument();
    });
  });

  it('renders six metric rows with data when opted in with cohort data', async () => {
    mockGet.mockResolvedValue(OPTED_IN_WITH_DATA);
    renderWithProviders();

    await waitFor(() => {
      expect(screen.getByText('Days Payable Outstanding')).toBeInTheDocument();
    });

    expect(screen.getByText('Avg Approval Cycle')).toBeInTheDocument();
    expect(screen.getByText('OCR Straight-Through')).toBeInTheDocument();
    expect(screen.getByText('Exception Rate')).toBeInTheDocument();
    expect(screen.getByText('Discount Capture')).toBeInTheDocument();
    expect(screen.getByText('Cost per Invoice')).toBeInTheDocument();

    // Verify cohort size text
    expect(screen.getByText(/12 anonymized peers/)).toBeInTheDocument();
  });

  it('shows loading spinner while fetching', () => {
    mockGet.mockReturnValue(new Promise(() => {})); // never resolves
    renderWithProviders();

    expect(document.querySelector('.animate-spin')).toBeInTheDocument();
  });

  it('calls optIn when form is submitted', async () => {
    const user = userEvent.setup();
    mockGet.mockResolvedValue(NOT_OPTED_IN);
    mockOptIn.mockResolvedValue({ opted_in: true });
    renderWithProviders();

    await waitFor(() => {
      expect(screen.getByText('Opt In')).toBeInTheDocument();
    });

    await user.selectOptions(screen.getByLabelText('Industry'), 'manufacturing');
    await user.selectOptions(screen.getByLabelText('Headcount band'), '50-200');
    await user.selectOptions(screen.getByLabelText('Monthly invoice volume band'), '500-2000');
    await user.click(screen.getByText('Opt In'));

    expect(mockOptIn).toHaveBeenCalledWith({
      industry: 'manufacturing',
      headcount_band: '50-200',
      volume_band: '500-2000',
    });
  });
});
