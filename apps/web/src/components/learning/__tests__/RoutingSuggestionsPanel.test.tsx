import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RoutingSuggestionsPanel } from '../RoutingSuggestionsPanel';

const mockGetSuggestions = vi.fn();

vi.mock('@/lib/api', () => ({
  routingPatternsApi: {
    getSuggestions: (...args: unknown[]) => mockGetSuggestions(...args),
  },
}));

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RoutingSuggestionsPanel />
    </QueryClientProvider>,
  );
}

const facilitiesSuggestion = {
  tenant_id: '00000000-0000-0000-0000-000000000001',
  pattern_key: {
    vendor_id: null,
    vendor_name: null,
    department: 'facilities',
    amount_bucket: 'range5k_to25k' as const,
  },
  dominant_approver_id: '11111111-1111-1111-1111-111111111111',
  dominant_approver_name: 'Dana',
  sample_size: 10,
  confidence_pct: 80,
  current_rule_approver_id: '22222222-2222-2222-2222-222222222222',
  suggested_action: 'update_rule' as const,
};

describe('RoutingSuggestionsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the narrative for each suggestion with the issue wording', async () => {
    mockGetSuggestions.mockResolvedValue({ suggestions: [facilitiesSuggestion] });
    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('routing-suggestions')).toBeInTheDocument(),
    );

    expect(
      screen.getByText(
        /80% of facilities invoices over \$5k are re-routed to Dana - update the rule\?/i,
      ),
    ).toBeInTheDocument();

    const updateLink = screen.getByTestId('routing-suggestion-update') as HTMLAnchorElement;
    expect(updateLink.getAttribute('href')).toMatch(
      /^\/processing\/assignment-rules\/new\?/,
    );
    expect(updateLink.getAttribute('href')).toMatch(/department=facilities/);
    expect(updateLink.getAttribute('href')).toMatch(/approver_id=11111111/);
  });

  it('hides a card when dismissed', async () => {
    mockGetSuggestions.mockResolvedValue({ suggestions: [facilitiesSuggestion] });
    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('routing-suggestion-card')).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId('routing-suggestion-dismiss'));

    await waitFor(() =>
      expect(screen.getByTestId('routing-suggestions-empty')).toBeInTheDocument(),
    );
  });

  it('shows the empty state when no suggestions are returned', async () => {
    mockGetSuggestions.mockResolvedValue({ suggestions: [] });
    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('routing-suggestions-empty')).toBeInTheDocument(),
    );
  });

  it('shows the error state on API failure', async () => {
    mockGetSuggestions.mockRejectedValue(new Error('boom'));
    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId('routing-suggestions-error')).toBeInTheDocument(),
    );
  });
});
