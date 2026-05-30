import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import CloseCalendarPage from '../page';

// Mock the API
const mockList = vi.fn();
const mockCreate = vi.fn();
const mockUpdate = vi.fn();
const mockRunClose = vi.fn();

vi.mock('@/lib/api', () => ({
  closePeriodsApi: {
    list: () => mockList(),
    create: (body: unknown) => mockCreate(body),
    update: (id: string, body: unknown) => mockUpdate(id, body),
    runClose: (id: string) => mockRunClose(id),
  },
}));

// Mock the auth store
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    hasModule: () => true,
  })),
}));

const mockPeriods = [
  {
    id: 'period-1',
    tenant_id: 'tenant-1',
    period_label: '2026-04',
    period_start: '2026-04-01',
    period_end: '2026-04-30',
    cutoff_date: '2026-04-25',
    status: 'locked' as const,
    locked_at: '2026-04-30T23:59:59Z',
    locked_by_user_id: 'user-1',
    created_at: '2026-04-01T00:00:00Z',
    updated_at: '2026-04-30T23:59:59Z',
  },
  {
    id: 'period-2',
    tenant_id: 'tenant-1',
    period_label: '2026-05',
    period_start: '2026-05-01',
    period_end: '2026-05-31',
    cutoff_date: '2026-05-25',
    status: 'open' as const,
    locked_at: null,
    locked_by_user_id: null,
    created_at: '2026-05-01T00:00:00Z',
    updated_at: '2026-05-01T00:00:00Z',
  },
];

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

describe('CloseCalendarPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockList.mockResolvedValue(mockPeriods);
    mockCreate.mockResolvedValue({ id: 'period-3', status: 'open' });
    mockRunClose.mockResolvedValue({
      period_id: 'period-2',
      accrual_entries_created: 3,
      erp_post_status: 'posted',
      erp_post_error: null,
    });
  });

  it('renders period list with status badges', async () => {
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-04')).toBeInTheDocument();
    });

    expect(screen.getByText('2026-05')).toBeInTheDocument();
    expect(screen.getByText('Locked')).toBeInTheDocument();
    expect(screen.getByText('Open')).toBeInTheDocument();
  });

  it('opens new period dialog on button click', async () => {
    const user = userEvent.setup();
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-04')).toBeInTheDocument();
    });

    await user.click(screen.getByText('New Period'));

    expect(screen.getByText('Create Close Period')).toBeInTheDocument();
    expect(screen.getByLabelText('Period Label')).toBeInTheDocument();
  });

  it('creates a period when form is submitted', async () => {
    const user = userEvent.setup();
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-04')).toBeInTheDocument();
    });

    await user.click(screen.getByText('New Period'));

    await user.type(screen.getByLabelText('Period Label'), '2026-06');
    await user.type(screen.getByLabelText('Period Start'), '2026-06-01');
    await user.type(screen.getByLabelText('Period End'), '2026-06-30');
    await user.type(screen.getByLabelText('Cutoff Date'), '2026-06-25');

    await user.click(screen.getByText('Create Period'));

    expect(mockCreate).toHaveBeenCalledWith({
      period_label: '2026-06',
      period_start: '2026-06-01',
      period_end: '2026-06-30',
      cutoff_date: '2026-06-25',
    });
  });

  it('shows Run Close button for open periods', async () => {
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-05')).toBeInTheDocument();
    });

    expect(screen.getByText('Run Close')).toBeInTheDocument();
  });

  it('opens confirmation dialog and runs close', async () => {
    const user = userEvent.setup();
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-05')).toBeInTheDocument();
    });

    // Click Run Close on the open period
    const runCloseButtons = screen.getAllByText('Run Close');
    await user.click(runCloseButtons[0]);

    // Confirmation dialog appears
    expect(screen.getByText('Run Month-End Close')).toBeInTheDocument();
    expect(screen.getByText(/2026-05-31/)).toBeInTheDocument();

    // Confirm
    const confirmButtons = screen.getAllByText('Run Close');
    // The last one is the confirm button in the dialog
    await user.click(confirmButtons[confirmButtons.length - 1]);

    expect(mockRunClose).toHaveBeenCalledWith('period-2');
  });

  it('shows accrual entries count after close', async () => {
    const user = userEvent.setup();
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('2026-05')).toBeInTheDocument();
    });

    const runCloseButtons = screen.getAllByText('Run Close');
    await user.click(runCloseButtons[0]);

    const confirmButtons = screen.getAllByText('Run Close');
    await user.click(confirmButtons[confirmButtons.length - 1]);

    await waitFor(() => {
      expect(screen.getByText(/3 accrual entries created/)).toBeInTheDocument();
    });

    expect(screen.getByText(/ERP post status: posted/)).toBeInTheDocument();
  });

  it('shows empty state when no periods exist', async () => {
    mockList.mockResolvedValue([]);
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('No close periods')).toBeInTheDocument();
    });
  });

  it('shows error state when API fails', async () => {
    mockList.mockRejectedValue(new Error('Network error'));
    renderWithProviders(<CloseCalendarPage />);

    await waitFor(() => {
      expect(screen.getByText('Failed to load close periods.')).toBeInTheDocument();
    });
  });
});
