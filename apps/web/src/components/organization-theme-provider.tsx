'use client';

import { createContext, useContext, useEffect, useState, useCallback, ReactNode } from 'react';
import { useAuthStore, OrganizationTheme } from '@/stores/auth';
import { useThemeStore, themePresets, ThemeColors } from '@/stores/theme';

interface OrganizationThemeContextValue {
  organizationTheme: OrganizationTheme | null;
  isOrgThemeActive: boolean;
  applyOrganizationTheme: (theme: OrganizationTheme) => void;
  resetToUserTheme: () => void;
  updateOrganizationTheme: (updates: Partial<OrganizationTheme>) => void;
}

const OrganizationThemeContext = createContext<OrganizationThemeContextValue | null>(null);

export function useOrganizationTheme() {
  const context = useContext(OrganizationThemeContext);
  if (!context) {
    throw new Error('useOrganizationTheme must be used within OrganizationThemeProvider');
  }
  return context;
}

interface OrganizationThemeProviderProps {
  children: ReactNode;
}

export function OrganizationThemeProvider({ children }: OrganizationThemeProviderProps) {
  const { tenant } = useAuthStore();
  const { getCurrentColors, presetId } = useThemeStore();
  const [isOrgThemeActive, setIsOrgThemeActive] = useState(false);
  const [organizationTheme, setOrganizationTheme] = useState<OrganizationTheme | null>(null);

  // Apply colors to CSS variables
  const applyColorsToRoot = useCallback((colors: ThemeColors) => {
    if (typeof window === 'undefined') return;

    const root = document.documentElement;
    root.style.setProperty('--primary', colors.primary);
    root.style.setProperty('--accent', colors.accent);
    root.style.setProperty('--capture', colors.capture);
    root.style.setProperty('--processing', colors.processing);
    root.style.setProperty('--vendor', colors.vendor);
    root.style.setProperty('--reporting', colors.reporting);
    root.style.setProperty('--ring', colors.primary);
  }, []);

  // Apply organization theme
  const applyOrganizationTheme = useCallback((theme: OrganizationTheme) => {
    setOrganizationTheme(theme);
    setIsOrgThemeActive(true);

    if (theme.customColors) {
      applyColorsToRoot(theme.customColors);
    } else {
      const preset = themePresets.find((p) => p.id === theme.presetId);
      if (preset) {
        applyColorsToRoot(preset.colors);
      }
    }
  }, [applyColorsToRoot]);

  // Reset to user's personal theme preference
  const resetToUserTheme = useCallback(() => {
    setIsOrgThemeActive(false);
    const userColors = getCurrentColors();
    applyColorsToRoot(userColors);
  }, [getCurrentColors, applyColorsToRoot]);

  // Update organization theme (for settings page)
  const updateOrganizationTheme = useCallback((updates: Partial<OrganizationTheme>) => {
    if (!organizationTheme) return;

    const newTheme = { ...organizationTheme, ...updates };
    setOrganizationTheme(newTheme);

    if (updates.customColors || updates.presetId) {
      if (newTheme.customColors) {
        applyColorsToRoot(newTheme.customColors);
      } else {
        const preset = themePresets.find((p) => p.id === newTheme.presetId);
        if (preset) {
          applyColorsToRoot(preset.colors);
        }
      }
    }
  }, [organizationTheme, applyColorsToRoot]);

  // Initialize from tenant on mount
  useEffect(() => {
    if (tenant?.theme) {
      applyOrganizationTheme(tenant.theme);
    }
  }, [tenant?.theme, applyOrganizationTheme]);

  // Sync with tenant changes
  useEffect(() => {
    if (!tenant?.theme && isOrgThemeActive) {
      resetToUserTheme();
    }
  }, [tenant, isOrgThemeActive, resetToUserTheme]);

  const value: OrganizationThemeContextValue = {
    organizationTheme,
    isOrgThemeActive,
    applyOrganizationTheme,
    resetToUserTheme,
    updateOrganizationTheme,
  };

  return (
    <OrganizationThemeContext.Provider value={value}>
      {children}
    </OrganizationThemeContext.Provider>
  );
}
