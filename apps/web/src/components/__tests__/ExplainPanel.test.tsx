import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ExplainPanel } from '../ExplainPanel';

vi.mock('@/lib/api/explain', () => ({
  getCategorizationExplanation: vi.fn(),
  submitOverride: vi.fn(),
}));

import {
  getCategorizationExplanation,
  submitOverride,
} from '@/lib/api/explain';
const mockedGet = vi.mocked(getCategorizationExplanation);
const mockedOverride = vi.mocked(submitOverride);

function renderPanel(onOverride?: () => void) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ExplainPanel
        decisionKind="categorization"
        invoiceId="invoice-1"
        onOverride={onOverride}
      />
    </QueryClientProvider>,
  );
}

const explanation = {
  decision_id: 'invoice-1',
  decision_kind: 'categorization',
  inputs: {
    vendor_name: 'Acme Software',
    amount_cents: 50000,
    line_text: 'Annual SaaS license',
  },
  top_signals: [
    { name: 'keyword_match', weight: 0.5, direction: '+', value: "'software' family" },
    { name: 'vendor_history', weight: 0.3, direction: '+', value: '5 prior invoices' },
    { name: 'model_confidence', weight: 0.2, direction: '+', value: '92% confidence' },
  ],
  citations: [
    { kind: 'keyword', ref: 'software', span: 'keyword family in line text' },
    { kind: 'prior_coding', ref: 'inv-prior-1', span: 'INV-001 — Acme Software' },
  ],
  counterfactual: {
    variable: 'vendor',
    current: 'Acme Software',
    alternative: 'Marketing Maven',
    predicted_outcome: '7000-Marketing',
  },
  current_outcome: '6000-Software & Subscriptions',
  rationale_text: 'Keyword + vendor history',
};

describe('ExplainPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders inputs, signals, citations, and counterfactual sections', async () => {
    mockedGet.mockResolvedValueOnce(explanation);
    renderPanel();

    await waitFor(() => {
      expect(screen.getByTestId('explain-panel')).toBeInTheDocument();
    });

    expect(screen.getByTestId('explain-section-inputs')).toBeInTheDocument();
    expect(screen.getByTestId('explain-section-signals')).toBeInTheDocument();
    expect(screen.getByTestId('explain-section-citations')).toBeInTheDocument();
    expect(screen.getByTestId('explain-section-counterfactual')).toBeInTheDocument();

    // Signal names and citation spans should appear.
    expect(screen.getByText('keyword_match')).toBeInTheDocument();
    expect(screen.getByText(/'software' family/)).toBeInTheDocument();
    expect(screen.getByText(/INV-001 — Acme Software/)).toBeInTheDocument();

    // Counterfactual mentions the alternative and predicted outcome.
    expect(screen.getByText(/Marketing Maven/)).toBeInTheDocument();
    expect(screen.getByText(/7000-Marketing/)).toBeInTheDocument();
  });

  it('submits override and invokes onOverride callback', async () => {
    mockedGet.mockResolvedValueOnce(explanation);
    mockedOverride.mockResolvedValueOnce({
      recorded: true,
      correction_type: 'gl_recode',
    });

    const onOverride = vi.fn();
    renderPanel(onOverride);

    await waitFor(() => {
      expect(screen.getByTestId('explain-panel')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('explain-panel-override-toggle'));

    const glInput = screen.getByTestId('explain-panel-override-gl') as HTMLInputElement;
    await userEvent.type(glInput, '7000-Marketing');

    fireEvent.click(screen.getByTestId('explain-panel-override-submit'));

    await waitFor(() => {
      expect(mockedOverride).toHaveBeenCalledWith('invoice-1', {
        corrected_gl_code: '7000-Marketing',
        reason: undefined,
      });
    });

    await waitFor(() => {
      expect(onOverride).toHaveBeenCalled();
    });
  });

  it('falls back silently on API error', async () => {
    mockedGet.mockRejectedValueOnce(new Error('404'));
    renderPanel();

    await waitFor(() => {
      expect(screen.getByTestId('explain-panel-empty')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('explain-panel')).not.toBeInTheDocument();
  });
});
