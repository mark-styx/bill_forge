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

export interface ThemePreset {
  id: string;
  name: string;
  description: string;
  colors: ThemeColors;
  preview: string; // Gradient for preview
  category: 'bright' | 'professional' | 'vibrant';
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
    colors: {
      primary: '217 91% 60%',
      accent: '199 89% 48%',
      capture: '199 89% 48%',
      processing: '162 78% 45%',
      vendor: '262 83% 58%',
      reporting: '43 96% 56%',
    },
  },
  // Vibrant Category
  {
    id: 'violet',
    name: 'Violet',
    description: 'Creative and bold with purple accents',
    preview: 'from-violet-500 to-purple-400',
    category: 'vibrant',
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
    colors: {
      primary: '84 85% 45%',
      accent: '142 76% 45%',
      capture: '199 89% 48%',
      processing: '84 85% 45%',
      vendor: '262 83% 58%',
      reporting: '43 96% 56%',
    },
  },
  // Professional Category
  {
    id: 'slate',
    name: 'Slate',
    description: 'Minimalist and sophisticated',
    preview: 'from-slate-500 to-slate-400',
    category: 'professional',
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
    colors: {
      primary: '240 5% 34%',
      accent: '220 14% 46%',
      capture: '199 89% 48%',
      processing: '162 78% 45%',
      vendor: '262 68% 55%',
      reporting: '43 96% 56%',
    },
  },
];

// Organization theme that can be stored/synced with backend
export interface OrganizationThemeConfig {
  presetId: string;
  customColors?: ThemeColors;
  logoUrl?: string;
  logoMark?: string;
  brandName?: string;
  brandGradient?: string;
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
