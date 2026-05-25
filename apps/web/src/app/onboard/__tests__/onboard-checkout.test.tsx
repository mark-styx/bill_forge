import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import OnboardPage from '../page';

// Mock API module
const mockProvision = vi.fn();
const mockCreateCheckout = vi.fn();
vi.mock('@/lib/api', () => ({
  authApi: {
    provision: (...args: unknown[]) => mockProvision(...args),
  },
  billingApi: {
    createCheckout: (...args: unknown[]) => mockCreateCheckout(...args),
  },
  api: {
    setToken: vi.fn(),
    setRefreshToken: vi.fn(),
  },
}));

// Mock auth store
vi.mock('@/stores/auth', () => ({
  useAuthStore: Object.assign(
    (selector: (state: Record<string, unknown>) => unknown) => selector({ login: vi.fn() }),
    { setState: vi.fn() }
  ),
  setupApiCallbacks: vi.fn(),
}));

// Mock next/navigation
const mockPush = vi.fn();
let mockSearchParams: Record<string, string> = {};
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush }),
  useSearchParams: () => new URLSearchParams(mockSearchParams),
}));

// Mock sonner
vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

function provisionResponse() {
  return {
    access_token: 'at',
    refresh_token: 'rt',
    user: { id: 'u1', email: 'test@test.com', name: 'Test', roles: ['admin'] },
    tenant: { id: 't1', name: 'TestCo', enabled_modules: [], settings: {} },
  };
}

describe('OnboardPage checkout wiring', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSearchParams = {};
  });

  it('calls billingApi.createCheckout with plan_id when plan param is set', async () => {
    const user = userEvent.setup();
    mockSearchParams = { plan: 'starter' };

    mockProvision.mockResolvedValue(provisionResponse());
    mockCreateCheckout.mockResolvedValue({
      mode: 'mock',
      url: '/dashboard?checkout=mock',
    });

    render(<OnboardPage />);

    // Fill company name (step 0)
    const companyInput = screen.getByPlaceholderText('Acme Corporation');
    await user.type(companyInput, 'TestCo');
    await user.click(screen.getByText('Next'));

    // Fill admin fields (step 1)
    await user.type(screen.getByPlaceholderText('Jane Smith'), 'Test User');
    await user.type(screen.getByPlaceholderText('jane@acme.com'), 'test@test.com');
    await user.type(screen.getByPlaceholderText('Min 8 characters'), 'password1');
    await user.type(screen.getByPlaceholderText('Confirm password'), 'password1');
    await user.click(screen.getByText('Next'));

    // Skip OCR (step 2)
    await user.click(screen.getByText('Next'));

    // Skip ERP (step 3)
    await user.click(screen.getByText('Next'));

    // Step 4 - Launch
    await user.click(screen.getByText('Launch BillForge'));

    await waitFor(() => {
      expect(mockProvision).toHaveBeenCalled();
    });

    await waitFor(() => {
      expect(mockCreateCheckout).toHaveBeenCalledWith({ plan_id: 'starter' });
    });

    // Should navigate to the mock URL
    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/dashboard?checkout=mock');
    });
  });

  it('falls back to dashboard when checkout fails', async () => {
    const user = userEvent.setup();
    mockSearchParams = { plan: 'starter' };

    mockProvision.mockResolvedValue(provisionResponse());
    mockCreateCheckout.mockRejectedValue(new Error('checkout failed'));

    render(<OnboardPage />);

    await user.type(screen.getByPlaceholderText('Acme Corporation'), 'TestCo');
    await user.click(screen.getByText('Next'));

    await user.type(screen.getByPlaceholderText('Jane Smith'), 'Test User');
    await user.type(screen.getByPlaceholderText('jane@acme.com'), 'test@test.com');
    await user.type(screen.getByPlaceholderText('Min 8 characters'), 'password1');
    await user.type(screen.getByPlaceholderText('Confirm password'), 'password1');
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Launch BillForge'));

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/dashboard');
    });
  });

  it('navigates to dashboard without checkout when no plan param', async () => {
    const user = userEvent.setup();

    mockProvision.mockResolvedValue(provisionResponse());

    render(<OnboardPage />);

    await user.type(screen.getByPlaceholderText('Acme Corporation'), 'TestCo');
    await user.click(screen.getByText('Next'));

    await user.type(screen.getByPlaceholderText('Jane Smith'), 'Test User');
    await user.type(screen.getByPlaceholderText('jane@acme.com'), 'test@test.com');
    await user.type(screen.getByPlaceholderText('Min 8 characters'), 'password1');
    await user.type(screen.getByPlaceholderText('Confirm password'), 'password1');
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Next'));
    await user.click(screen.getByText('Launch BillForge'));

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/dashboard');
    });

    expect(mockCreateCheckout).not.toHaveBeenCalled();
  });
});
