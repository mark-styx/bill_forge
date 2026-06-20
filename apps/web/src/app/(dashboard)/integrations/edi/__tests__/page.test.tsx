import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import EdiStatusPage from '../page';
import { ediApi } from '@/lib/api';

vi.mock('@/lib/api', () => ({
  ediApi: {
    status: vi.fn(),
  },
}));

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn((selector) => {
    const state = { hasModule: vi.fn().mockReturnValue(false) };
    return selector ? selector(state) : state;
  }),
}));

describe('EDI status page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the entitlement-blocked component when the tenant lacks the EDI module', async () => {
    vi.mocked(ediApi.status).mockResolvedValue({
      connected: false,
      provider: null,
      document_count: 0,
      partner_count: 0,
      entitled: false,
      pending_acks: 0,
      last_inbound_at: null,
      last_outbound_at: null,
      ack_timeouts_last_24h: 0,
    });

    render(<EdiStatusPage />);

    expect(await screen.findByText(/edi add-on required/i)).toBeInTheDocument();
    // The blocked surface must NOT render the stat grid.
    expect(screen.queryByTestId('edi-stat-pending-acks')).not.toBeInTheDocument();
  });

  it('renders all four stat cards with API-driven values when the tenant is entitled', async () => {
    vi.mocked(ediApi.status).mockResolvedValue({
      connected: true,
      provider: 'stedi',
      document_count: 42,
      partner_count: 3,
      entitled: true,
      pending_acks: 7,
      last_inbound_at: '2026-06-20T10:00:00Z',
      last_outbound_at: '2026-06-20T11:30:00Z',
      ack_timeouts_last_24h: 2,
    });

    render(<EdiStatusPage />);

    await waitFor(() => {
      expect(screen.getByTestId('edi-stat-pending-acks-value')).toHaveTextContent('7');
    });

    expect(screen.getByTestId('edi-stat-ack-timeouts-value')).toHaveTextContent('2');
    expect(screen.getByTestId('edi-stat-last-inbound-value').textContent).not.toBe('');
    expect(screen.getByTestId('edi-stat-last-outbound-value').textContent).not.toBe('');

    // Anti-pattern check from commit f3ec46a3: no hardcoded 'Unavailable' values.
    expect(screen.queryByText(/^Unavailable$/i)).not.toBeInTheDocument();
  });
});
