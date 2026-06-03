import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import IntegrationsPage from '../page';
import {
  billingApi,
  billComApi,
  getIntegrationStatus,
  notificationsApi,
  sageIntacctApi,
} from '@/lib/api';

vi.mock('@/lib/api', () => ({
  api: {
    post: vi.fn().mockResolvedValue({ success: true }),
  },
  billingApi: {
    getSubscription: vi.fn().mockResolvedValue({
      subscription: {
        id: 'sub-1',
        tenant_id: 't-1',
        plan_id: 'starter',
        status: 'active',
        billing_cycle: 'monthly',
        add_on_modules: [],
        started_at: '2025-01-01T00:00:00Z',
        current_period_start: '2025-01-01T00:00:00Z',
        current_period_end: '2025-02-01T00:00:00Z',
        canceled_at: null,
        trial_end: null,
        stripe_subscription_id: null,
        stripe_customer_id: null,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    }),
  },
  getIntegrationStatus: vi.fn().mockResolvedValue({ connected: false }),
  sageIntacctApi: {
    connect: vi.fn().mockResolvedValue({ status: 'connected' }),
  },
  billComApi: {
    connect: vi.fn().mockResolvedValue({ status: 'connected' }),
  },
  notificationsApi: {
    getSlackStatus: vi.fn().mockResolvedValue(null),
    installSlack: vi.fn().mockResolvedValue({ authorize_url: 'https://slack.test/install', state: 'state-1' }),
    disconnectSlack: vi.fn().mockResolvedValue({ success: true }),
    getTeamsStatus: vi.fn().mockResolvedValue(null),
    configureTeams: vi.fn().mockResolvedValue({ success: true, webhook_id: 'teams-1' }),
    disconnectTeams: vi.fn().mockResolvedValue({ success: true }),
  },
}));

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn((selector) => {
    const state = { hasModule: vi.fn().mockReturnValue(false) };
    return selector ? selector(state) : state;
  }),
}));

describe('Integrations page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.pushState({}, '', '/integrations');
    vi.mocked(getIntegrationStatus).mockResolvedValue({ connected: false });
    vi.mocked(notificationsApi.getSlackStatus).mockResolvedValue(null);
    vi.mocked(notificationsApi.getTeamsStatus).mockResolvedValue(null);
    // Default: no paid add-ons, hasModule returns false
    vi.mocked(billingApi.getSubscription).mockResolvedValue({
      subscription: {
        id: 'sub-1',
        tenant_id: 't-1',
        plan_id: 'starter',
        status: 'active',
        billing_cycle: 'monthly',
        add_on_modules: [],
        started_at: '2025-01-01T00:00:00Z',
        current_period_start: '2025-01-01T00:00:00Z',
        current_period_end: '2025-02-01T00:00:00Z',
        canceled_at: null,
        trial_end: null,
        stripe_subscription_id: null,
        stripe_customer_id: null,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    });
  });

  it('renders paid integrations in a locked state when subscription lacks the add-on module', async () => {
    render(<IntegrationsPage />);

    // QuickBooks card should show "Upgrade required" badge
    const upgradeButtons = await screen.findAllByRole('button', { name: /upgrade required for/i });
    expect(upgradeButtons.length).toBeGreaterThanOrEqual(1);

    // QuickBooks card should show "Requires ... add-on" subtitle
    expect(await screen.findByText(/requires quickbooks online add-on/i)).toBeInTheDocument();

    // Clicking the upgrade button should NOT call any connect/OAuth endpoint
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /upgrade required for quickbooks/i }));

    // No OAuth redirect or connect call should happen
    expect(notificationsApi.installSlack).not.toHaveBeenCalled();
    expect(sageIntacctApi.connect).not.toHaveBeenCalled();
    expect(billComApi.connect).not.toHaveBeenCalled();
  });

  it('renders paid integrations as connectable when subscription includes the add-on module', async () => {
    vi.mocked(billingApi.getSubscription).mockResolvedValue({
      subscription: {
        id: 'sub-1',
        tenant_id: 't-1',
        plan_id: 'pro',
        status: 'active',
        billing_cycle: 'monthly',
        add_on_modules: ['quickbooks' as const, 'sage_intacct' as const, 'bill_com' as const],
        started_at: '2025-01-01T00:00:00Z',
        current_period_start: '2025-01-01T00:00:00Z',
        current_period_end: '2025-02-01T00:00:00Z',
        canceled_at: null,
        trial_end: null,
        stripe_subscription_id: null,
        stripe_customer_id: null,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    });

    render(<IntegrationsPage />);

    // QuickBooks should have a regular Connect button, not "Upgrade required"
    const connectButton = await screen.findByRole('button', { name: /connect quickbooks online/i });
    expect(connectButton).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /upgrade required for quickbooks/i })).not.toBeInTheDocument();
  });

  it('opens and submits Sage Intacct credentials when subscription includes the add-on', async () => {
    vi.mocked(billingApi.getSubscription).mockResolvedValue({
      subscription: {
        id: 'sub-1',
        tenant_id: 't-1',
        plan_id: 'pro',
        status: 'active',
        billing_cycle: 'monthly',
        add_on_modules: ['sage_intacct' as const],
        started_at: '2025-01-01T00:00:00Z',
        current_period_start: '2025-01-01T00:00:00Z',
        current_period_end: '2025-02-01T00:00:00Z',
        canceled_at: null,
        trial_end: null,
        stripe_subscription_id: null,
        stripe_customer_id: null,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    });

    const user = userEvent.setup();
    render(<IntegrationsPage />);

    await user.click(await screen.findByRole('button', { name: /connect sage intacct/i }));

    const dialog = screen.getByRole('dialog', { name: /connect sage intacct/i });
    await user.type(within(dialog).getByLabelText(/sender id/i), 'sender-1');
    await user.type(within(dialog).getByLabelText(/sender password/i), 'sender-secret');
    await user.type(within(dialog).getByLabelText(/company id/i), 'company-1');
    await user.type(within(dialog).getByLabelText(/entity id/i), 'entity-1');
    await user.type(within(dialog).getByLabelText(/^user id$/i), 'user-1');
    await user.type(within(dialog).getByLabelText(/user password/i), 'user-secret');
    await user.click(within(dialog).getByRole('button', { name: /^connect$/i }));

    await waitFor(() => {
      expect(sageIntacctApi.connect).toHaveBeenCalledWith({
        sender_id: 'sender-1',
        sender_password: 'sender-secret',
        company_id: 'company-1',
        entity_id: 'entity-1',
        user_id: 'user-1',
        user_password: 'user-secret',
      });
    });
    expect(screen.queryByText(/settings -> integrations/i)).not.toBeInTheDocument();
  });

  it('opens and submits Bill.com credentials when subscription includes the add-on', async () => {
    vi.mocked(billingApi.getSubscription).mockResolvedValue({
      subscription: {
        id: 'sub-1',
        tenant_id: 't-1',
        plan_id: 'pro',
        status: 'active',
        billing_cycle: 'monthly',
        add_on_modules: ['bill_com' as const],
        started_at: '2025-01-01T00:00:00Z',
        current_period_start: '2025-01-01T00:00:00Z',
        current_period_end: '2025-02-01T00:00:00Z',
        canceled_at: null,
        trial_end: null,
        stripe_subscription_id: null,
        stripe_customer_id: null,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    });

    const user = userEvent.setup();
    render(<IntegrationsPage />);

    await user.click(await screen.findByRole('button', { name: /connect bill\.com/i }));

    const dialog = screen.getByRole('dialog', { name: /connect bill\.com/i });
    await user.type(within(dialog).getByLabelText(/developer key/i), 'dev-key');
    await user.type(within(dialog).getByLabelText(/organization id/i), 'org-1');
    await user.type(within(dialog).getByLabelText(/user name/i), 'finance@example.com');
    await user.type(within(dialog).getByLabelText(/^password$/i), 'bill-secret');
    await user.selectOptions(within(dialog).getByLabelText(/environment/i), 'production');
    await user.click(within(dialog).getByRole('button', { name: /^connect$/i }));

    await waitFor(() => {
      expect(billComApi.connect).toHaveBeenCalledWith({
        dev_key: 'dev-key',
        org_id: 'org-1',
        user_name: 'finance@example.com',
        password: 'bill-secret',
        environment: 'production',
      });
    });
  });

  it('opens directly to notification integrations from the category query param', async () => {
    window.history.pushState({}, '', '/integrations?category=notifications');

    render(<IntegrationsPage />);

    expect(await screen.findByText('Slack')).toBeInTheDocument();
    expect(screen.getByText('Microsoft Teams')).toBeInTheDocument();
    expect(screen.queryByText('QuickBooks Online')).not.toBeInTheDocument();
  });

  it('starts Slack installation from the notifications card', async () => {
    const user = userEvent.setup();
    render(<IntegrationsPage />);

    await user.click(await screen.findByRole('button', { name: /connect slack/i }));

    await waitFor(() => {
      expect(notificationsApi.installSlack).toHaveBeenCalledWith(expect.stringContaining('/integrations?category=notifications'));
    });
  });

  it('configures Teams through the webhook modal', async () => {
    const user = userEvent.setup();
    render(<IntegrationsPage />);

    await user.click(await screen.findByRole('button', { name: /connect microsoft teams/i }));

    const dialog = screen.getByRole('dialog', { name: /configure microsoft teams/i });
    await user.type(within(dialog).getByLabelText(/webhook url/i), 'https://outlook.office.com/webhook/test');
    await user.type(within(dialog).getByLabelText(/channel name/i), 'AP Approvals');
    await user.click(within(dialog).getByRole('button', { name: /^save$/i }));

    await waitFor(() => {
      expect(notificationsApi.configureTeams).toHaveBeenCalledWith({
        webhook_url: 'https://outlook.office.com/webhook/test',
        channel_name: 'AP Approvals',
      });
    });
  });
});
