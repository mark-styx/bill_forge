import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// --- Hoisted mock state so the vi.mock factory and the test body share refs ---
const { mockList, mockMarkRead, mockMarkAllRead, mockRemove, mockPush, mockReplace } = vi.hoisted(
  () => ({
    mockList: vi.fn(),
    mockMarkRead: vi.fn(),
    mockMarkAllRead: vi.fn(),
    mockRemove: vi.fn(),
    mockPush: vi.fn(),
    mockReplace: vi.fn(),
  }),
);

// --- Mocks ---

vi.mock('next/link', () => ({
  default: ({
    children,
    href,
    ...props
  }: {
    children: React.ReactNode;
    href: string;
    [key: string]: unknown;
  }) => (
    <a href={href} {...props}>
      {children}
    </a>
  ),
}));

vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush, replace: mockReplace }),
  usePathname: () => '/dashboard',
}));

vi.mock('@/stores/auth', () => {
  const store = {
    isAuthenticated: true,
    hasHydrated: true,
    user: { name: 'Test User', email: 'test@example.com', roles: [] },
    tenant: {
      enabled_modules: ['invoice_capture', 'invoice_processing', 'reporting'],
      settings: { company_name: 'TestCo' },
    },
    logout: vi.fn(),
    hasModule: (m: string) => true,
  };

  const useAuthStore = Object.assign(
    (selector?: (s: typeof store) => unknown) => {
      if (typeof selector === 'function') return selector(store);
      return store;
    },
    { getState: () => store, setState: vi.fn() },
  );

  return { useAuthStore };
});

vi.mock('@/stores/theme', () => ({
  useThemeStore: () => ({
    sidebarCollapsed: false,
    toggleSidebar: vi.fn(),
    getCurrentColors: () => ({
      background: '#fff',
      foreground: '#000',
      card: '#fff',
      border: '#ddd',
      primary: '#0066ff',
      muted: '#f5f5f5',
      secondary: '#eee',
    }),
  }),
  themePresets: [],
}));

vi.mock('@/components/organization-theme-provider', () => ({
  useOrganizationTheme: () => ({
    getBrandGradient: () => 'linear-gradient(135deg, #0066ff, #00ccff)',
  }),
}));

// Mock command palette so it doesn't pull in router/keyboard machinery.
vi.mock('@/components/ui/command-palette', () => ({
  CommandPalette: () => null,
  CommandPaletteTrigger: () => null,
}));

// Mock the API module: the bell's data source under test (refs #375).
vi.mock('@/lib/api', () => ({
  notificationsApi: {
    list: mockList,
    markRead: mockMarkRead,
    markAllRead: mockMarkAllRead,
    remove: mockRemove,
  },
}));

// jsdom does not implement scrollTo on elements.
beforeAll(() => {
  HTMLElement.prototype.scrollTo = vi.fn();
});

// Import AFTER mocks are registered so the layout picks up the mocked api.
import DashboardLayout from '../layout';

describe('NotificationCenter bell fetches real notifications', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Default to a single unread approval_request notification so the badge is
    // visible; individual tests override as needed.
    mockList.mockResolvedValue({
      items: [
        {
          id: 'notif-1',
          kind: 'approval_request',
          title: 'Approval needed: INV-123',
          message: 'Approval request abc is pending your review.',
          link: '/processing/approvals/abc',
          read: false,
          created_at: new Date().toISOString(),
        },
      ],
      unread_count: 1,
    });
    mockMarkRead.mockResolvedValue({ success: true });
    mockMarkAllRead.mockResolvedValue({ success: true });
    mockRemove.mockResolvedValue({ success: true });
  });

  it('renders the server-sourced unread count on the bell badge', async () => {
    render(
      <DashboardLayout>
        <div />
      </DashboardLayout>,
    );

    // The bell calls notificationsApi.list() on mount; wait for the unread badge.
    await waitFor(() => {
      expect(mockList).toHaveBeenCalledTimes(1);
    });

    // The badge text node renders the unread count (1 here). Use a function
    // matcher so we don't collide with other "1"s in the sidebar.
    await waitFor(() => {
      const badge = screen.getByText('1');
      expect(badge).toBeInTheDocument();
      expect(badge.className).toContain('rounded-full');
    });
  });

  it('keeps the bell empty (no badge) when the feed returns zero unread', async () => {
    mockList.mockResolvedValue({ items: [], unread_count: 0 });

    render(
      <DashboardLayout>
        <div />
      </DashboardLayout>,
    );

    await waitFor(() => {
      expect(mockList).toHaveBeenCalledTimes(1);
    });

    // No badge should render when unread_count is 0.
    expect(screen.queryByText('1')).not.toBeInTheDocument();
  });

  it('invokes notificationsApi.markRead when the user marks a notification read', async () => {
    const user = userEvent.setup();

    render(
      <DashboardLayout>
        <div />
      </DashboardLayout>,
    );

    // The unread badge (text "1") lives inside the bell trigger button, so we
    // can resolve the trigger robustly as the badge's closest button ancestor.
    const badge = await screen.findByText('1');
    const trigger = badge.closest('button')!;
    expect(trigger).toBeTruthy();
    await user.click(trigger);

    // After opening, the mark-as-read button (title="Mark as read") appears.
    const markReadButton = await screen.findByTitle('Mark as read');
    await user.click(markReadButton);

    await waitFor(() => {
      expect(mockMarkRead).toHaveBeenCalledWith('notif-1');
    });
  });

  it('does not crash when notificationsApi.list rejects (401/forbidden)', async () => {
    mockList.mockRejectedValue(new Error('API error 401'));

    render(
      <DashboardLayout>
        <div />
      </DashboardLayout>,
    );

    // The layout must still render (e.g. the sidebar company name) even though
    // the bell fetch failed. 'TestCo' appears in both the sidebar logo and the
    // user-section company name, so use getAllByText to avoid a multi-match.
    await waitFor(() => {
      expect(mockList).toHaveBeenCalledTimes(1);
    });
    expect(screen.getAllByText('TestCo').length).toBeGreaterThan(0);
    // No unread badge leaks through.
    expect(screen.queryByText('1')).not.toBeInTheDocument();
  });
});
