import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom';
import React from 'react';

// Mock next/link and next/navigation
vi.mock('next/link', () => ({
  default: ({ children, ...props }: any) => <a {...props}>{children}</a>,
}));
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: vi.fn() }),
  usePathname: () => '/processing/policies/compose',
}));

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// Mock @/lib/api
const mockCompose = vi.fn();
const mockCommit = vi.fn();
vi.mock('@/lib/api', () => ({
  policiesApi: {
    compose: (...args: any[]) => mockCompose(...args),
    commit: (...args: any[]) => mockCommit(...args),
  },
  ApiClientError: class extends Error {
    status: number;
    constructor(status: number, body: any) {
      super(body?.error?.message ?? `API error ${status}`);
      this.status = status;
    }
  },
}));

// Mock UI components as simple pass-throughs
vi.mock('@/components/ui/card', () => ({
  Card: ({ children, ...props }: any) => <div data-testid={props['data-testid'] ?? 'card'} {...props}>{children}</div>,
  CardHeader: ({ children }: any) => <div>{children}</div>,
  CardTitle: ({ children }: any) => <div>{children}</div>,
  CardDescription: ({ children }: any) => <div>{children}</div>,
  CardContent: ({ children }: any) => <div>{children}</div>,
}));
vi.mock('@/components/ui/button', () => ({
  Button: ({ children, onClick, disabled, ...props }: any) => (
    <button onClick={onClick} disabled={disabled} {...props}>{children}</button>
  ),
}));
vi.mock('@/components/ui/textarea', () => ({
  Textarea: ({ value, onChange, ...props }: any) => (
    <textarea value={value} onChange={onChange} {...props} />
  ),
}));
vi.mock('@/components/ui/badge', () => ({
  Badge: ({ children }: any) => <span>{children}</span>,
}));

import PolicyComposerPage from '../page';

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

const MOCK_COMPOSE_RESPONSE = {
  proposed_rule: {
    name: 'Approval threshold $5000.00',
    description: 'Invoices over $5000.00 require approval from manager',
    priority: 50,
    guardrail_kind: 'approval_limit',
    condition_json: { amount_greater_than: 500000 },
    action_json: { approval_from_role: 'manager', action: 'require_approval' },
    summary: 'Any invoice over $5000.00 will require approval from manager.',
  },
  preview: {
    matched_count: 7,
    total_invoices: 20,
    sample_invoices: [
      { id: 'inv-1', invoice_number: 'INV-001', vendor_name: 'Acme Corp', total_amount_cents: 750000, processing_status: 'approved', invoice_date: '2026-05-01' },
      { id: 'inv-2', invoice_number: 'INV-002', vendor_name: 'Beta LLC', total_amount_cents: 1200000, processing_status: 'submitted', invoice_date: '2026-05-15' },
    ],
    projected_action_breakdown: { by_status: { approved: 4, submitted: 3 }, action: 'require_approval' },
  },
  warnings: [],
  unparseable_segments: [],
};

describe('PolicyComposerPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the composer page with textarea and preview button', () => {
    renderWithProviders(<PolicyComposerPage />);
    expect(screen.getByTestId('policy-text-input')).toBeInTheDocument();
    expect(screen.getByTestId('preview-btn')).toBeInTheDocument();
  });

  it('calls compose endpoint on Preview and displays parsed rule and preview table', async () => {
    mockCompose.mockResolvedValueOnce(MOCK_COMPOSE_RESPONSE);

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'over $5000 require approval from manager' } });

    const previewBtn = screen.getByTestId('preview-btn');
    fireEvent.click(previewBtn);

    await waitFor(() => {
      expect(mockCompose).toHaveBeenCalledWith('over $5000 require approval from manager');
    });

    await waitFor(() => {
      // Parsed rule card renders
      expect(screen.getByTestId('parsed-rule-card')).toBeInTheDocument();
      // Preview card renders
      expect(screen.getByTestId('preview-card')).toBeInTheDocument();
      // Sample table renders
      expect(screen.getByTestId('sample-table')).toBeInTheDocument();
    });

    // Match count shows
    expect(screen.getByTestId('matched-count')).toHaveTextContent('7');
  });

  it('calls commit endpoint on Save with the proposed_rule payload', async () => {
    mockCompose.mockResolvedValueOnce(MOCK_COMPOSE_RESPONSE);
    mockCommit.mockResolvedValueOnce({ success: true, rule_id: 'rule-123' });

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'over $5000 require approval from manager' } });

    fireEvent.click(screen.getByTestId('preview-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('save-btn')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('save-btn'));

    await waitFor(() => {
      expect(mockCommit).toHaveBeenCalledWith(
        expect.objectContaining({ guardrail_kind: 'approval_limit' }),
        'over $5000 require approval from manager',
      );
    });
  });

  it('displays error when compose fails', async () => {
    mockCompose.mockRejectedValueOnce(new Error('Could not parse policy: ...'));

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'make everything go faster' } });

    fireEvent.click(screen.getByTestId('preview-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('error-message')).toBeInTheDocument();
    });
  });
});
