import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import HomePage from '../page';

const { authState, replaceMock } = vi.hoisted(() => ({
  authState: {
    isAuthenticated: false,
    hasHydrated: true,
  },
  replaceMock: vi.fn(),
}));

vi.mock('next/navigation', () => ({
  useRouter: () => ({ replace: replaceMock }),
}));

vi.mock('@/stores/auth', () => ({
  useAuthStore: (selector: (state: typeof authState) => unknown) => selector(authState),
}));

describe('root routing', () => {
  beforeEach(() => {
    replaceMock.mockClear();
    authState.isAuthenticated = false;
    authState.hasHydrated = true;
  });

  it('routes unauthenticated users to the marketing home page', async () => {
    render(<HomePage />);

    await waitFor(() => {
      expect(replaceMock).toHaveBeenCalledWith('/home');
    });
  });

  it('routes authenticated users to the dashboard', async () => {
    authState.isAuthenticated = true;

    render(<HomePage />);

    await waitFor(() => {
      expect(replaceMock).toHaveBeenCalledWith('/dashboard');
    });
  });

  it('waits for auth hydration before routing', () => {
    authState.hasHydrated = false;

    render(<HomePage />);

    expect(replaceMock).not.toHaveBeenCalled();
  });
});
