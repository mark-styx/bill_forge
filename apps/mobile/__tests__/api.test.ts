/**
 * Unit tests for the BillForge API client (lib/api.ts).
 *
 * Mocks global fetch and tests all endpoint methods including
 * auth header attachment, request/response shape, and error handling.
 */

// Mock expo-secure-store before importing api (which registers a token getter)
jest.mock('expo-secure-store', () => ({}));

import {
  api,
  ApiClientError,
  registerTokenGetter,
  registerRefreshTokenGetter,
  registerTokenSetter,
  registerLogoutHandler,
} from '../lib/api';

// ---- Test helpers ----

let mockToken: string | null = null;
let mockRefreshToken: string | null = null;
let logoutCalled = false;

// Register test callbacks
registerTokenGetter(async () => mockToken);
registerRefreshTokenGetter(async () => mockRefreshToken);
registerTokenSetter(async (access, refresh) => {
  mockToken = access;
  mockRefreshToken = refresh;
});
registerLogoutHandler(async () => {
  logoutCalled = true;
  mockToken = null;
  mockRefreshToken = null;
});

function mockFetchResponse(status: number, body: unknown): void {
  (global.fetch as jest.Mock).mockResolvedValueOnce({
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  });
}

beforeEach(() => {
  jest.clearAllMocks();
  mockToken = null;
  mockRefreshToken = null;
  logoutCalled = false;
  (global.fetch as jest.Mock) = jest.fn();
});

// ---- Tests ----

describe('api.login', () => {
  it('posts tenant_id, email, password and stores returned token', async () => {
    const authResponse = {
      access_token: 'jwt-token-123',
      refresh_token: 'refresh-456',
      user: { id: 'user-1', tenant_id: 'tenant-1', email: 'a@b.com', name: 'Test', roles: ['approver'] },
      tenant: { id: 'tenant-1', name: 'Test Co', enabled_modules: [], settings: {} },
    };
    mockFetchResponse(200, authResponse);

    const result = await api.login('tenant-1', 'a@b.com', 'password123');

    expect(result.access_token).toBe('jwt-token-123');
    expect(global.fetch).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/auth/login'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ tenant_id: 'tenant-1', email: 'a@b.com', password: 'password123' }),
      }),
    );
    // Login should NOT send an auth header
    const call = (global.fetch as jest.Mock).mock.calls[0];
    const headers = call[1].headers;
    expect(headers['Authorization']).toBeUndefined();
  });
});

describe('api.getApprovals', () => {
  it('sends Authorization header when token is set', async () => {
    mockToken = 'test-jwt';
    mockFetchResponse(200, []);

    await api.getApprovals();

    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(call[1].headers['Authorization']).toBe('Bearer test-jwt');
    expect(call[0]).toContain('/api/v1/mobile/approvals');
  });

  it('returns parsed approval list', async () => {
    mockToken = 'tok';
    const approvals = [
      {
        id: 'apr-1',
        invoice: {
          id: 'inv-1',
          vendor_name: 'Acme',
          invoice_number: 'INV-001',
          total_amount_cents: 150000,
          currency: 'USD',
          due_date: '2026-04-10',
          status: 'pending_approval',
          days_until_due: 6,
          requires_action: true,
          created_at: '2026-04-01T00:00:00Z',
        },
        requested_at: '2026-04-01T00:00:00Z',
        expires_at: null,
        can_approve: true,
      },
    ];
    mockFetchResponse(200, approvals);

    const result = await api.getApprovals();
    expect(result).toHaveLength(1);
    expect(result[0].invoice.vendor_name).toBe('Acme');
  });
});

describe('api.approveInvoice', () => {
  it('sends correct body with optional comment', async () => {
    mockToken = 'tok';
    mockFetchResponse(200, { success: true });

    await api.approveInvoice('apr-1', 'Looks good');

    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(call[0]).toContain('/api/v1/mobile/approvals/apr-1/approve');
    expect(call[1].method).toBe('POST');
    expect(call[1].body).toBe(JSON.stringify({ comment: 'Looks good' }));
  });

  it('sends null comment when omitted', async () => {
    mockToken = 'tok';
    mockFetchResponse(200, { success: true });

    await api.approveInvoice('apr-1');

    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(call[1].body).toBe(JSON.stringify({ comment: null }));
  });
});

describe('api.rejectInvoice', () => {
  it('sends reason in request body', async () => {
    mockToken = 'tok';
    mockFetchResponse(200, { success: true });

    await api.rejectInvoice('apr-1', 'Incorrect amount');

    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(call[0]).toContain('/api/v1/mobile/approvals/apr-1/reject');
    expect(call[1].body).toBe(JSON.stringify({ reason: 'Incorrect amount' }));
  });
});

describe('api.registerDevice', () => {
  it('sends platform and token', async () => {
    mockToken = 'tok';
    mockFetchResponse(200, {
      id: 'dev-1',
      device_id: 'xyz',
      platform: 'ios',
      device_name: 'iPhone',
      is_active: true,
      last_used_at: null,
      created_at: '2026-04-01T00:00:00Z',
    });

    const result = await api.registerDevice({
      device_id: 'xyz',
      platform: 'ios',
      token: 'expo-push-token',
      device_name: 'iPhone 15',
    });

    expect(result.platform).toBe('ios');
    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(JSON.parse(call[1].body).platform).toBe('ios');
    expect(JSON.parse(call[1].body).token).toBe('expo-push-token');
  });
});

describe('api.deltaSync', () => {
  it('passes lastSyncAt as query parameter', async () => {
    mockToken = 'tok';
    mockFetchResponse(200, []);

    await api.deltaSync('2026-04-01T00:00:00Z');

    const call = (global.fetch as jest.Mock).mock.calls[0];
    expect(call[0]).toContain('last_sync_at=2026-04-01T00%3A00%3A00Z');
  });
});

describe('error handling', () => {
  it('throws ApiClientError with parsed body on non-401 error response', async () => {
    mockToken = 'tok';
    mockFetchResponse(403, {
      error: { code: 'forbidden', message: 'Access denied' },
    });

    try {
      await api.getApprovals();
      fail('Expected error');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(403);
      expect(apiErr.body?.error?.code).toBe('forbidden');
      expect(apiErr.code).toBe('forbidden');
      expect(apiErr.message).toBe('Access denied');
    }
  });

  it('handles non-JSON error responses', async () => {
    mockToken = 'tok';
    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: false,
      status: 502,
      json: async () => { throw new Error('not json'); },
    });

    try {
      await api.getDashboard();
      fail('Expected error');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      expect((err as ApiClientError).status).toBe(502);
      expect((err as ApiClientError).body).toBeNull();
    }
  });

  it('exposes code and fieldErrors from error body', async () => {
    mockToken = 'tok';
    mockFetchResponse(422, {
      error: {
        code: 'VALIDATION_ERROR',
        message: 'Validation failed',
        field_errors: { amount: ['must be positive'], currency: ['is required'] },
      },
    });

    try {
      await api.approveInvoice('inv-1');
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.code).toBe('VALIDATION_ERROR');
      expect(apiErr.fieldErrors).toEqual({
        amount: ['must be positive'],
        currency: ['is required'],
      });
    }
  });

  it('defaults code to UNKNOWN when absent', async () => {
    mockToken = 'tok';
    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: async () => { throw new Error('no json'); },
    });

    try {
      await api.getDashboard();
      fail('Expected error');
    } catch (err) {
      expect((err as ApiClientError).code).toBe('UNKNOWN');
      expect((err as ApiClientError).fieldErrors).toBeUndefined();
    }
  });
});

describe('token refresh on 401', () => {
  it('retries request after successful token refresh', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = 'valid-refresh';

    // First call returns 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Token expired' } });
    // Refresh call succeeds
    mockFetchResponse(200, { access_token: 'new-tok', refresh_token: 'new-refresh' });
    // Retry succeeds
    mockFetchResponse(200, []);

    const result = await api.getApprovals();

    expect(result).toEqual([]);
    expect(global.fetch).toHaveBeenCalledTimes(3);
    // Verify refresh was called
    const refreshCall = (global.fetch as jest.Mock).mock.calls[1];
    expect(refreshCall[0]).toContain('/api/v1/auth/refresh');
    expect(JSON.parse(refreshCall[1].body).refresh_token).toBe('valid-refresh');
    // Verify tokens were updated
    expect(mockToken).toBe('new-tok');
    expect(mockRefreshToken).toBe('new-refresh');
  });

  it('triggers logout when refresh token is invalid (terminal failure)', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = 'bad-refresh';

    // First call returns 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Token expired' } });
    // Refresh call returns 401 (invalid refresh token)
    mockFetchResponse(401, { error: { code: 'invalid_token', message: 'Refresh token invalid' } });

    try {
      await api.getApprovals();
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(401);
      expect(apiErr.code).toBe('SESSION_EXPIRED');
      expect(apiErr.message).toBe('Session expired. Please login again.');
    }

    expect(logoutCalled).toBe(true);
    expect(global.fetch).toHaveBeenCalledTimes(2);
  });

  it('does not logout on transient refresh failure (5xx)', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = 'valid-refresh';

    // First call returns 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Token expired' } });
    // Refresh call returns 500 (server error, transient)
    mockFetchResponse(500, { error: { code: 'internal', message: 'Server error' } });

    try {
      await api.getApprovals();
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(401);
      // Should get the original 401, not SESSION_EXPIRED
      expect(apiErr.body?.error?.code).toBe('unauthorized');
    }

    // Should NOT have logged out
    expect(logoutCalled).toBe(false);
    expect(global.fetch).toHaveBeenCalledTimes(2);
  });

  it('does not logout on network error during refresh', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = 'valid-refresh';

    // First call returns 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Token expired' } });
    // Refresh call throws network error
    (global.fetch as jest.Mock).mockRejectedValueOnce(new Error('Network request failed'));

    try {
      await api.getApprovals();
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(401);
    }

    expect(logoutCalled).toBe(false);
    expect(global.fetch).toHaveBeenCalledTimes(2);
  });

  it('does not attempt refresh when no refresh token is available', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = null;

    // First call returns 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Token expired' } });

    try {
      await api.getApprovals();
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(401);
      expect(apiErr.code).toBe('SESSION_EXPIRED');
    }

    expect(logoutCalled).toBe(true);
    expect(global.fetch).toHaveBeenCalledTimes(1);
  });

  it('does not attempt refresh for login requests', async () => {
    mockToken = null;
    mockRefreshToken = 'some-refresh';

    // Login returns 401 (wrong credentials)
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'Bad credentials' } });

    try {
      await api.login('tenant-1', 'bad@email.com', 'wrong');
      fail('Expected error');
    } catch (err) {
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(401);
      expect(apiErr.code).toBe('unauthorized');
      expect(apiErr.message).toBe('Bad credentials');
    }

    // Should NOT have tried to refresh or called logout
    expect(logoutCalled).toBe(false);
    expect(global.fetch).toHaveBeenCalledTimes(1);
  });

  it('coalesces concurrent refresh attempts', async () => {
    mockToken = 'expired-tok';
    mockRefreshToken = 'valid-refresh';

    // Both calls return 401
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'expired' } });
    mockFetchResponse(401, { error: { code: 'unauthorized', message: 'expired' } });
    // Single refresh call succeeds
    mockFetchResponse(200, { access_token: 'new-tok', refresh_token: 'new-refresh' });
    // Both retries succeed
    mockFetchResponse(200, []);
    mockFetchResponse(200, { pending_approvals: 0, pending_review: 0, requires_attention: 0, upcoming_due_dates: [], recent_activity: [] });

    const [approvals, dashboard] = await Promise.all([
      api.getApprovals(),
      api.getDashboard(),
    ]);

    expect(approvals).toEqual([]);
    expect(dashboard.pending_approvals).toBe(0);
    // 2 initial + 1 refresh + 2 retries = 5 calls
    expect(global.fetch).toHaveBeenCalledTimes(5);
  });
});
