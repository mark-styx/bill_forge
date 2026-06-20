import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { WeeklyLearningPanel } from '../WeeklyLearningPanel';

const mockGetWeeklyInsights = vi.fn();

vi.mock('@/lib/api', () => ({
  learningApi: {
    getWeeklyInsights: (...args: unknown[]) => mockGetWeeklyInsights(...args),
  },
}));

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <WeeklyLearningPanel />
    </QueryClientProvider>,
  );
}

const baseResponse = {
  week_start: '2026-06-15',
  insights: {
    week_start: '2026-06-15',
    corrections_ingested: {
      gl_recode: 3,
      approver_reroute: 1,
      autopilot_override: 2,
      duplicate_dismissal: 0,
    },
    model_changes: [
      {
        model_kind: 'categorization',
        version: 2,
        corrections_applied: 3,
        baseline_metric: 0.7,
        new_metric: 0.82,
        note: 'Re-fitted correction rules from gl_recode stream',
      },
    ],
    top_recategorizations: [
      { from_value: '5100', to_value: '6100', count: 2 },
    ],
    routing_shifts: [],
    autopilot_overrides_resolved: 2,
  },
};

describe('WeeklyLearningPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders correction counts and model updates from the API', async () => {
    mockGetWeeklyInsights.mockResolvedValue(baseResponse);
    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('corrections-by-kind')).toBeInTheDocument(),
    );

    expect(screen.getByText(/6 corrections ingested/i)).toBeInTheDocument();
    expect(screen.getByText(/GL recodes/i)).toBeInTheDocument();
    expect(screen.getByTestId('model-changes')).toBeInTheDocument();
    expect(screen.getByText(/^categorization$/)).toBeInTheDocument();
  });

  it('shows the empty state when no corrections happened this week', async () => {
    mockGetWeeklyInsights.mockResolvedValue({
      week_start: '2026-06-15',
      insights: {
        week_start: '2026-06-15',
        corrections_ingested: {
          gl_recode: 0,
          approver_reroute: 0,
          autopilot_override: 0,
          duplicate_dismissal: 0,
        },
        model_changes: [],
        top_recategorizations: [],
        routing_shifts: [],
        autopilot_overrides_resolved: 0,
      },
    });

    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('weekly-learning-empty')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('corrections-by-kind')).not.toBeInTheDocument();
  });

  it('renders an error fallback when the API fails', async () => {
    mockGetWeeklyInsights.mockRejectedValue(new Error('network'));
    renderPanel();

    await waitFor(() =>
      expect(
        screen.getByText(/unable to load the weekly learning summary/i),
      ).toBeInTheDocument(),
    );
  });
});
