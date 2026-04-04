import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { api, authApi, sandboxApi, PersonaInfo } from '@/lib/api';

export interface User {
  id: string;
  tenant_id: string;
  email: string;
  name: string;
  roles: string[];
}

export interface OrganizationTheme {
  presetId: string;
  customColors?: {
    primary: string;      // HSL values like "220 90% 56%"
    accent: string;
    capture: string;
    processing: string;
    vendor: string;
    reporting: string;
  };
  logoUrl?: string;
  logoMark?: string;      // Small icon version
  brandGradient?: string; // Custom gradient for brand areas
  gradientConfig?: {
    enabled: boolean;
    type: 'linear' | 'radial' | 'conic';
    angle?: number;
    positions?: { color: string; position: number }[];
  };
  branding?: {
    brandName: string;
    logoUrl?: string;
    logoMark?: string;
    faviconUrl?: string;
  };
  enabledForAllUsers?: boolean;
  allowUserOverride?: boolean;
}

export interface Tenant {
  id: string;
  name: string;
  enabled_modules: string[];
  settings: {
    logo_url?: string;
    primary_color?: string;
    company_name: string;
    timezone?: string;
    default_currency?: string;
  };
  theme?: OrganizationTheme;
}

interface AuthState {
  user: User | null;
  tenant: Tenant | null;
  currentPersona: PersonaInfo | null;
  accessToken: string | null;
  refreshToken: string | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  hasHydrated: boolean;

  // Actions
  login: (tenantId: string, email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  setTokens: (access: string, refresh: string) => void;
  hasRole: (role: string) => boolean;
  hasModule: (module: string) => boolean;
  switchPersona: (personaId: string) => Promise<void>;
  refreshTenantContext: () => Promise<void>;
  setHasHydrated: (state: boolean) => void;
}

// Helper function to set up API callbacks (avoids duplication)
export function setupApiCallbacks() {
  api.setTokenRefreshCallback((access, refresh) => {
    const store = useAuthStore.getState();
    store.setTokens(access, refresh);
  });
  api.setLogoutCallback(() => {
    const store = useAuthStore.getState();
    store.logout();
  });
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      user: null,
      tenant: null,
      currentPersona: null,
      accessToken: null,
      refreshToken: null,
      isAuthenticated: false,
      isLoading: false,
      hasHydrated: false,

      setHasHydrated: (state: boolean) => {
        set({ hasHydrated: state });
      },

      login: async (tenantId: string, email: string, password: string) => {
        set({ isLoading: true });
        try {
          const response = await authApi.login({
            tenant_id: tenantId,
            email,
            password,
          });

          api.setToken(response.access_token);
          api.setRefreshToken(response.refresh_token);
          setupApiCallbacks();

          set({
            user: response.user,
            accessToken: response.access_token,
            refreshToken: response.refresh_token,
            isAuthenticated: true,
            isLoading: false,
            tenant: {
              id: response.tenant.id,
              name: response.tenant.name,
              enabled_modules: response.tenant.enabled_modules,
              settings: response.tenant.settings,
            },
          });

          // Fetch current persona info
          try {
            const personaResponse = await sandboxApi.getCurrentPersona();
            set({ currentPersona: personaResponse.persona });
          } catch {
            // Ignore persona fetch errors - not critical
          }
        } catch (error) {
          set({ isLoading: false });
          throw error;
        }
      },

      logout: async () => {
        // Clear local state immediately
        api.setToken(null);
        api.setRefreshToken(null);
        set({
          user: null,
          tenant: null,
          currentPersona: null,
          accessToken: null,
          refreshToken: null,
          isAuthenticated: false,
        });
        // Best-effort call to backend to revoke refresh tokens
        try {
          await authApi.logout();
        } catch {
          // Ignore - local state already cleared
        }
      },

      setTokens: (access: string, refresh: string) => {
        api.setToken(access);
        api.setRefreshToken(refresh);
        set({
          accessToken: access,
          refreshToken: refresh,
        });
      },

      hasRole: (role: string) => {
        const { user } = get();
        if (!user) return false;
        return user.roles.includes(role) || user.roles.includes('tenant_admin');
      },

      hasModule: (module: string) => {
        const { tenant } = get();
        if (!tenant) return false;
        return tenant.enabled_modules.includes(module);
      },

      switchPersona: async (personaId: string) => {
        set({ isLoading: true });
        try {
          const response = await sandboxApi.switchPersona(personaId);
          
          // Update tenant modules based on new persona
          set((state) => ({
            isLoading: false,
            currentPersona: response.persona,
            tenant: state.tenant
              ? {
                  ...state.tenant,
                  enabled_modules: response.persona.modules
                    .filter((m) => m.enabled)
                    .map((m) => m.id),
                }
              : null,
          }));
        } catch (error) {
          set({ isLoading: false });
          throw error;
        }
      },

      refreshTenantContext: async () => {
        try {
          const context = await sandboxApi.getTenantContext();
          set((state) => ({
            currentPersona: context.persona,
            tenant: state.tenant
              ? {
                  ...state.tenant,
                  enabled_modules: context.enabled_modules
                    .filter((m) => m.enabled)
                    .map((m) => m.id),
                }
              : null,
          }));
        } catch {
          // Ignore errors
        }
      },
    }),
    {
      name: 'billforge-auth',
      partialize: (state) => ({
        user: state.user,
        tenant: state.tenant,
        currentPersona: state.currentPersona,
        accessToken: state.accessToken,
        refreshToken: state.refreshToken,
        isAuthenticated: state.isAuthenticated,
      }),
      onRehydrateStorage: () => (state) => {
        state?.setHasHydrated(true);
      },
    }
  )
);

// Initialize API token and callbacks on store hydration
if (typeof window !== 'undefined') {
  const stored = localStorage.getItem('billforge-auth');
  if (stored) {
    try {
      const parsed = JSON.parse(stored);
      if (parsed.state?.accessToken) {
        api.setToken(parsed.state.accessToken);
      }
      if (parsed.state?.refreshToken) {
        api.setRefreshToken(parsed.state.refreshToken);
      }
      // Set up refresh/logout callbacks
      setupApiCallbacks();
    } catch (e) {
      // Ignore parse errors
    }
  }
}

