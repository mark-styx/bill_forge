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
}

interface AuthState {
  user: User | null;
  tenant: Tenant | null;
  currentPersona: PersonaInfo | null;
  accessToken: string | null;
  refreshToken: string | null;
  isAuthenticated: boolean;
  isLoading: boolean;

  // Actions
  login: (tenantId: string, email: string, password: string) => Promise<void>;
  logout: () => void;
  setTokens: (access: string, refresh: string) => void;
  hasRole: (role: string) => boolean;
  hasModule: (module: string) => boolean;
  switchPersona: (personaId: string) => Promise<void>;
  refreshTenantContext: () => Promise<void>;
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

      login: async (tenantId: string, email: string, password: string) => {
        set({ isLoading: true });
        try {
          const response = await authApi.login({
            tenant_id: tenantId,
            email,
            password,
          });

          api.setToken(response.access_token);

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

      logout: () => {
        api.setToken(null);
        set({
          user: null,
          tenant: null,
          currentPersona: null,
          accessToken: null,
          refreshToken: null,
          isAuthenticated: false,
        });
      },

      setTokens: (access: string, refresh: string) => {
        api.setToken(access);
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
    }
  )
);

// Initialize API token on store hydration
if (typeof window !== 'undefined') {
  const stored = localStorage.getItem('billforge-auth');
  if (stored) {
    try {
      const parsed = JSON.parse(stored);
      if (parsed.state?.accessToken) {
        api.setToken(parsed.state.accessToken);
      }
    } catch (e) {
      // Ignore parse errors
    }
  }
}
