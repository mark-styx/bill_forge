import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';
import { render, screen } from '@testing-library/react';

// --- Hoisted state (available inside vi.mock factories) ---
const { _state, mockReplace, mockPush } = vi.hoisted(() => ({
  _state: { modules: [] as string[] },
  mockReplace: vi.fn(),
  mockPush: vi.fn(),
}));

// --- Hoisted mocks ---

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
    tenant: { enabled_modules: _state.modules, settings: { company_name: 'TestCo' } },
    logout: vi.fn(),
    hasModule: (m: string) => _state.modules.includes(m),
  };

  const useAuthStore = Object.assign(
    (selector?: (s: typeof store) => unknown) => {
      if (typeof selector === 'function') {
        return selector(store);
      }
      return store;
    },
    {
      getState: () => store,
      setState: vi.fn(),
    },
  );

  return { useAuthStore };
});

// Mock @/stores/theme
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

// Mock organization theme provider
vi.mock('@/components/organization-theme-provider', () => ({
  useOrganizationTheme: () => ({
    getBrandGradient: () => 'linear-gradient(135deg, #0066ff, #00ccff)',
  }),
}));

// Mock notification center
vi.mock('@/components/ui/notification-center', () => ({
  NotificationCenter: () => null,
}));

// Mock command palette
vi.mock('@/components/ui/command-palette', () => ({
  CommandPalette: () => null,
  CommandPaletteTrigger: () => null,
}));

// Mock @tanstack/react-query
vi.mock('@tanstack/react-query', () => ({
  useMutation: () => ({ mutate: vi.fn(), isPending: false }),
}));

// Mock @/lib/api
vi.mock('@/lib/api', () => ({
  aiAssistantApi: { chat: vi.fn() },
}));

// Mock sonner
vi.mock('sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

// jsdom does not implement scrollTo on elements
beforeAll(() => {
  HTMLElement.prototype.scrollTo = vi.fn();
});

// --- Imports that depend on the mocks above ---
import DashboardLayout from '../layout';
import AiAssistantPage from '../ai-assistant/page';

describe('AI Assistant gating', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    _state.modules = [];
  });

  describe('Sidebar navigation', () => {
    it('hides Winston AI Assistant when tenant lacks ai_assistant module', () => {
      _state.modules = ['invoice_capture', 'reporting'];

      render(
        <DashboardLayout>
          <div />
        </DashboardLayout>,
      );

      expect(screen.queryByText('Winston AI Assistant')).not.toBeInTheDocument();
    });

    it('shows Winston AI Assistant sidebar entry when tenant has ai_assistant module', () => {
      _state.modules = ['invoice_capture', 'reporting', 'ai_assistant'];

      render(
        <DashboardLayout>
          <div />
        </DashboardLayout>,
      );

      const link = screen.getByText('Winston AI Assistant').closest('a');
      expect(link).toBeInTheDocument();
      expect(link?.getAttribute('href')).toBe('/ai-assistant');
    });
  });

  describe('AiAssistantPage direct access', () => {
    it('redirects to /dashboard and renders no assistant UI when module is absent', () => {
      _state.modules = ['invoice_capture'];

      render(<AiAssistantPage />);

      expect(mockReplace).toHaveBeenCalledWith('/dashboard');
      expect(screen.queryByText('Winston AI Assistant')).not.toBeInTheDocument();
      expect(screen.queryByPlaceholderText('Type your message…')).not.toBeInTheDocument();
    });

    it('renders Winston heading and input when module is present', () => {
      _state.modules = ['ai_assistant'];

      render(<AiAssistantPage />);

      expect(screen.getByText('Winston AI Assistant')).toBeInTheDocument();
      expect(screen.getByPlaceholderText('Type your message…')).toBeInTheDocument();
      expect(mockReplace).not.toHaveBeenCalled();
    });
  });
});
