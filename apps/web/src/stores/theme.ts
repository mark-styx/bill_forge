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
}

// Modern, bright theme presets
export const themePresets: ThemePreset[] = [
  {
    id: 'ocean',
    name: 'Ocean Blue',
    description: 'Clean and professional with calming blue tones',
    preview: 'from-blue-500 to-cyan-400',
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
    id: 'emerald',
    name: 'Emerald',
    description: 'Fresh and modern with vibrant greens',
    preview: 'from-emerald-500 to-teal-400',
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
    id: 'violet',
    name: 'Violet',
    description: 'Creative and bold with purple accents',
    preview: 'from-violet-500 to-purple-400',
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
    id: 'slate',
    name: 'Slate',
    description: 'Minimalist and sophisticated',
    preview: 'from-slate-500 to-slate-400',
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
    id: 'sunset',
    name: 'Sunset',
    description: 'Energetic with warm gradients',
    preview: 'from-orange-500 to-pink-500',
    colors: {
      primary: '25 95% 55%',
      accent: '340 80% 55%',
      capture: '195 90% 48%',
      processing: '155 75% 40%',
      vendor: '280 70% 58%',
      reporting: '40 95% 52%',
    },
  },
];

interface ThemeState {
  mode: ThemeMode;
  presetId: string;
  customColors: ThemeColors | null;
  sidebarCollapsed: boolean;

  // Actions
  setMode: (mode: ThemeMode) => void;
  setPreset: (presetId: string) => void;
  setCustomColors: (colors: ThemeColors) => void;
  clearCustomColors: () => void;
  toggleSidebar: () => void;
  
  // Getters
  getCurrentColors: () => ThemeColors;
  getCurrentPreset: () => ThemePreset | undefined;
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      mode: 'light',
      presetId: 'ocean',
      customColors: null,
      sidebarCollapsed: false,

      setMode: (mode) => {
        set({ mode });
        applyMode(mode);
      },

      setPreset: (presetId) => {
        set({ presetId, customColors: null });
        const preset = themePresets.find((p) => p.id === presetId);
        if (preset) {
          applyColors(preset.colors);
        }
      },

      setCustomColors: (colors) => {
        set({ customColors: colors });
        applyColors(colors);
      },

      clearCustomColors: () => {
        const { presetId } = get();
        set({ customColors: null });
        const preset = themePresets.find((p) => p.id === presetId);
        if (preset) {
          applyColors(preset.colors);
        }
      },

      toggleSidebar: () => {
        set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed }));
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
    }),
    {
      name: 'billforge-theme',
      onRehydrateStorage: () => (state) => {
        if (state) {
          applyMode(state.mode);
          const colors = state.customColors || 
            themePresets.find((p) => p.id === state.presetId)?.colors ||
            themePresets[0].colors;
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
