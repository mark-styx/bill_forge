import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import IntegrationsPage from '../page';
import {
  billComApi,
  getIntegrationStatus,
  notificationsApi,
  sageIntacctApi,
} from '@/lib/api';

vi.mock('@/lib/api', () => ({
  api: {
    post: vi.fn().mockResolvedValue({ success: true }),
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

describe('Integrations page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.pushState({}, '', '/integrations');
    vi.mocked(getIntegrationStatus).mockResolvedValue({ connected: false });
    vi.mocked(notificationsApi.getSlackStatus).mockResolvedValue(null);
    vi.mocked(notificationsApi.getTeamsStatus).mockResolvedValue(null);
  });

  it('opens and submits Sage Intacct credentials instead of showing a settings alert', async () => {
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

  it('opens and submits Bill.com credentials with the selected environment', async () => {
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
