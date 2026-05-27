import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import LoginPage from '../page';

const { loginMock, pushMock, toastSuccessMock, toastErrorMock } = vi.hoisted(() => ({
  loginMock: vi.fn(),
  pushMock: vi.fn(),
  toastSuccessMock: vi.fn(),
  toastErrorMock: vi.fn(),
}));

vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: pushMock }),
}));

vi.mock('@/stores/auth', () => ({
  useAuthStore: (selector: (state: { login: typeof loginMock; isLoading: boolean }) => unknown) =>
    selector({ login: loginMock, isLoading: false }),
}));

vi.mock('sonner', () => ({
  toast: {
    success: toastSuccessMock,
    error: toastErrorMock,
  },
}));

describe('LoginPage demo account', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    loginMock.mockResolvedValue(undefined);
  });

  it('signs into the seeded sandbox from the demo account button', async () => {
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.click(screen.getByRole('button', { name: /use demo account/i }));

    await waitFor(() => {
      expect(loginMock).toHaveBeenCalledWith(
        '11111111-1111-1111-1111-111111111111',
        'admin@sandbox.local',
        'sandbox123',
      );
    });
    expect(toastSuccessMock).toHaveBeenCalledWith('Welcome back!');
    expect(pushMock).toHaveBeenCalledWith('/dashboard');
  });

  it('keeps the normal email and password submit path working', async () => {
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.clear(screen.getByPlaceholderText('name@company.com'));
    await user.type(screen.getByPlaceholderText('name@company.com'), 'ap@example.com');
    await user.clear(screen.getByPlaceholderText('••••••••'));
    await user.type(screen.getByPlaceholderText('••••••••'), 'password123');
    await user.click(screen.getByRole('button', { name: /^sign in$/i }));

    await waitFor(() => {
      expect(loginMock).toHaveBeenCalledWith(
        '11111111-1111-1111-1111-111111111111',
        'ap@example.com',
        'password123',
      );
    });
  });
});
