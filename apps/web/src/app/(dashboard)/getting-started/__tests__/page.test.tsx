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
  updatePrivacyMode: vi.fn(),
  updateCaptureChannels: vi.fn(),
  verifyEmailForwarding: vi.fn(),
  ackModuleEntitlements: vi.fn(),
  updateNotificationApprovals: vi.fn(),
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
      updatePrivacyMode: apiMocks.updatePrivacyMode,
      updateCaptureChannels: apiMocks.updateCaptureChannels,
      verifyEmailForwarding: apiMocks.verifyEmailForwarding,
      ackModuleEntitlements: apiMocks.ackModuleEntitlements,
      updateNotificationApprovals: apiMocks.updateNotificationApprovals,
    },
  };
});

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    tenant: { id: 'test-tenant-1', name: 'Test Org', enabled_modules: [] },
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
      configuration: {
        status: 'not_started',
        configuration: {
          privacy_mode: { enabled: false, scope: null, confirmed_at: null },
          capture_channels: {
            email_forwarding: { address: '', verified_at: null },
            manual_upload_enabled: false,
            erp_sync_enabled: false,
          },
          module_entitlements: [],
          notification_approvals: {
            ap_team_distribution: [],
            escalation_distribution: [],
            approved_at: null,
          },
        },
      },
      go_live: {
        status: 'not_started',
        checks: {
          confirm_cutover_date: false,
          forwarding_email_verified: false,
          sample_invoice_routed: false,
          notifications_acknowledged: false,
          privacy_mode_confirmed: false,
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

  it('renders backend implementation status and the phase headings including Configuration', async () => {
    render(<GettingStartedPage />);

    expect(await screen.findByText('Implementation Wizard')).toBeInTheDocument();
    expect(screen.getByText('Connect your accounting system')).toBeInTheDocument();
    expect(screen.getByText('Choose an approval-chain template')).toBeInTheDocument();
    expect(screen.getByText('Validate OCR with 10 sample invoices')).toBeInTheDocument();
    expect(screen.getByText('Configuration')).toBeInTheDocument();
    expect(screen.getByText('Go-live checklist')).toBeInTheDocument();
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('0% complete');
    expect(apiMocks.status).toHaveBeenCalledTimes(1);
  });

  it('does not render any payment-related checklist items', async () => {
    render(<GettingStartedPage />);

    const card = await screen.findByText('Go-live checklist');
    const checklist = card.closest('.border')!;
    // Regression guard: no label containing "payment" should appear
    const allLabels = within(checklist as HTMLElement).getAllByRole('checkbox');
    for (const label of allLabels) {
      const labelText = (label.closest('label') || label).textContent || '';
      expect(labelText.toLowerCase()).not.toContain('payment');
    }
  });

  it('runs backend ERP sync and updates progress', async () => {
    const user = userEvent.setup();
    const synced = makeStatus({
      percent_complete: 20,
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
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('20% complete');
  });

  it('selects an approval template through the implementation API', async () => {
    const user = userEvent.setup();
    const selected = makeStatus({
      percent_complete: 20,
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

  it('persists go-live cutover date toggle through the implementation API', async () => {
    const user = userEvent.setup();
    const updated = makeStatus({
      phases: {
        ...makeStatus().phases,
        go_live: {
          status: 'in_progress',
          checks: {
            confirm_cutover_date: true,
            forwarding_email_verified: false,
            sample_invoice_routed: false,
            notifications_acknowledged: false,
            privacy_mode_confirmed: false,
          },
        },
      },
    });
    apiMocks.updateChecklist.mockResolvedValueOnce(updated);

    render(<GettingStartedPage />);
    const card = (await screen.findByText('Go-live checklist')).closest('.border')!;
    await user.click(within(card as HTMLElement).getByLabelText('Confirm cutover date'));

    expect(apiMocks.updateChecklist).toHaveBeenCalledWith(updated.phases.go_live.checks);
  });

  it('renders derived go-live signals as read-only status indicators, not toggles', async () => {
    render(<GettingStartedPage />);

    const card = (await screen.findByText('Go-live checklist')).closest('.border')!;
    const goLiveSection = card as HTMLElement;

    // Derived items should NOT have checkboxes
    const checkboxes = within(goLiveSection).getAllByRole('checkbox');
    // Only the manual "Confirm cutover date" toggle should be a checkbox
    expect(checkboxes).toHaveLength(1);
    expect(checkboxes[0]).toHaveAccessibleName('Confirm cutover date');

    // Derived items should appear as read-only text
    expect(screen.getByText('Email forwarding verified')).toBeInTheDocument();
    expect(screen.getByText('Sample invoice routed end-to-end')).toBeInTheDocument();
    expect(screen.getByText('Notifications acknowledged')).toBeInTheDocument();
    expect(screen.getByText('Privacy mode confirmed')).toBeInTheDocument();
  });

  it('renders the Configuration phase card with privacy mode and capture channels', async () => {
    render(<GettingStartedPage />);

    const card = (await screen.findByText('Configuration')).closest('.border')!;
    const configSection = card as HTMLElement;

    expect(within(configSection).getByText('Privacy mode')).toBeInTheDocument();
    expect(within(configSection).getByText('Capture channels')).toBeInTheDocument();
    expect(within(configSection).getByText('Module entitlements')).toBeInTheDocument();
    expect(within(configSection).getByText('Notification approvals')).toBeInTheDocument();
  });
});
