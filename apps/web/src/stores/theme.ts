import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type ThemeMode = 'light' | 'dark' | 'system';

export interface ThemeColors {
  primary: string;      // HSL values like "220 90% 56%"
  accent: string;
  capture: string;
  processing: string;
  vendor: string;
  reporting: string;
}

export interface BrandingConfig {
  logoUrl?: string;
  logoMark?: string;
  faviconUrl?: string;
  brandName: string;
  brandGradient?: string;
  customCSS?: string;
}

export interface ThemePreset {
  id: string;
  name: string;
  description: string;
  colors: ThemeColors;
  preview: string; // Gradient for preview
  category: 'bright' | 'professional' | 'vibrant' | 'modern';
  gradient?: {
    from: string;
    via?: string;
    to: string;
    angle?: number;
  };
}

// Modern, bright theme presets with expanded options
export const themePresets: ThemePreset[] = [
  // Bright & Clean Category
  {
    id: 'ocean',
    name: 'Ocean Blue',
    description: 'Clean and professional with calming blue tones',
    preview: 'from-blue-500 to-cyan-400',
    category: 'bright',
    gradient: { from: '210 100% 50%', to: '190 95% 45%', angle: 135 },
    colors: {
      primary: '210 100% 50%',
      accent: '190 95% 45%',
      capture: '195 100% 45%',
      processing: '160 84% 39%',
      vendor: '270 70% 55%',
      reporting: '35 95% 55%',
    },
  },
  {
    id: 'sky',
    name: 'Sky',
    description: 'Light and airy with bright sky blues',
    preview: 'from-sky-400 to-blue-400',
    category: 'bright',
    gradient: { from: '200 100% 50%', to: '210 100% 55%', angle: 135 },
    colors: {
      primary: '200 100% 50%',
      accent: '185 100% 45%',
      capture: '195 100% 48%',
      processing: '158 80% 42%',
      vendor: '265 70% 58%',
      reporting: '40 95% 52%',
    },
  },
  {
    id: 'emerald',
    name: 'Emerald',
    description: 'Fresh and modern with vibrant greens',
    preview: 'from-emerald-500 to-teal-400',
    category: 'bright',
    gradient: { from: '160 84% 39%', to: '170 80% 45%', angle: 135 },
    colors: {
      primary: '160 84% 39%',
      accent: '170 80% 45%',
      capture: '200 90% 48%',
      processing: '160 84% 39%',
      vendor: '280 65% 55%',
      reporting: '38 92% 50%',
    },
  },
  {
    id: 'mint',
    name: 'Mint',
    description: 'Refreshing mint with cool undertones',
    preview: 'from-emerald-400 to-cyan-400',
    category: 'bright',
    gradient: { from: '168 76% 42%', to: '175 75% 50%', angle: 135 },
    colors: {
      primary: '168 76% 42%',
      accent: '175 75% 45%',
      capture: '190 85% 48%',
      processing: '168 76% 42%',
      vendor: '270 65% 55%',
      reporting: '42 90% 52%',
    },
  },
  {
    id: 'azure',
    name: 'Azure',
    description: 'Bright and modern with electric blues',
    preview: 'from-blue-400 to-indigo-400',
    category: 'bright',
    gradient: { from: '217 91% 60%', to: '238 75% 58%', angle: 135 },
    colors: {
      primary: '217 91% 60%',
      accent: '199 89% 48%',
      capture: '199 89% 48%',
      processing: '162 78% 45%',
      vendor: '262 83% 58%',
      reporting: '43 96% 56%',
    },
  },
  // Modern Category - NEW
  {
    id: 'electric',
    name: 'Electric',
    description: 'High-energy with vivid electric blue',
    preview: 'from-blue-600 to-cyan-500',
    category: 'modern',
    gradient: { from: '220 100% 55%', to: '185 100% 50%', angle: 120 },
    colors: {
      primary: '220 100% 55%',
      accent: '185 100% 50%',
      capture: '190 100% 50%',
      processing: '155 90% 45%',
      vendor: '275 85% 58%',
      reporting: '42 100% 55%',
    },
  },
  {
    id: 'neon',
    name: 'Neon',
    description: 'Bold neon aesthetics for standout interfaces',
    preview: 'from-cyan-400 to-fuchsia-500',
    category: 'modern',
    gradient: { from: '180 100% 50%', via: '220 100% 60%', to: '300 100% 55%', angle: 135 },
    colors: {
      primary: '180 100% 50%',
      accent: '300 100% 55%',
      capture: '180 100% 50%',
      processing: '150 100% 45%',
      vendor: '300 100% 55%',
      reporting: '50 100% 55%',
    },
  },
  {
    id: 'aurora',
    name: 'Aurora',
    description: 'Inspired by northern lights with shifting hues',
    preview: 'from-green-400 via-cyan-500 to-purple-500',
    category: 'modern',
    gradient: { from: '150 80% 50%', via: '190 100% 50%', to: '280 80% 55%', angle: 135 },
    colors: {
      primary: '190 100% 50%',
      accent: '150 80% 50%',
      capture: '190 100% 50%',
      processing: '150 80% 50%',
      vendor: '280 80% 55%',
      reporting: '50 95% 55%',
    },
  },
  {
    id: 'cosmic',
    name: 'Cosmic',
    description: 'Deep space vibes with purple and blue',
    preview: 'from-purple-600 via-blue-600 to-cyan-500',
    category: 'modern',
    gradient: { from: '280 85% 50%', via: '230 85% 55%', to: '195 100% 50%', angle: 135 },
    colors: {
      primary: '250 85% 55%',
      accent: '195 100% 50%',
      capture: '195 100% 50%',
      processing: '160 85% 45%',
      vendor: '280 85% 55%',
      reporting: '45 95% 55%',
    },
  },
  {
    id: 'sunset-glow',
    name: 'Sunset Glow',
    description: 'Warm sunset gradients with orange and pink',
    preview: 'from-orange-500 via-red-500 to-pink-500',
    category: 'modern',
    gradient: { from: '30 100% 55%', via: '0 85% 58%', to: '330 85% 55%', angle: 135 },
    colors: {
      primary: '15 95% 55%',
      accent: '330 85% 55%',
      capture: '195 90% 50%',
      processing: '155 80% 45%',
      vendor: '280 75% 55%',
      reporting: '45 100% 55%',
    },
  },
  // Vibrant Category
  {
    id: 'violet',
    name: 'Violet',
    description: 'Creative and bold with purple accents',
    preview: 'from-violet-500 to-purple-400',
    category: 'vibrant',
    gradient: { from: '270 75% 55%', to: '280 70% 60%', angle: 135 },
    colors: {
      primary: '270 75% 55%',
      accent: '280 70% 60%',
      capture: '200 90% 50%',
      processing: '155 80% 42%',
      vendor: '270 75% 55%',
      reporting: '40 95% 52%',
    },
  },
  {
    id: 'coral',
    name: 'Coral',
    description: 'Warm and inviting with coral highlights',
    preview: 'from-rose-400 to-orange-300',
    category: 'vibrant',
    gradient: { from: '10 90% 60%', to: '25 95% 55%', angle: 135 },
    colors: {
      primary: '10 90% 60%',
      accent: '15 85% 55%',
      capture: '195 90% 50%',
      processing: '150 75% 42%',
      vendor: '260 70% 58%',
      reporting: '35 95% 55%',
    },
  },
  {
    id: 'sunset',
    name: 'Sunset',
    description: 'Energetic with warm gradients',
    preview: 'from-orange-500 to-pink-500',
    category: 'vibrant',
    gradient: { from: '25 95% 55%', to: '340 80% 55%', angle: 135 },
    colors: {
      primary: '25 95% 55%',
      accent: '340 80% 55%',
      capture: '195 90% 48%',
      processing: '155 75% 40%',
      vendor: '280 70% 58%',
      reporting: '40 95% 52%',
    },
  },
  {
    id: 'fuchsia',
    name: 'Fuchsia',
    description: 'Bold and energetic with pink-purple vibes',
    preview: 'from-fuchsia-500 to-pink-400',
    category: 'vibrant',
    gradient: { from: '300 80% 55%', to: '320 75% 55%', angle: 135 },
    colors: {
      primary: '300 80% 55%',
      accent: '320 75% 55%',
      capture: '195 90% 50%',
      processing: '160 80% 42%',
      vendor: '300 80% 55%',
      reporting: '38 95% 52%',
    },
  },
  {
    id: 'amber',
    name: 'Amber',
    description: 'Warm golden tones for a friendly feel',
    preview: 'from-amber-500 to-yellow-400',
    category: 'vibrant',
    gradient: { from: '38 92% 50%', to: '45 95% 55%', angle: 135 },
    colors: {
      primary: '38 92% 50%',
      accent: '45 95% 52%',
      capture: '195 90% 48%',
      processing: '155 78% 42%',
      vendor: '270 70% 55%',
      reporting: '38 92% 50%',
    },
  },
  {
    id: 'rose',
    name: 'Rose',
    description: 'Soft pink tones with modern appeal',
    preview: 'from-rose-500 to-pink-400',
    category: 'vibrant',
    gradient: { from: '350 89% 60%', to: '330 81% 60%', angle: 135 },
    colors: {
      primary: '350 89% 60%',
      accent: '330 81% 60%',
      capture: '199 89% 48%',
      processing: '162 78% 45%',
      vendor: '280 68% 55%',
      reporting: '43 96% 56%',
    },
  },
  {
    id: 'lime',
    name: 'Lime',
    description: 'Fresh and energetic with lime green accents',
    preview: 'from-lime-500 to-green-400',
    category: 'vibrant',
    gradient: { from: '84 85% 45%', to: '142 76% 45%', angle: 135 },
    colors: {
      primary: '84 85% 45%',
      accent: '142 76% 45%',
      capture: '199 89% 48%',
      processing: '84 85% 45%',
      vendor: '262 83% 58%',
      reporting: '43 96% 56%',
    },
  },
  {
    id: 'tropical',
    name: 'Tropical',
    description: 'Bright tropical vibes with turquoise and coral',
    preview: 'from-teal-400 to-orange-400',
    category: 'vibrant',
    gradient: { from: '175 85% 45%', to: '25 90% 55%', angle: 135 },
    colors: {
      primary: '175 85% 45%',
      accent: '25 90% 55%',
      capture: '175 85% 48%',
      processing: '160 80% 42%',
      vendor: '280 70% 55%',
      reporting: '38 95% 52%',
    },
  },
  // Professional Category
  {
    id: 'slate',
    name: 'Slate',
    description: 'Minimalist and sophisticated',
    preview: 'from-slate-500 to-slate-400',
    category: 'professional',
    gradient: { from: '215 28% 45%', to: '220 25% 55%', angle: 135 },
    colors: {
      primary: '215 28% 45%',
      accent: '220 25% 50%',
      capture: '200 80% 50%',
      processing: '155 70% 45%',
      vendor: '260 60% 55%',
      reporting: '38 85% 52%',
    },
  },
  {
    id: 'indigo',
    name: 'Indigo',
    description: 'Deep and trustworthy professional blue',
    preview: 'from-indigo-600 to-indigo-400',
    category: 'professional',
    gradient: { from: '238 75% 55%', to: '245 70% 60%', angle: 135 },
    colors: {
      primary: '238 75% 55%',
      accent: '245 70% 58%',
      capture: '200 85% 50%',
      processing: '158 75% 42%',
      vendor: '260 70% 55%',
      reporting: '40 90% 52%',
    },
  },
  {
    id: 'charcoal',
    name: 'Charcoal',
    description: 'Elegant dark accents for a premium look',
    preview: 'from-gray-700 to-gray-500',
    category: 'professional',
    gradient: { from: '220 15% 35%', to: '215 18% 50%', angle: 135 },
    colors: {
      primary: '220 15% 35%',
      accent: '215 18% 45%',
      capture: '200 75% 48%',
      processing: '155 70% 42%',
      vendor: '260 55% 52%',
      reporting: '38 85% 50%',
    },
  },
  {
    id: 'graphite',
    name: 'Graphite',
    description: 'Modern and sleek with subtle blue tones',
    preview: 'from-zinc-600 to-zinc-400',
    category: 'professional',
    gradient: { from: '240 5% 34%', to: '220 14% 50%', angle: 135 },
    colors: {
      primary: '240 5% 34%',
      accent: '220 14% 46%',
      capture: '199 89% 48%',
      processing: '162 78% 45%',
      vendor: '262 68% 55%',
      reporting: '43 96% 56%',
    },
  },
  {
    id: 'corporate',
    name: 'Corporate',
    description: 'Clean corporate blue for enterprise applications',
    preview: 'from-blue-700 to-blue-500',
    category: 'professional',
    gradient: { from: '215 70% 45%', to: '215 75% 55%', angle: 135 },
    colors: {
      primary: '215 70% 48%',
      accent: '210 65% 55%',
      capture: '200 80% 50%',
      processing: '155 65% 45%',
      vendor: '260 60% 52%',
      reporting: '38 80% 50%',
    },
  },
];

// Organization theme that can be stored/synced with backend
export interface OrganizationThemeConfig {
  presetId: string;
  customColors?: ThemeColors;
  branding: BrandingConfig;
  enabledForAllUsers?: boolean;
  allowUserOverride?: boolean;
  gradientConfig?: {
    enabled: boolean;
    type: 'linear' | 'radial';
    angle?: number;
    positions?: { color: string; position: number }[];
  };
}

// Utility functions for working with themes
export function hslToCSS(hsl: string): string {
  return `hsl(${hsl})`;
}

export function getContrastColor(hsl: string): string {
  const parts = hsl.split(' ').map((p) => parseFloat(p.replace('%', '')));
  const lightness = parts[2] || 50;
  return lightness > 55 ? '0 0% 0%' : '0 0% 100%';
}

export function generateGradient(preset: ThemePreset): string {
  if (!preset.gradient) {
    return `linear-gradient(135deg, hsl(${preset.colors.primary}), hsl(${preset.colors.accent}))`;
  }

  const { from, via, to, angle = 135 } = preset.gradient;
  if (via) {
    return `linear-gradient(${angle}deg, hsl(${from}), hsl(${via}), hsl(${to}))`;
  }
  return `linear-gradient(${angle}deg, hsl(${from}), hsl(${to}))`;
}

export function getThemeByCategory(category: string): ThemePreset[] {
  return themePresets.filter((p) => p.category === category);
}

export function getThemeCategories(): string[] {
  return Array.from(new Set(themePresets.map((p) => p.category)));
}

// Export/Import theme configuration
export function exportThemeConfig(state: {
  presetId: string;
  customColors: ThemeColors | null;
  organizationTheme: OrganizationThemeConfig | null;
}): string {
  return JSON.stringify({
    version: '1.0',
    exportedAt: new Date().toISOString(),
    config: {
      presetId: state.presetId,
      customColors: state.customColors,
      organizationTheme: state.organizationTheme,
    },
  }, null, 2);
}

export function importThemeConfig(json: string): {
  presetId: string;
  customColors: ThemeColors | null;
  organizationTheme: OrganizationThemeConfig | null;
} | null {
  try {
    const data = JSON.parse(json);
    if (data.version && data.config) {
      return data.config;
    }
    return null;
  } catch {
    return null;
  }
}

interface ThemeState {
  mode: ThemeMode;
  presetId: string;
  customColors: ThemeColors | null;
  sidebarCollapsed: boolean;

  // Organization-level theme (takes precedence when set)
  organizationTheme: OrganizationThemeConfig | null;
  isOrgThemeActive: boolean;

  // Actions
  setMode: (mode: ThemeMode) => void;
  setPreset: (presetId: string) => void;
  setCustomColors: (colors: ThemeColors) => void;
  clearCustomColors: () => void;
  toggleSidebar: () => void;

  // Organization theme actions
  setOrganizationTheme: (theme: OrganizationThemeConfig) => void;
  clearOrganizationTheme: () => void;
  updateOrganizationTheme: (updates: Partial<OrganizationThemeConfig>) => void;

  // Getters
  getCurrentColors: () => ThemeColors;
  getCurrentPreset: () => ThemePreset | undefined;
  getEffectiveColors: () => ThemeColors; // Considers org theme first
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      mode: 'light',
      presetId: 'ocean',
      customColors: null,
      sidebarCollapsed: false,
      organizationTheme: null,
      isOrgThemeActive: false,

      setMode: (mode) => {
        set({ mode });
        applyMode(mode);
      },

      setPreset: (presetId) => {
        set({ presetId, customColors: null });
        const { isOrgThemeActive } = get();
        if (!isOrgThemeActive) {
          const preset = themePresets.find((p) => p.id === presetId);
          if (preset) {
            applyColors(preset.colors);
          }
        }
      },

      setCustomColors: (colors) => {
        set({ customColors: colors });
        const { isOrgThemeActive } = get();
        if (!isOrgThemeActive) {
          applyColors(colors);
        }
      },

      clearCustomColors: () => {
        const { presetId, isOrgThemeActive } = get();
        set({ customColors: null });
        if (!isOrgThemeActive) {
          const preset = themePresets.find((p) => p.id === presetId);
          if (preset) {
            applyColors(preset.colors);
          }
        }
      },

      toggleSidebar: () => {
        set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed }));
      },

      // Organization theme management
      setOrganizationTheme: (theme) => {
        set({ organizationTheme: theme, isOrgThemeActive: true });
        const colors = theme.customColors ||
          themePresets.find((p) => p.id === theme.presetId)?.colors ||
          themePresets[0].colors;
        applyColors(colors);
      },

      clearOrganizationTheme: () => {
        const { customColors, presetId } = get();
        set({ organizationTheme: null, isOrgThemeActive: false });
        // Revert to user's personal theme
        const colors = customColors ||
          themePresets.find((p) => p.id === presetId)?.colors ||
          themePresets[0].colors;
        applyColors(colors);
      },

      updateOrganizationTheme: (updates) => {
        const { organizationTheme } = get();
        if (!organizationTheme) return;

        const newTheme = { ...organizationTheme, ...updates };
        set({ organizationTheme: newTheme });

        if (updates.customColors || updates.presetId) {
          const colors = newTheme.customColors ||
            themePresets.find((p) => p.id === newTheme.presetId)?.colors ||
            themePresets[0].colors;
          applyColors(colors);
        }
      },

      getCurrentColors: () => {
        const { customColors, presetId } = get();
        if (customColors) return customColors;
        const preset = themePresets.find((p) => p.id === presetId);
        return preset?.colors || themePresets[0].colors;
      },

      getCurrentPreset: () => {
        const { presetId } = get();
        return themePresets.find((p) => p.id === presetId);
      },

      getEffectiveColors: () => {
        const { organizationTheme, isOrgThemeActive, customColors, presetId } = get();

        // Organization theme takes precedence
        if (isOrgThemeActive && organizationTheme) {
          if (organizationTheme.customColors) return organizationTheme.customColors;
          const orgPreset = themePresets.find((p) => p.id === organizationTheme.presetId);
          if (orgPreset) return orgPreset.colors;
        }

        // Fall back to user's personal theme
        if (customColors) return customColors;
        const preset = themePresets.find((p) => p.id === presetId);
        return preset?.colors || themePresets[0].colors;
      },
    }),
    {
      name: 'billforge-theme',
      onRehydrateStorage: () => (state) => {
        if (state) {
          applyMode(state.mode);

          // Apply organization theme if active, otherwise user theme
          let colors: ThemeColors;
          if (state.isOrgThemeActive && state.organizationTheme) {
            colors = state.organizationTheme.customColors ||
              themePresets.find((p) => p.id === state.organizationTheme?.presetId)?.colors ||
              themePresets[0].colors;
          } else {
            colors = state.customColors ||
              themePresets.find((p) => p.id === state.presetId)?.colors ||
              themePresets[0].colors;
          }
          applyColors(colors);
        }
      },
    }
  )
);

// Apply theme mode to document
function applyMode(mode: ThemeMode) {
  if (typeof window === 'undefined') return;

  const root = document.documentElement;
  
  if (mode === 'system') {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    root.classList.toggle('dark', prefersDark);
  } else {
    root.classList.toggle('dark', mode === 'dark');
  }
}

// Apply theme colors to CSS variables
function applyColors(colors: ThemeColors) {
  if (typeof window === 'undefined') return;

  const root = document.documentElement;
  root.style.setProperty('--primary', colors.primary);
  root.style.setProperty('--accent', colors.accent);
  root.style.setProperty('--capture', colors.capture);
  root.style.setProperty('--processing', colors.processing);
  root.style.setProperty('--vendor', colors.vendor);
  root.style.setProperty('--reporting', colors.reporting);
  root.style.setProperty('--ring', colors.primary);
}

// Initialize theme on client
if (typeof window !== 'undefined') {
  const stored = localStorage.getItem('billforge-theme');
  if (stored) {
    try {
      const parsed = JSON.parse(stored);
      if (parsed.state) {
        applyMode(parsed.state.mode);
        const colors = parsed.state.customColors ||
          themePresets.find((p) => p.id === parsed.state.presetId)?.colors ||
          themePresets[0].colors;
        applyColors(colors);
      }
    } catch (e) {
      // Ignore parse errors
    }
  }
}
