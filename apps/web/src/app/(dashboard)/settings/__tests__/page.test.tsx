import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import SettingsPage from '../page';
import { notificationsApi } from '@/lib/api';

// Mock API
vi.mock('@/lib/api', () => ({
  api: {
    put: vi.fn(),
    get: vi.fn().mockResolvedValue({ usage: { invoices_count: 0, vendor_count: 0, user_count: 0 }, plan_id: 'free' }),
  },
  billingApi: {
    getSubscription: vi.fn().mockResolvedValue({ subscription: { plan_id: 'starter', status: 'active', add_on_modules: [] } }),
  },
  invoiceStatusApi: {
    list: vi.fn().mockResolvedValue([]),
    update: vi.fn(),
    seedDefaults: vi.fn(),
  },
  notificationsApi: {
    getPreferences: vi.fn().mockResolvedValue([
      { channel: 'invoice_received', enabled: true },
      { channel: 'approval_required', enabled: true },
      { channel: 'weekly_digest', enabled: false },
    ]),
    updatePreferences: vi.fn().mockResolvedValue({ success: true }),
  },
  InvoiceStatusConfigInput: undefined,
  NotificationPreference: undefined,
  UpdateNotificationPreferencesInput: undefined,
}));

// Mock stores
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    user: { name: 'Jane Doe', id: 'u1', tenant_id: 't1', email: 'jane@example.com', roles: ['admin'] },
    tenant: {
      id: 't1',
      name: 'Test Org',
      enabled_modules: [],
      settings: { company_name: 'Test Org', timezone: 'UTC', default_currency: 'USD' },
    },
    refreshTenantContext: vi.fn(),
  })),
}));

vi.mock('@/stores/theme', () => ({
  useThemeStore: vi.fn(() => ({
    mode: 'light',
    setMode: vi.fn(),
    presetId: 'ocean',
    setPreset: vi.fn(),
    customColors: null,
    setCustomColors: vi.fn(),
    clearCustomColors: vi.fn(),
    organizationTheme: null,
    isOrgThemeActive: false,
    setOrganizationTheme: vi.fn(),
    clearOrganizationTheme: vi.fn(),
    updateOrganizationTheme: vi.fn(),
  })),
  themePresets: [
    { id: 'ocean', name: 'Ocean', description: 'test', category: 'bright', colors: { primary: '210 100% 50%', accent: '190 95% 45%', capture: '195 100% 45%', processing: '160 84% 39%', vendor: '270 70% 55%', reporting: '35 95% 55%' } },
  ],
  ThemeColors: undefined,
  generateGradient: vi.fn(() => 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)'),
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('@/components/ui/color-picker', () => ({
  ColorPicker: () => null,
  ColorSwatch: () => null,
}));

function createQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false } },
    logger: { log: console.log, warn: console.warn, error: () => {} },
  });
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      {ui}
    </QueryClientProvider>,
  );
}

describe('Settings page - non-persistent controls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Profile tab', () => {
    it('renders name fields as read-only', async () => {
      renderWithProviders(<SettingsPage />);

      // Click Profile tab
      const profileTab = screen.getByText('Profile');
      await userEvent.click(profileTab);

      // Name fields should be read-only
      const firstNameInput = screen.getByDisplayValue('Jane') as HTMLInputElement;
      const lastNameInput = screen.getByDisplayValue('Doe') as HTMLInputElement;

      expect(firstNameInput.readOnly).toBe(true);
      expect(lastNameInput.readOnly).toBe(true);
    });

    it('disables the Save Profile button', async () => {
      renderWithProviders(<SettingsPage />);

      const profileTab = screen.getByText('Profile');
      await userEvent.click(profileTab);

      const saveButton = screen.getByRole('button', { name: /save profile/i });
      expect(saveButton).toBeDisabled();
    });

    it('disables the Change Avatar button', async () => {
      renderWithProviders(<SettingsPage />);

      const profileTab = screen.getByText('Profile');
      await userEvent.click(profileTab);

      const avatarButton = screen.getByRole('button', { name: /change avatar/i });
      expect(avatarButton).toBeDisabled();
    });
  });

  describe('Security tab', () => {
    it('disables password fields and Update Password button', async () => {
      renderWithProviders(<SettingsPage />);

      const securityTab = screen.getByText('Security');
      await userEvent.click(securityTab);

      const passwordFields = screen.getAllByPlaceholderText(/password/i);
      for (const field of passwordFields) {
        expect((field as HTMLInputElement).disabled).toBe(true);
      }

      const updateButton = screen.getByRole('button', { name: /update password/i });
      expect(updateButton).toBeDisabled();
    });

    it('disables the Enable 2FA button', async () => {
      renderWithProviders(<SettingsPage />);

      const securityTab = screen.getByText('Security');
      await userEvent.click(securityTab);

      const enable2fa = screen.getByRole('button', { name: /enable 2fa/i });
      expect(enable2fa).toBeDisabled();
    });

    it('shows coming-soon messages for security features', async () => {
      renderWithProviders(<SettingsPage />);

      const securityTab = screen.getByText('Security');
      await userEvent.click(securityTab);

      expect(screen.getByText(/password management is coming soon/i)).toBeInTheDocument();
      expect(screen.getByText(/two-factor authentication is coming soon/i)).toBeInTheDocument();
    });
  });

  describe('Notifications tab', () => {
    it('loads preferences from the API', async () => {
      renderWithProviders(<SettingsPage />);

      const notificationsTab = screen.getByText('Notifications');
      await userEvent.click(notificationsTab);

      // Wait for preferences to load
      await waitFor(() => {
        expect(notificationsApi.getPreferences).toHaveBeenCalled();
      });

      // The toggles should be rendered
      const toggles = screen.getAllByRole('checkbox');
      expect(toggles.length).toBe(5);
    });

    it('calls updatePreferences when a toggle is clicked', async () => {
      renderWithProviders(<SettingsPage />);

      const notificationsTab = screen.getByText('Notifications');
      await userEvent.click(notificationsTab);

      // Wait for load
      await waitFor(() => {
        expect(notificationsApi.getPreferences).toHaveBeenCalled();
      });

      // Clear previous calls
      vi.mocked(notificationsApi.updatePreferences).mockClear();

      // Find the "Weekly digest" toggle (last one, enabled=false in mock data)
      const toggles = screen.getAllByRole('checkbox');
      const weeklyDigestToggle = toggles[4];
      await userEvent.click(weeklyDigestToggle);

      await waitFor(() => {
        expect(notificationsApi.updatePreferences).toHaveBeenCalledWith(
          expect.objectContaining({
            channel: 'weekly_digest',
          }),
        );
      });
    });
  });
});
