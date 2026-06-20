import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import AddinTaskpane from '../page';

interface OfficeStub {
  context: {
    mailbox: {
      item: {
        itemId: string;
        internetMessageId: string;
        subject: string;
        from: { emailAddress: string; displayName: string };
        attachments: Array<{
          id: string;
          name: string;
          contentType: string;
          attachmentType: string;
          isInline: boolean;
        }>;
        getAttachmentContentAsync?: (
          id: string,
          cb: (result: { status: string; value: { content: string; format: string } }) => void,
        ) => void;
      };
    };
  };
}

function installOffice(stub: OfficeStub) {
  (globalThis as unknown as { Office: OfficeStub }).Office = stub;
}

function makeOffice(overrides: Partial<OfficeStub['context']['mailbox']['item']> = {}): OfficeStub {
  return {
    context: {
      mailbox: {
        item: {
          itemId: 'AAMkAGI...',
          internetMessageId: '<msg-1@example.com>',
          subject: 'Invoice ACME-2024-001',
          from: { emailAddress: 'billing@acme.com', displayName: 'ACME Billing' },
          attachments: [],
          ...overrides,
        },
      },
    },
  };
}

function installLocalStorage(token: string) {
  const store = new Map<string, string>();
  store.set('billforge-auth', JSON.stringify({ state: { accessToken: token } }));
  const stub: Storage = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (key: string) => store.get(key) ?? null,
    key: (idx: number) => Array.from(store.keys())[idx] ?? null,
    removeItem: (key: string) => {
      store.delete(key);
    },
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
  };
  Object.defineProperty(window, 'localStorage', {
    value: stub,
    writable: true,
    configurable: true,
  });
}

beforeEach(() => {
  vi.restoreAllMocks();
  installLocalStorage('tok-123');
});

describe('Outlook add-in taskpane', () => {
  it('renders the full triage surface for a matched invoice', async () => {
    installOffice(makeOffice());

    const lookupBody = {
      invoice_id: '11111111-1111-1111-1111-111111111111',
      vendor: { id: 'v1', name: 'ACME Corp', email: 'billing@acme.com' },
      totals: {
        subtotal_cents: 100000,
        tax_amount_cents: 8000,
        total_amount_cents: 108000,
        currency: 'USD',
        invoice_number: 'ACME-2024-001',
        invoice_date: '2024-01-15',
        due_date: '2024-02-15',
      },
      line_items: [
        {
          description: 'Consulting',
          quantity: 10,
          unit_price_cents: 10000,
          total_cents: 100000,
        },
      ],
      gl_coding: [{ gl_code: '6010', department: 'Eng', cost_center: 'NYC' }],
      policy_warnings: [{ severity: 'error', message: 'Over budget by $5,000' }],
      vendor_history_summary: {
        invoice_count_last_90d: 3,
        total_spend_cents_last_90d: 300000,
        last_invoice_date: '2024-01-01',
      },
      comments: [],
      approval_state: 'pending_approval',
    };

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => lookupBody,
    });
    vi.stubGlobal('fetch', fetchMock);

    render(<AddinTaskpane />);

    await waitFor(() => {
      expect(screen.getByTestId('preview')).toBeInTheDocument();
    });

    expect(screen.getByText('ACME-2024-001')).toBeInTheDocument();
    expect(screen.getByTestId('line-items')).toBeInTheDocument();
    expect(screen.getByText('Consulting')).toBeInTheDocument();
    expect(screen.getByTestId('gl-coding')).toBeInTheDocument();
    expect(screen.getByText('6010')).toBeInTheDocument();
    expect(screen.getByTestId('policy-warnings')).toBeInTheDocument();
    expect(screen.getByText('Over budget by $5,000')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /approve/i })).toBeEnabled();
    expect(screen.getByRole('button', { name: /reject/i })).toBeEnabled();

    expect(fetchMock).toHaveBeenCalled();
    const [, fetchInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    const authHeader = new Headers(fetchInit.headers).get('Authorization');
    expect(authHeader).toBe('Bearer tok-123');
  });

  it('falls back to the "Push to BillForge" surface when no invoice matches', async () => {
    installOffice(
      makeOffice({
        attachments: [
          {
            id: 'att-1',
            name: 'invoice.pdf',
            contentType: 'application/pdf',
            attachmentType: 'file',
            isInline: false,
          },
        ],
      }),
    );

    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      json: async () => ({}),
    });
    vi.stubGlobal('fetch', fetchMock);

    render(<AddinTaskpane />);

    await waitFor(() => {
      expect(screen.getByTestId('not-found-message')).toBeInTheDocument();
    });

    expect(screen.getByTestId('push-attachments')).toBeInTheDocument();
    expect(screen.getByText('invoice.pdf')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /push to billforge/i })).toBeEnabled();
  });
});
