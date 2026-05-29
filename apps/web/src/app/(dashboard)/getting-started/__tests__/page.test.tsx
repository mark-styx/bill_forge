import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ImplementationStatus } from '@/lib/api';

const apiMocks = vi.hoisted(() => ({
  status: vi.fn(),
  syncErp: vi.fn(),
  updateErpSubItems: vi.fn(),
  selectApprovalTemplate: vi.fn(),
  uploadSampleInvoices: vi.fn(),
  updateChecklist: vi.fn(),
}));

vi.mock('@/lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api')>();
  return {
    ...actual,
    implementationApi: {
      status: apiMocks.status,
      syncErp: apiMocks.syncErp,
      updateErpSubItems: apiMocks.updateErpSubItems,
      selectApprovalTemplate: apiMocks.selectApprovalTemplate,
      uploadSampleInvoices: apiMocks.uploadSampleInvoices,
      updateChecklist: apiMocks.updateChecklist,
    },
  };
});

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    tenant: { id: 'test-tenant-1', name: 'Test Org' },
  })),
}));

vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

import GettingStartedPage from '../page';

function makeStatus(overrides: Partial<ImplementationStatus> = {}): ImplementationStatus {
  const base: ImplementationStatus = {
    started_at: '2026-05-29T00:00:00Z',
    day_number: 1,
    percent_complete: 0,
    phases: {
      erp: {
        status: 'not_started',
        provider: null,
        sub_items: { chart_of_accounts: false, vendors: false, open_pos: false },
        last_sync: null,
        last_error: null,
      },
      approvals: {
        status: 'not_started',
        template: null,
        template_id: null,
      },
      ocr: {
        status: 'not_started',
        count: 0,
        sample_invoice_ids: [],
      },
      go_live: {
        status: 'not_started',
        checks: {
          notify_ap_team: false,
          set_email_forwarding: false,
          enable_approval_routing: false,
          schedule_first_payment_run: false,
          confirm_cutover_date: false,
        },
      },
    },
  };
  return { ...base, ...overrides };
}

describe('GettingStartedPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiMocks.status.mockResolvedValue(makeStatus());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders backend implementation status and the four phase headings', async () => {
    render(<GettingStartedPage />);

    expect(await screen.findByText('Implementation Wizard')).toBeInTheDocument();
    expect(screen.getByText('Connect your accounting system')).toBeInTheDocument();
    expect(screen.getByText('Choose an approval-chain template')).toBeInTheDocument();
    expect(screen.getByText('Validate OCR with 10 sample invoices')).toBeInTheDocument();
    expect(screen.getByText('Go-live checklist')).toBeInTheDocument();
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('0% complete');
    expect(apiMocks.status).toHaveBeenCalledTimes(1);
  });

  it('runs backend ERP sync and updates progress', async () => {
    const user = userEvent.setup();
    const synced = makeStatus({
      percent_complete: 25,
      phases: {
        ...makeStatus().phases,
        erp: {
          status: 'complete',
          provider: 'quickbooks',
          sub_items: { chart_of_accounts: true, vendors: true, open_pos: true },
          last_sync: null,
          last_error: null,
        },
      },
    });
    apiMocks.syncErp.mockResolvedValueOnce(synced);

    render(<GettingStartedPage />);
    await user.click(await screen.findByRole('button', { name: /sync quickbooks/i }));

    expect(apiMocks.syncErp).toHaveBeenCalledWith('quickbooks');
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('25% complete');
  });

  it('selects an approval template through the implementation API', async () => {
    const user = userEvent.setup();
    const selected = makeStatus({
      percent_complete: 25,
      phases: {
        ...makeStatus().phases,
        approvals: { status: 'complete', template: 'department', template_id: 'template-1' },
      },
    });
    apiMocks.selectApprovalTemplate.mockResolvedValueOnce(selected);

    render(<GettingStartedPage />);
    await user.click(await screen.findByText('By department'));

    expect(apiMocks.selectApprovalTemplate).toHaveBeenCalledWith('department');
    const templateButton = screen.getByText('By department').closest('button')!;
    expect(templateButton.className).toContain('border-primary');
  });

  it('uploads sample invoice files and updates the OCR count', async () => {
    const user = userEvent.setup();
    const uploaded = makeStatus({
      phases: {
        ...makeStatus().phases,
        ocr: { status: 'in_progress', count: 2, sample_invoice_ids: ['inv-1', 'inv-2'] },
      },
    });
    apiMocks.uploadSampleInvoices.mockResolvedValueOnce({ uploaded: [], status: uploaded });

    render(<GettingStartedPage />);
    const input = await screen.findByLabelText('Upload Samples');
    await user.upload(input, [
      new File(['one'], 'one.pdf', { type: 'application/pdf' }),
      new File(['two'], 'two.pdf', { type: 'application/pdf' }),
    ]);

    expect(apiMocks.uploadSampleInvoices).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('ocr-count')).toHaveTextContent('2 / 10 uploaded');
  });

  it('persists go-live checklist changes through the implementation API', async () => {
    const user = userEvent.setup();
    const updated = makeStatus({
      phases: {
        ...makeStatus().phases,
        go_live: {
          status: 'in_progress',
          checks: {
            notify_ap_team: true,
            set_email_forwarding: false,
            enable_approval_routing: false,
            schedule_first_payment_run: false,
            confirm_cutover_date: false,
          },
        },
      },
    });
    apiMocks.updateChecklist.mockResolvedValueOnce(updated);

    render(<GettingStartedPage />);
    const card = (await screen.findByText('Go-live checklist')).closest('.border')!;
    await user.click(within(card as HTMLElement).getByLabelText('Notify AP team of go-live date'));

    expect(apiMocks.updateChecklist).toHaveBeenCalledWith(updated.phases.go_live.checks);
  });
});
