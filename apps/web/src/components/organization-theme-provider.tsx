'use client';

import { createContext, useContext, useEffect, useState, useCallback, ReactNode, useMemo } from 'react';
import { useAuthStore, OrganizationTheme } from '@/stores/auth';
import { useThemeStore, themePresets, ThemeColors, generateCSSGradient, GradientConfig } from '@/stores/theme';

interface OrganizationThemeContextValue {
  organizationTheme: OrganizationTheme | null;
  isOrgThemeActive: boolean;
  isPreviewMode: boolean;
  previewColors: ThemeColors | null;
  applyOrganizationTheme: (theme: OrganizationTheme) => void;
  resetToUserTheme: () => void;
  updateOrganizationTheme: (updates: Partial<OrganizationTheme>) => void;
  startPreview: (colors: ThemeColors) => void;
  stopPreview: () => void;
  applyPreviewColors: (colors: ThemeColors) => void;
  getEffectiveGradient: () => string;
  getBrandGradient: () => string;
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
  const { getCurrentColors } = useThemeStore();
  const [isOrgThemeActive, setIsOrgThemeActive] = useState(false);
  const [organizationTheme, setOrganizationTheme] = useState<OrganizationTheme | null>(null);
  const [isPreviewMode, setIsPreviewMode] = useState(false);
  const [previewColors, setPreviewColors] = useState<ThemeColors | null>(null);
  const [originalColors, setOriginalColors] = useState<ThemeColors | null>(null);

  // Apply colors to CSS custom properties
  const applyColorsToRoot = useCallback((colors: ThemeColors, applyGradient = true) => {
    if (typeof window === 'undefined') return;

    const root = document.documentElement;

    // Core color variables
    root.style.setProperty('--primary', colors.primary);
    root.style.setProperty('--accent', colors.accent);
    root.style.setProperty('--capture', colors.capture);
    root.style.setProperty('--processing', colors.processing);
    root.style.setProperty('--vendor', colors.vendor);
    root.style.setProperty('--reporting', colors.reporting);
    root.style.setProperty('--ring', colors.primary);

    // Organization gradient variables
    if (applyGradient) {
      root.style.setProperty('--org-gradient-from', colors.primary);
      root.style.setProperty('--org-gradient-to', colors.accent);
    }

    // Trigger a custom event for components that need to react to theme changes
    window.dispatchEvent(new CustomEvent('org-theme-change', { detail: { colors } }));
  }, []);

  // Apply organization theme
  const applyOrganizationTheme = useCallback((theme: OrganizationTheme) => {
    // Save current colors before applying org theme
    if (!isOrgThemeActive) {
      setOriginalColors(getCurrentColors());
    }

    setOrganizationTheme(theme);
    setIsOrgThemeActive(true);

    const colors = theme.custom_colors || themePresets.find((p) => p.id === theme.preset_id)?.colors;
    if (colors) {
      applyColorsToRoot(colors);
    }
  }, [applyColorsToRoot, getCurrentColors, isOrgThemeActive]);

  // Reset to user's personal theme preference
  const resetToUserTheme = useCallback(() => {
    setIsOrgThemeActive(false);
    setIsPreviewMode(false);
    setPreviewColors(null);

    const userColors = getCurrentColors();
    applyColorsToRoot(userColors);
  }, [getCurrentColors, applyColorsToRoot]);

  // Update organization theme (for settings page)
  const updateOrganizationTheme = useCallback((updates: Partial<OrganizationTheme>) => {
    if (!organizationTheme) return;

    const newTheme = { ...organizationTheme, ...updates };
    setOrganizationTheme(newTheme);

    if (updates.custom_colors || updates.preset_id) {
      const colors = newTheme.custom_colors || themePresets.find((p) => p.id === newTheme.preset_id)?.colors;
      if (colors) {
        applyColorsToRoot(colors);
      }
    }
  }, [organizationTheme, applyColorsToRoot]);

  // Start live preview mode
  const startPreview = useCallback((colors: ThemeColors) => {
    if (!isPreviewMode) {
      // Save current state before preview
      setOriginalColors(getCurrentColors());
    }
    setIsPreviewMode(true);
    setPreviewColors(colors);
    applyColorsToRoot(colors, true);
  }, [isPreviewMode, getCurrentColors, applyColorsToRoot]);

  // Stop preview and restore previous state
  const stopPreview = useCallback(() => {
    setIsPreviewMode(false);
    setPreviewColors(null);

    if (isOrgThemeActive && organizationTheme) {
      const orgColors = organizationTheme.custom_colors ||
        themePresets.find((p) => p.id === organizationTheme.preset_id)?.colors;
      if (orgColors) {
        applyColorsToRoot(orgColors);
      }
    } else if (originalColors) {
      applyColorsToRoot(originalColors);
    } else {
      const userColors = getCurrentColors();
      applyColorsToRoot(userColors);
    }
  }, [isOrgThemeActive, organizationTheme, originalColors, getCurrentColors, applyColorsToRoot]);

  // Apply preview colors in real-time (for color picker changes)
  const applyPreviewColors = useCallback((colors: ThemeColors) => {
    if (isPreviewMode) {
      setPreviewColors(colors);
      applyColorsToRoot(colors, true);
    }
  }, [isPreviewMode, applyColorsToRoot]);

  // Get effective gradient CSS
  const getEffectiveGradient = useCallback(() => {
    const colors = previewColors ||
      (isOrgThemeActive && organizationTheme?.custom_colors) ||
      (isOrgThemeActive && themePresets.find((p) => p.id === organizationTheme?.preset_id)?.colors) ||
      getCurrentColors();

    const gradientConfig: GradientConfig = organizationTheme?.gradient_config || {
      enabled: true,
      type: 'linear',
      angle: 135,
    };

    return generateCSSGradient(gradientConfig, colors);
  }, [previewColors, isOrgThemeActive, organizationTheme, getCurrentColors]);

  // Get brand gradient for headers/banners
  const getBrandGradient = useCallback(() => {
    const colors = previewColors ||
      (isOrgThemeActive && organizationTheme?.custom_colors) ||
      (isOrgThemeActive && themePresets.find((p) => p.id === organizationTheme?.preset_id)?.colors) ||
      getCurrentColors();

    return `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`;
  }, [previewColors, isOrgThemeActive, organizationTheme, getCurrentColors]);

  // Initialize from tenant on mount
  useEffect(() => {
    if (tenant?.theme) {
      applyOrganizationTheme(tenant.theme);
    }
  }, [tenant?.theme, applyOrganizationTheme]);

  // Sync with tenant changes
  useEffect(() => {
    if (!tenant?.theme && isOrgThemeActive && !isPreviewMode) {
      resetToUserTheme();
    }
  }, [tenant, isOrgThemeActive, isPreviewMode, resetToUserTheme]);

  // Listen for system theme changes and reapply
  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    const handleChange = () => {
      // Re-apply colors when system theme changes
      if (isPreviewMode && previewColors) {
        applyColorsToRoot(previewColors);
      } else if (isOrgThemeActive && organizationTheme) {
        const colors = organizationTheme.custom_colors ||
          themePresets.find((p) => p.id === organizationTheme.preset_id)?.colors;
        if (colors) {
          applyColorsToRoot(colors);
        }
      }
    };

    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, [isPreviewMode, previewColors, isOrgThemeActive, organizationTheme, applyColorsToRoot]);

  const value = useMemo<OrganizationThemeContextValue>(() => ({
    organizationTheme,
    isOrgThemeActive,
    isPreviewMode,
    previewColors,
    applyOrganizationTheme,
    resetToUserTheme,
    updateOrganizationTheme,
    startPreview,
    stopPreview,
    applyPreviewColors,
    getEffectiveGradient,
    getBrandGradient,
  }), [
    organizationTheme,
    isOrgThemeActive,
    isPreviewMode,
    previewColors,
    applyOrganizationTheme,
    resetToUserTheme,
    updateOrganizationTheme,
    startPreview,
    stopPreview,
    applyPreviewColors,
    getEffectiveGradient,
    getBrandGradient,
  ]);

  return (
    <OrganizationThemeContext.Provider value={value}>
      {children}
    </OrganizationThemeContext.Provider>
  );
}
