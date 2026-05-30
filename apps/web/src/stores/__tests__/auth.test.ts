import { describe, it, expect, vi, beforeEach } from 'vitest';

// Ensure localStorage exists before auth.ts IIFE runs at import time.
// Node.js 22+ has an experimental localStorage that may be undefined without
// --localstorage-file; jsdom's polyfill is on window.localStorage but the
// global reference can be missing.
vi.hoisted(() => {
  if (typeof globalThis.localStorage === 'undefined') {
    const store: Record<string, string> = {};
    globalThis.localStorage = {
      getItem: (k: string) => store[k] ?? null,
      setItem: (k: string, v: string) => { store[k] = v; },
      removeItem: (k: string) => { delete store[k]; },
      clear: () => { for (const k of Object.keys(store)) delete store[k]; },
      get length() { return Object.keys(store).length; },
      key: (i: number) => Object.keys(store)[i] ?? null,
    };
  }
});

import { useAuthStore, setupApiCallbacks } from '../auth';
import type { User, Tenant } from '../auth';
import type { PersonaInfo } from '@/lib/api';

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

vi.mock('@/lib/api', () => ({
  api: {
    setToken: vi.fn(),
    setRefreshToken: vi.fn(),
    setTokenRefreshCallback: vi.fn(),
    setLogoutCallback: vi.fn(),
  },
  authApi: {
    login: vi.fn(),
    logout: vi.fn(),
  },
  sandboxApi: {
    getCurrentPersona: vi.fn(),
    switchPersona: vi.fn(),
    getTenantContext: vi.fn(),
  },
}));

// Re-import after mock so the module sees the mocked versions.
import { api, authApi, sandboxApi } from '@/lib/api';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const makeUser = (roles: string[] = []): User => ({
  id: 'u1',
  tenant_id: 't1',
  email: 'a@b.com',
  name: 'Test User',
  roles,
});

const makeTenant = (modules: string[] = []): Tenant => ({
  id: 't1',
  name: 'Acme Corp',
  enabled_modules: modules,
  settings: { company_name: 'Acme Corp' },
});

const makePersona = (moduleIds: string[]): PersonaInfo => ({
  id: 'p1',
  name: 'AP Clerk',
  description: 'Standard AP persona',
  modules: moduleIds.map((id, i) => ({
    id,
    name: `Module ${i}`,
    enabled: !id.startsWith('disabled_'),
  })),
  roles: [],
  reporting_sections: [],
});

function resetStore() {
  useAuthStore.setState({
    user: null,
    tenant: null,
    currentPersona: null,
    accessToken: null,
    refreshToken: null,
    isAuthenticated: false,
    isLoading: false,
    hasHydrated: false,
  });
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

describe('useAuthStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    resetStore();
  });

  // =========================================================================
  // A. RBAC — hasRole
  // =========================================================================
  describe('hasRole', () => {
    it('returns false when user is null', () => {
      useAuthStore.setState({ user: null });
      expect(useAuthStore.getState().hasRole('admin')).toBe(false);
    });

    it('returns true when user.roles includes the requested role', () => {
      useAuthStore.setState({ user: makeUser(['invoice_viewer']) });
      expect(useAuthStore.getState().hasRole('invoice_viewer')).toBe(true);
    });

    it('returns false when user.roles does not include the role and does not include tenant_admin', () => {
      useAuthStore.setState({ user: makeUser(['invoice_viewer']) });
      expect(useAuthStore.getState().hasRole('super_admin')).toBe(false);
    });

    it('tenant_admin bypass: returns true for ANY role when user is a tenant_admin', () => {
      useAuthStore.setState({ user: makeUser(['tenant_admin']) });
      // The user does not literally have 'batch_processor' or 'vendor_manager'
      expect(useAuthStore.getState().hasRole('batch_processor')).toBe(true);
      expect(useAuthStore.getState().hasRole('vendor_manager')).toBe(true);
    });

    it('returns true when querying tenant_admin itself for a tenant_admin user', () => {
      useAuthStore.setState({ user: makeUser(['tenant_admin']) });
      expect(useAuthStore.getState().hasRole('tenant_admin')).toBe(true);
    });
  });

  // =========================================================================
  // B. Module gating — hasModule
  // =========================================================================
  describe('hasModule', () => {
    it('returns false when tenant is null', () => {
      useAuthStore.setState({ tenant: null });
      expect(useAuthStore.getState().hasModule('invoice_capture')).toBe(false);
    });

    it('returns true when tenant.enabled_modules contains the module id', () => {
      useAuthStore.setState({ tenant: makeTenant(['invoice_capture', 'vendor_management']) });
      expect(useAuthStore.getState().hasModule('invoice_capture')).toBe(true);
    });

    it('returns false when tenant.enabled_modules does not contain the module id', () => {
      useAuthStore.setState({ tenant: makeTenant(['invoice_capture']) });
      expect(useAuthStore.getState().hasModule('vendor_management')).toBe(false);
    });

    it('returns false for an empty enabled_modules array', () => {
      useAuthStore.setState({ tenant: makeTenant([]) });
      expect(useAuthStore.getState().hasModule('invoice_capture')).toBe(false);
    });
  });

  // =========================================================================
  // C. Login / logout / setTokens
  // =========================================================================
  describe('login', () => {
    const loginResponse = {
      access_token: 'at-123',
      refresh_token: 'rt-456',
      user: makeUser(['invoice_viewer']),
      tenant: {
        id: 't1',
        name: 'Acme Corp',
        enabled_modules: ['invoice_capture'],
        settings: { company_name: 'Acme Corp' },
      },
    };

    it('success: sets tokens, user, tenant, and isAuthenticated', async () => {
      vi.mocked(authApi.login).mockResolvedValue(loginResponse);
      vi.mocked(sandboxApi.getCurrentPersona).mockResolvedValue({
        persona: makePersona(['invoice_capture']),
        tenant_id: 't1',
        tenant_name: 'Acme Corp',
      });

      await useAuthStore.getState().login('t1', 'a@b.com', 'pw');

      // API calls
      expect(authApi.login).toHaveBeenCalledWith({
        tenant_id: 't1',
        email: 'a@b.com',
        password: 'pw',
      });
      expect(api.setToken).toHaveBeenCalledWith('at-123');
      expect(api.setRefreshToken).toHaveBeenCalledWith('rt-456');

      // Store state
      const s = useAuthStore.getState();
      expect(s.accessToken).toBe('at-123');
      expect(s.refreshToken).toBe('rt-456');
      expect(s.user).toEqual(loginResponse.user);
      expect(s.tenant?.id).toBe('t1');
      expect(s.isAuthenticated).toBe(true);
      expect(s.isLoading).toBe(false);
    });

    it('failure: rejects, leaves isAuthenticated false, resets isLoading', async () => {
      vi.mocked(authApi.login).mockRejectedValue(new Error('bad creds'));

      await expect(
        useAuthStore.getState().login('t1', 'a@b.com', 'wrong')
      ).rejects.toThrow('bad creds');

      const s = useAuthStore.getState();
      expect(s.isAuthenticated).toBe(false);
      expect(s.isLoading).toBe(false);
      expect(api.setToken).not.toHaveBeenCalled();
    });

    it('persona fetch failure is swallowed — auth still succeeds', async () => {
      vi.mocked(authApi.login).mockResolvedValue(loginResponse);
      vi.mocked(sandboxApi.getCurrentPersona).mockRejectedValue(new Error('persona down'));

      await useAuthStore.getState().login('t1', 'a@b.com', 'pw');

      const s = useAuthStore.getState();
      expect(s.isAuthenticated).toBe(true);
      expect(s.currentPersona).toBeNull();
    });
  });

  describe('logout', () => {
    it('clears tokens, user, tenant, currentPersona, isAuthenticated', async () => {
      // Seed state first
      useAuthStore.setState({
        user: makeUser(['admin']),
        tenant: makeTenant(['invoice_capture']),
        currentPersona: makePersona(['invoice_capture']),
        accessToken: 'at',
        refreshToken: 'rt',
        isAuthenticated: true,
      });
      vi.mocked(authApi.logout).mockResolvedValue(undefined);

      await useAuthStore.getState().logout();

      expect(authApi.logout).toHaveBeenCalled();
      expect(api.setToken).toHaveBeenCalledWith(null);
      expect(api.setRefreshToken).toHaveBeenCalledWith(null);

      const s = useAuthStore.getState();
      expect(s.user).toBeNull();
      expect(s.tenant).toBeNull();
      expect(s.currentPersona).toBeNull();
      expect(s.accessToken).toBeNull();
      expect(s.refreshToken).toBeNull();
      expect(s.isAuthenticated).toBe(false);
    });

    it('proceeds with local cleanup even when authApi.logout rejects', async () => {
      useAuthStore.setState({
        user: makeUser([]),
        accessToken: 'at',
        isAuthenticated: true,
      });
      vi.mocked(authApi.logout).mockRejectedValue(new Error('network'));

      await useAuthStore.getState().logout();

      expect(api.setToken).toHaveBeenCalledWith(null);
      const s = useAuthStore.getState();
      expect(s.user).toBeNull();
      expect(s.isAuthenticated).toBe(false);
    });
  });

  describe('setTokens', () => {
    it('writes tokens to store and propagates to api', () => {
      useAuthStore.getState().setTokens('new-at', 'new-rt');

      expect(api.setToken).toHaveBeenCalledWith('new-at');
      expect(api.setRefreshToken).toHaveBeenCalledWith('new-rt');

      const s = useAuthStore.getState();
      expect(s.accessToken).toBe('new-at');
      expect(s.refreshToken).toBe('new-rt');
    });
  });

  // =========================================================================
  // D. Persistence — partialize and onRehydrateStorage
  // =========================================================================
  describe('persistence', () => {
    it('partialize returns only the persisted fields (no isLoading, no hasHydrated)', () => {
      const options = useAuthStore.persist.getOptions();
      const partialize = options.partialize as (state: Record<string, unknown>) => Record<string, unknown>;

      const fullState = {
        user: makeUser(['admin']),
        tenant: makeTenant(['m1']),
        currentPersona: makePersona(['m1']),
        accessToken: 'at',
        refreshToken: 'rt',
        isAuthenticated: true,
        isLoading: true,          // should NOT be persisted
        hasHydrated: true,        // should NOT be persisted
      };

      const result = partialize(fullState);

      expect(result).toEqual({
        user: fullState.user,
        tenant: fullState.tenant,
        currentPersona: fullState.currentPersona,
        accessToken: 'at',
        refreshToken: 'rt',
        isAuthenticated: true,
      });

      // Explicitly verify transient fields are excluded
      expect(result).not.toHaveProperty('isLoading');
      expect(result).not.toHaveProperty('hasHydrated');
    });

    it('setHasHydrated toggles the flag', () => {
      useAuthStore.getState().setHasHydrated(true);
      expect(useAuthStore.getState().hasHydrated).toBe(true);

      useAuthStore.getState().setHasHydrated(false);
      expect(useAuthStore.getState().hasHydrated).toBe(false);
    });

    it('onRehydrateStorage callback sets hasHydrated to true', () => {
      const options = useAuthStore.persist.getOptions();
      const onRehydrateStorage = options.onRehydrateStorage as (
        state: Record<string, unknown>
      ) => (state: Record<string, unknown> | undefined) => void;

      // Get the callback that onRehydrateStorage returns
      const rehydrateFinish = onRehydrateStorage(useAuthStore.getState());

      // hasHydrated should be false before the callback runs
      expect(useAuthStore.getState().hasHydrated).toBe(false);

      // Simulate rehydration finishing — the store receives the state
      rehydrateFinish(useAuthStore.getState());

      expect(useAuthStore.getState().hasHydrated).toBe(true);
    });
  });

  // =========================================================================
  // E. Persona / tenant context
  // =========================================================================
  describe('switchPersona', () => {
    it('updates currentPersona and filters tenant.enabled_modules to enabled ones', async () => {
      const persona = makePersona(['m1', 'disabled_m2']);
      vi.mocked(sandboxApi.switchPersona).mockResolvedValue({
        persona,
        tenant_id: 't1',
      });
      useAuthStore.setState({ tenant: makeTenant(['old']) });

      await useAuthStore.getState().switchPersona('p1');

      const s = useAuthStore.getState();
      expect(s.currentPersona).toEqual(persona);
      // Only enabled modules (m1); disabled_m2 starts with 'disabled_' so enabled=false
      expect(s.tenant?.enabled_modules).toEqual(['m1']);
      expect(s.isLoading).toBe(false);
    });

    it('leaves tenant null when tenant is null (no crash)', async () => {
      const persona = makePersona(['m1']);
      vi.mocked(sandboxApi.switchPersona).mockResolvedValue({
        persona,
        tenant_id: 't1',
      });
      useAuthStore.setState({ tenant: null });

      await useAuthStore.getState().switchPersona('p1');

      expect(useAuthStore.getState().tenant).toBeNull();
      expect(useAuthStore.getState().isLoading).toBe(false);
    });
  });

  describe('refreshTenantContext', () => {
    it('updates currentPersona and rewrites enabled_modules from context', async () => {
      const persona = makePersona(['m1']);
      vi.mocked(sandboxApi.getTenantContext).mockResolvedValue({
        persona,
        tenant_id: 't1',
        enabled_modules: [
          { id: 'm1', name: 'M1', enabled: true },
          { id: 'm2', name: 'M2', enabled: false },
        ],
      });
      useAuthStore.setState({ tenant: makeTenant(['old']) });

      await useAuthStore.getState().refreshTenantContext();

      const s = useAuthStore.getState();
      expect(s.currentPersona).toEqual(persona);
      expect(s.tenant?.enabled_modules).toEqual(['m1']);
    });

    it('swallows errors silently when API rejects', async () => {
      vi.mocked(sandboxApi.getTenantContext).mockRejectedValue(new Error('fail'));
      useAuthStore.setState({ tenant: makeTenant(['keep']) });

      // Should not throw
      await useAuthStore.getState().refreshTenantContext();

      // State unchanged
      expect(useAuthStore.getState().tenant?.enabled_modules).toEqual(['keep']);
    });
  });
});

// ---------------------------------------------------------------------------
// setupApiCallbacks
// ---------------------------------------------------------------------------
describe('setupApiCallbacks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
  });

  it('registers a token refresh callback that calls setTokens', () => {
    setupApiCallbacks();

    expect(api.setTokenRefreshCallback).toHaveBeenCalledTimes(1);
    const cb = vi.mocked(api.setTokenRefreshCallback).mock.calls[0][0];

    // Invoke the callback — it should delegate to the store's setTokens
    resetStore();
    cb('new-at', 'new-rt');

    expect(useAuthStore.getState().accessToken).toBe('new-at');
    expect(useAuthStore.getState().refreshToken).toBe('new-rt');
    expect(api.setToken).toHaveBeenCalledWith('new-at');
    expect(api.setRefreshToken).toHaveBeenCalledWith('new-rt');
  });

  it('registers a logout callback that calls store logout', async () => {
    setupApiCallbacks();

    expect(api.setLogoutCallback).toHaveBeenCalledTimes(1);
    const cb = vi.mocked(api.setLogoutCallback).mock.calls[0][0];

    // Seed then invoke
    useAuthStore.setState({ isAuthenticated: true, accessToken: 'at' });
    vi.mocked(authApi.logout).mockResolvedValue(undefined);

    await cb();

    expect(api.setToken).toHaveBeenCalledWith(null);
    expect(useAuthStore.getState().isAuthenticated).toBe(false);
  });
});
