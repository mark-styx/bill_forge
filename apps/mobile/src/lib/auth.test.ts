import {
  login,
  loginWithTenant,
  loadAuth,
  saveAuth,
  clearAuth,
  AuthState,
} from './auth';
import { KVStore } from './offline-queue';

function createStore(): KVStore {
  const map = new Map<string, string>();
  return {
    async getItem(key: string) {
      return map.get(key) ?? null;
    },
    async setItem(key: string, value: string) {
      map.set(key, value);
    },
  };
}

const sampleState: AuthState = {
  jwt: 'jwt-token-abc',
  tenantId: 'tenant-1',
  userId: 'user-1',
  email: 'approver@example.com',
  issuedAt: '2026-05-28T20:00:00.000Z',
};

describe('auth', () => {
  describe('login', () => {
    it('returns logged_in on single-tenant response', async () => {
      const originalFetch = global.fetch;
      global.fetch = async () =>
        new Response(
          JSON.stringify({
            jwt: 'jwt-1',
            tenant_id: 't-1',
            user_id: 'u-1',
          }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );

      try {
        const result = await login('http://localhost:8080', {
          email: 'a@b.com',
          password: 'secret',
        });
        expect(result.kind).toBe('logged_in');
        if (result.kind === 'logged_in') {
          expect(result.state.jwt).toBe('jwt-1');
          expect(result.state.tenantId).toBe('t-1');
          expect(result.state.userId).toBe('u-1');
        }
      } finally {
        global.fetch = originalFetch;
      }
    });

    it('returns tenant_picker on multi-tenant response', async () => {
      const originalFetch = global.fetch;
      global.fetch = async () =>
        new Response(
          JSON.stringify({
            jwt: 'picker-jwt',
            tenants: [
              { tenantId: 't-1', tenantName: 'Acme Corp', role: 'approver' },
              { tenantId: 't-2', tenantName: 'Beta LLC', role: 'admin' },
            ],
          }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );

      try {
        const result = await login('http://localhost:8080', {
          email: 'multi@b.com',
          password: 'secret',
        });
        expect(result.kind).toBe('tenant_picker');
        if (result.kind === 'tenant_picker') {
          expect(result.jwt).toBe('picker-jwt');
          expect(result.tenants).toHaveLength(2);
          expect(result.tenants[0].tenantId).toBe('t-1');
        }
      } finally {
        global.fetch = originalFetch;
      }
    });

    it('throws on non-200 response', async () => {
      const originalFetch = global.fetch;
      global.fetch = async () =>
        new Response('Invalid credentials', { status: 401 });

      await expect(
        login('http://localhost:8080', { email: 'a@b.com', password: 'wrong' }),
      ).rejects.toThrow('Invalid credentials');
      global.fetch = originalFetch;
    });
  });

  describe('loginWithTenant', () => {
    it('returns AuthState from tenant-specific login', async () => {
      const originalFetch = global.fetch;
      let capturedAuth: string | null = null;
      let capturedBody: string | null = null;

      global.fetch = async (_url: string, init?: RequestInit) => {
        capturedAuth = (init?.headers as Record<string, string>)?.['Authorization'] ?? null;
        capturedBody = init?.body as string;
        return new Response(
          JSON.stringify({
            jwt: 'tenant-jwt',
            tenant_id: 't-1',
            user_id: 'u-1',
            email: 'a@b.com',
          }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );
      };

      try {
        const state = await loginWithTenant('http://localhost:8080', 'picker-jwt', 't-1');
        expect(state.jwt).toBe('tenant-jwt');
        expect(state.tenantId).toBe('t-1');
        expect(capturedAuth).toBe('Bearer picker-jwt');
        expect(JSON.parse(capturedBody!).tenant_id).toBe('t-1');
      } finally {
        global.fetch = originalFetch;
      }
    });
  });

  describe('persistence', () => {
    it('round-trips auth state through the store', async () => {
      const store = createStore();
      await saveAuth(store, sampleState);
      const loaded = await loadAuth(store);
      expect(loaded).toEqual(sampleState);
    });

    it('returns null when nothing stored', async () => {
      const store = createStore();
      const loaded = await loadAuth(store);
      expect(loaded).toBeNull();
    });

    it('clearAuth makes loadAuth return null', async () => {
      const store = createStore();
      await saveAuth(store, sampleState);
      await clearAuth(store);
      const loaded = await loadAuth(store);
      expect(loaded).toBeNull();
    });
  });
});
