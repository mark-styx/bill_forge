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

const SINGLE_RULE_MOCK = {
  rule: {
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
};

const MOCK_COMPOSE_RESPONSE = {
  proposed_rules: [SINGLE_RULE_MOCK],
  unparseable_segments: [],
  // Backward-compat single-rule mirror
  proposed_rule: SINGLE_RULE_MOCK.rule,
  preview: SINGLE_RULE_MOCK.preview,
  warnings: [],
};

// Marquee compound example: 2 rules (routing + new-vendor approval).
const MOCK_COMPOUND_RESPONSE = {
  proposed_rules: [
    {
      rule: {
        name: 'Route marketing invoices over $10000.00 to cmo',
        description: 'Invoices over $10000.00 from marketing are routed to cmo',
        priority: 35,
        guardrail_kind: 'routing_rule',
        condition_json: { amount_greater_than: 1000000, department: 'marketing' },
        action_json: { action: 'route_to_role', route_to_role: 'cmo' },
        summary: 'Invoices over $10000.00 from marketing will be routed to cmo.',
      },
      preview: {
        matched_count: 3,
        total_invoices: 20,
        sample_invoices: [
          { id: 'inv-a', invoice_number: 'INV-100', vendor_name: 'Marketing Co', total_amount_cents: 1500000, processing_status: 'submitted', invoice_date: '2026-05-01' },
        ],
        projected_action_breakdown: { by_status: { submitted: 3 }, action: 'route_to_role' },
      },
      warnings: [],
    },
    {
      rule: {
        name: 'New vendor approval: finance',
        description: 'Invoices from new vendors require finance review',
        priority: 60,
        guardrail_kind: 'approval_limit',
        condition_json: { new_vendor: true },
        action_json: { action: 'require_approval', approval_from_role: 'finance' },
        summary: 'Any invoice from a new vendor will require finance review before approval.',
      },
      preview: {
        matched_count: 5,
        total_invoices: 20,
        sample_invoices: [
          { id: 'inv-b', invoice_number: 'INV-200', vendor_name: 'Newco', total_amount_cents: 80000, processing_status: 'pending', invoice_date: '2026-05-02' },
        ],
        projected_action_breakdown: { by_status: { pending: 5 }, action: 'require_approval' },
      },
      warnings: [],
    },
  ],
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
      // Per-rule sample table renders
      expect(screen.getByTestId('sample-table-0')).toBeInTheDocument();
      // Per-rule matched count renders
      expect(screen.getByTestId('matched-count-0')).toHaveTextContent('7');
    });
  });

  it('calls commit endpoint on Save with the proposed_rules payload', async () => {
    mockCompose.mockResolvedValueOnce(MOCK_COMPOSE_RESPONSE);
    mockCommit.mockResolvedValueOnce({ success: true, rule_id: 'rule-123', rule_ids: ['rule-123'] });

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'over $5000 require approval from manager' } });

    fireEvent.click(screen.getByTestId('preview-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('save-btn')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('save-btn'));

    await waitFor(() => {
      expect(mockCommit).toHaveBeenCalledTimes(1);
      const [rules, originalText] = mockCommit.mock.calls[0];
      expect(originalText).toBe('over $5000 require approval from manager');
      expect(Array.isArray(rules)).toBe(true);
      expect(rules).toHaveLength(1);
      expect(rules[0]).toEqual(
        expect.objectContaining({ guardrail_kind: 'approval_limit' }),
      );
    });
  });

  it('renders two rule cards for the marquee compound example and commits both', async () => {
    mockCompose.mockResolvedValueOnce(MOCK_COMPOUND_RESPONSE);
    mockCommit.mockResolvedValueOnce({ success: true, rule_ids: ['rule-a', 'rule-b'] });

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, {
      target: {
        value:
          'Invoices over $10k from Marketing go to the CMO, and anything from a new vendor needs Finance review before approval',
      },
    });

    fireEvent.click(screen.getByTestId('preview-btn'));

    // Both rule cards render with their summaries.
    await waitFor(() => {
      expect(screen.getAllByTestId('parsed-rule-card')).toHaveLength(2);
    });
    expect(screen.getByTestId('rule-summary-0')).toHaveTextContent(
      'Invoices over $10000.00 from marketing will be routed to cmo.',
    );
    expect(screen.getByTestId('rule-summary-1')).toHaveTextContent(
      'Any invoice from a new vendor will require finance review before approval.',
    );

    fireEvent.click(screen.getByTestId('save-btn'));

    // Save posts a CommitRequest containing BOTH rules.
    await waitFor(() => {
      expect(mockCommit).toHaveBeenCalledTimes(1);
      const [rules, originalText] = mockCommit.mock.calls[0];
      expect(Array.isArray(rules)).toBe(true);
      expect(rules).toHaveLength(2);
      expect(originalText).toContain('Invoices over $10k');
      expect(rules[0]).toEqual(expect.objectContaining({ guardrail_kind: 'routing_rule' }));
      expect(rules[1]).toEqual(expect.objectContaining({ guardrail_kind: 'approval_limit' }));
    });
  });

  it('allows deselecting an individual rule before commit', async () => {
    mockCompose.mockResolvedValueOnce(MOCK_COMPOUND_RESPONSE);
    mockCommit.mockResolvedValueOnce({ success: true, rule_ids: ['rule-a'] });

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'marquee example' } });

    fireEvent.click(screen.getByTestId('preview-btn'));

    await waitFor(() => {
      expect(screen.getAllByTestId('parsed-rule-card')).toHaveLength(2);
    });

    // Deselect the second rule.
    fireEvent.click(screen.getByTestId('rule-checkbox-1'));

    fireEvent.click(screen.getByTestId('save-btn'));

    await waitFor(() => {
      expect(mockCommit).toHaveBeenCalledTimes(1);
      const [rules] = mockCommit.mock.calls[0];
      expect(rules).toHaveLength(1);
      expect(rules[0]).toEqual(expect.objectContaining({ guardrail_kind: 'routing_rule' }));
    });
  });

  it('renders unparseable segments panel when present', async () => {
    mockCompose.mockResolvedValueOnce({
      proposed_rules: [SINGLE_RULE_MOCK],
      unparseable_segments: ['frobnicate the widgets'],
    });

    renderWithProviders(<PolicyComposerPage />);

    const textarea = screen.getByTestId('policy-text-input');
    fireEvent.change(textarea, { target: { value: 'over $5000 require approval from manager, and frobnicate the widgets' } });

    fireEvent.click(screen.getByTestId('preview-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('unparseable-segments-card')).toBeInTheDocument();
      expect(screen.getByTestId('unparseable-segments-list')).toHaveTextContent(
        'frobnicate the widgets',
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
