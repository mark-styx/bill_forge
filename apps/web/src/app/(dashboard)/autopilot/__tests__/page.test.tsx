import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import AutopilotPage from '../page';

// Mock sonner so tests can assert toast.success / toast.error calls.
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock next/navigation
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: vi.fn() }),
  usePathname: () => '/autopilot',
}));

// Mock the API client
const mockGetQueue = vi.fn();
const mockResolve = vi.fn();
const mockGetSettings = vi.fn();
const mockUpdateSettings = vi.fn();
const mockGetReport = vi.fn();

vi.mock('@/lib/api', () => ({
  autopilotApi: {
    getQueue: (...args: unknown[]) => mockGetQueue(...args),
    resolve: (...args: unknown[]) => mockResolve(...args),
    getSettings: (...args: unknown[]) => mockGetSettings(...args),
    updateSettings: (...args: unknown[]) => mockUpdateSettings(...args),
    getReport: (...args: unknown[]) => mockGetReport(...args),
  },
}));

// Mock ConfidenceBadge so the test isn't coupled to its thresholds.
vi.mock('@/components/ConfidenceBadge', () => ({
  ConfidenceBadge: ({ confidence }: { confidence: number }) => (
    <span data-testid="confidence-badge">{Math.round(confidence * 100)}%</span>
  ),
}));

const mockQueue = {
  items: [
    {
      // Lower confidence (0.40) comes first because the backend sorts the
      // queue ascending by confidence so AP staff see the most uncertain
      // exceptions first.
      id: 'gl_ambiguity:22222222-2222-2222-2222-222222222222',
      invoice_id: '22222222-2222-2222-2222-222222222222',
      exception_type: 'gl_ambiguity',
      proposed_resolution: {
        action: 'assign_gl',
        payload: {},
        rationale: 'Categorization confidence 40% is below 80%.',
      },
      confidence: 0.4,
      auto_resolve_eligible: false,
    },
    {
      id: 'ocr_low_confidence:11111111-1111-1111-1111-111111111111',
      invoice_id: '11111111-1111-1111-1111-111111111111',
      exception_type: 'ocr_low_confidence',
      proposed_resolution: {
        action: 'approve',
        payload: { ocr_confidence: 0.55 },
        rationale: 'OCR confidence 55% is below the 90% review threshold.',
      },
      confidence: 0.55,
      auto_resolve_eligible: false,
    },
  ],
  threshold: 0.95,
  enabled_types: ['ocr_low_confidence'],
};

const mockSettings = {
  autopilot_threshold: 0.95,
  autopilot_enabled_types: ['ocr_low_confidence'],
};

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <AutopilotPage />
    </QueryClientProvider>,
  );
}

describe('AutopilotPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetQueue.mockResolvedValue(mockQueue);
    mockGetSettings.mockResolvedValue(mockSettings);
    mockUpdateSettings.mockResolvedValue(mockSettings);
    mockResolve.mockResolvedValue({
      exception_id: 'resolved-id',
      decision: 'confirm',
      applied_action: 'approve',
    });
    mockGetReport.mockResolvedValue({ date: '2026-06-19', rows: [], uncertain_types: [] });
  });

  it('renders queue rows for each exception', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
      expect(screen.getByText('GL Ambiguity')).toBeInTheDocument();
    });

    const badges = screen.getAllByTestId('confidence-badge');
    expect(badges).toHaveLength(2);
    expect(badges[0]).toHaveTextContent('40%'); // sorted lowest-confidence first
    expect(badges[1]).toHaveTextContent('55%');
  });

  it('calls resolve with confirm when the Confirm button is clicked', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    const confirms = screen.getAllByRole('button', { name: /^Confirm proposed resolution for / });
    fireEvent.click(confirms[0]);

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
      const [exceptionId, decision] = mockResolve.mock.calls[0];
      // The clicked row may be either queue item depending on sort order;
      // assert the call shape rather than the exact id.
      expect(decision).toBe('confirm');
      expect(exceptionId).toMatch(/^(ocr_low_confidence|gl_ambiguity):/);
    });
  });

  it('invokes resolve(confirm) via the Y keystroke on the selected row', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    // The first row is selected by default. 'y' confirms it.
    fireEvent.keyDown(window, { key: 'y' });

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
      const [, decision] = mockResolve.mock.calls[0];
      expect(decision).toBe('confirm');
    });
  });

  it('PUTs the threshold to settings when the slider changes', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByLabelText('Autopilot auto-resolve threshold')).toBeInTheDocument();
    });

    const slider = screen.getByLabelText('Autopilot auto-resolve threshold') as HTMLInputElement;
    expect(slider.value).toBe('0.95');

    fireEvent.change(slider, { target: { value: '0.8' } });

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith(
        expect.objectContaining({ autopilot_threshold: 0.8 }),
      );
    });
  });

  it('toggles enabled_types via the per-type chips and PUTs to settings', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByLabelText('Toggle autopilot auto-resolve for GL Ambiguity')).toBeInTheDocument();
    });

    const chip = screen.getByLabelText('Toggle autopilot auto-resolve for GL Ambiguity');
    // GL Ambiguity is not in the default enabled list, so toggling adds it.
    expect(chip).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(chip);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          autopilot_enabled_types: expect.arrayContaining(['gl_ambiguity']),
        }),
      );
    });
  });

  it('overrides via the N keystroke and sends decision=override', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'n' });

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
      const [, decision, overrideAction] = mockResolve.mock.calls[0];
      expect(decision).toBe('override');
      expect(overrideAction).toBeDefined();
      expect(overrideAction.action).toBeTruthy();
    });
  });

  it('j/k keystrokes move the selection without resolving', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    // First row is selected initially. Press 'j' to move down.
    fireEvent.keyDown(window, { key: 'j' });

    // Now 'y' should confirm the SECOND row (the OCR Low Confidence one,
    // since the queue is sorted ascending by confidence: GL Ambiguity 40% is first).
    fireEvent.keyDown(window, { key: 'y' });

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
    });
    const [confirmedId] = mockResolve.mock.calls[mockResolve.mock.calls.length - 1];
    expect(confirmedId).toBe('ocr_low_confidence:11111111-1111-1111-1111-111111111111');
  });

  it('shows the empty state when the queue is clear', async () => {
    mockGetQueue.mockResolvedValue({ items: [], threshold: 0.95, enabled_types: [] });

    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Queue is clear')).toBeInTheDocument();
    });
  });

  it('does not invoke resolve when typing into the threshold slider', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    const slider = screen.getByLabelText('Autopilot auto-resolve threshold');
    // Simulate a keydown on the input element itself; should be ignored by the
    // global keystroke handler so the slider's own keyboard semantics apply.
    fireEvent.keyDown(slider, { key: 'y' });

    // The slider onChange should not have fired a resolve mutation as a side
    // effect; resolve is only triggered by the global handler when the target
    // is not an INPUT.
    expect(mockResolve).not.toHaveBeenCalled();
  });

  it('shows a success toast when confirming an exception via Y', async () => {
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'y' });

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalledWith('Exception confirmed');
    });
  });

  it('shows an error toast when the resolve mutation fails', async () => {
    mockResolve.mockRejectedValueOnce(new Error('boom'));

    renderPage();

    await waitFor(() => {
      expect(screen.getByText('OCR Low Confidence')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'y' });

    await waitFor(() => {
      expect(mockResolve).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('boom');
    });
  });

  it('renders a retryable error state when the queue fails to load', async () => {
    mockGetQueue.mockRejectedValueOnce(new Error('queue down'));

    renderPage();

    const errorState = await screen.findByTestId('autopilot-queue-error');
    expect(errorState).toHaveTextContent("Couldn't load exception queue");
    expect(errorState).toHaveTextContent('queue down');

    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => {
      // First call (failed) + Retry call.
      expect(mockGetQueue).toHaveBeenCalledTimes(2);
    });
  });
});
