'use client';

import { useState, useCallback, useMemo, useRef } from 'react';
import {
  useThemeStore,
  themePresets,
  ThemeColors,
  ThemePreset,
  generateGradient,
  getThemeByCategory,
  OrganizationThemeConfig,
} from '@/stores/theme';
import { useOrganizationTheme } from '@/components/organization-theme-provider';
import { GlassCard, GlassCardHeader, GlassCardTitle, GlassCardDescription, GlassCardContent } from './glass-card';
import { ModernGradientButton as GradientButton, AnimatedBorderButton } from './gradient-button';
import { Button } from './button';
import { Switch } from './switch';
import { Input } from './input';
import { Label } from './label';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';
import {
  Check,
  Palette,
  Sparkles,
  Sun,
  Moon,
  Monitor,
  Eye,
  EyeOff,
  RotateCcw,
  Save,
  Download,
  Upload,
  Star,
  Zap,
  Layers,
  Building2,
  Copy,
  ChevronRight,
  Grid3X3,
  Grip,
  RefreshCw,
} from 'lucide-react';

interface ThemeCustomizerProps {
  organizationId?: string;
  organizationName?: string;
  onSave?: (config: OrganizationThemeConfig) => Promise<void>;
  showModeSelector?: boolean;
  showOrgSettings?: boolean;
  compact?: boolean;
}

export function ThemeCustomizer({
  organizationId,
  organizationName = 'Organization',
  onSave,
  showModeSelector = true,
  showOrgSettings = true,
  compact = false,
}: ThemeCustomizerProps) {
  const {
    mode,
    setMode,
    presetId,
    setPreset,
    customColors,
    setCustomColors,
    clearCustomColors,
    organizationTheme,
    isOrgThemeActive,
    getCurrentColors,
  } = useThemeStore();

  const {
    isPreviewMode,
    startPreview,
    stopPreview,
    applyPreviewColors,
    getBrandGradient,
  } = useOrganizationTheme();

  const [activeCategory, setActiveCategory] = useState<string>('modern');
  const [isCustomizing, setIsCustomizing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [localColors, setLocalColors] = useState<ThemeColors>(getCurrentColors());

  // Color input refs
  const primaryInputRef = useRef<HTMLInputElement>(null);
  const accentInputRef = useRef<HTMLInputElement>(null);

  const categories = useMemo(() => [
    { id: 'bright', label: 'Bright', icon: Zap, description: 'Clean and vibrant' },
    { id: 'modern', label: 'Modern', icon: Layers, description: 'Dynamic effects' },
    { id: 'vibrant', label: 'Vibrant', icon: Sparkles, description: 'Bold colors' },
    { id: 'professional', label: 'Professional', icon: Building2, description: 'Elegant tones' },
  ], []);

  const currentCategoryPresets = useMemo(() =>
    getThemeByCategory(activeCategory),
    [activeCategory]
  );

  const handlePresetSelect = useCallback((preset: ThemePreset) => {
    setPreset(preset.id);
    setLocalColors(preset.colors);
    if (isPreviewMode) {
      applyPreviewColors(preset.colors);
    }
    toast.success(`${preset.name} theme applied`);
  }, [setPreset, isPreviewMode, applyPreviewColors]);

  const handleColorChange = useCallback((key: keyof ThemeColors, value: string) => {
    const newColors = { ...localColors, [key]: value };
    setLocalColors(newColors);
    setCustomColors(newColors);
    if (isPreviewMode) {
      applyPreviewColors(newColors);
    }
  }, [localColors, setCustomColors, isPreviewMode, applyPreviewColors]);

  const togglePreview = useCallback(() => {
    if (isPreviewMode) {
      stopPreview();
    } else {
      startPreview(localColors);
    }
  }, [isPreviewMode, localColors, startPreview, stopPreview]);

  const handleReset = useCallback(() => {
    const defaultPreset = themePresets.find(p => p.recommended) || themePresets[0];
    setPreset(defaultPreset.id);
    setLocalColors(defaultPreset.colors);
    clearCustomColors();
    toast.info('Reset to default theme');
  }, [setPreset, clearCustomColors]);

  const handleSave = async () => {
    if (!onSave) return;
    setIsSaving(true);
    try {
      await onSave({
        presetId,
        customColors: customColors || undefined,
        branding: { brandName: organizationName },
      });
      toast.success('Theme saved successfully');
      if (isPreviewMode) stopPreview();
    } catch (error) {
      toast.error('Failed to save theme');
    } finally {
      setIsSaving(false);
    }
  };

  const copyColorToClipboard = (color: string) => {
    navigator.clipboard.writeText(`hsl(${color})`);
    toast.success('Color copied to clipboard');
  };

  const brandGradient = getBrandGradient();

  // Mode options
  const modeOptions = [
    { id: 'light', label: 'Light', icon: Sun },
    { id: 'dark', label: 'Dark', icon: Moon },
    { id: 'system', label: 'System', icon: Monitor },
  ] as const;

  return (
    <div className={cn('space-y-6', compact && 'space-y-4')}>
      {/* Theme Mode Selector */}
      {showModeSelector && (
        <GlassCard variant="subtle" padding={compact ? 'sm' : 'md'}>
          <div className="flex items-center justify-between mb-4">
            <div>
              <h3 className="font-semibold text-foreground">Appearance</h3>
              <p className="text-sm text-muted-foreground">Choose your preferred theme mode</p>
            </div>
          </div>
          <div className="grid grid-cols-3 gap-2">
            {modeOptions.map((option) => (
              <button
                key={option.id}
                onClick={() => setMode(option.id)}
                className={cn(
                  'flex flex-col items-center gap-2 p-3 rounded-xl border-2 transition-all',
                  mode === option.id
                    ? 'border-primary bg-primary/5'
                    : 'border-border hover:border-primary/30 hover:bg-secondary/50'
                )}
              >
                <option.icon className={cn(
                  'w-5 h-5',
                  mode === option.id ? 'text-primary' : 'text-muted-foreground'
                )} />
                <span className={cn(
                  'text-sm font-medium',
                  mode === option.id ? 'text-primary' : 'text-foreground'
                )}>
                  {option.label}
                </span>
              </button>
            ))}
          </div>
        </GlassCard>
      )}

      {/* Color Themes */}
      <GlassCard variant="subtle" padding="none">
        {/* Category Tabs */}
        <div className="flex items-center gap-1 p-2 border-b border-border/50 overflow-x-auto">
          {categories.map((category) => {
            const isActive = activeCategory === category.id;
            return (
              <button
                key={category.id}
                onClick={() => setActiveCategory(category.id)}
                className={cn(
                  'flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium whitespace-nowrap transition-all',
                  isActive
                    ? 'bg-primary text-primary-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground hover:bg-secondary'
                )}
              >
                <category.icon className="w-4 h-4" />
                {category.label}
              </button>
            );
          })}
        </div>

        {/* Presets Grid */}
        <div className={cn('p-4', compact && 'p-3')}>
          <div className={cn(
            'grid gap-3',
            compact ? 'grid-cols-3' : 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-4'
          )}>
            {currentCategoryPresets.map((preset) => {
              const isSelected = presetId === preset.id && !customColors;
              const gradient = generateGradient(preset);

              return (
                <button
                  key={preset.id}
                  onClick={() => handlePresetSelect(preset)}
                  disabled={isOrgThemeActive}
                  className={cn(
                    'theme-card relative group',
                    isSelected && 'selected',
                    isOrgThemeActive && 'opacity-50 cursor-not-allowed'
                  )}
                >
                  {/* Preview gradient */}
                  <div
                    className="theme-card-preview"
                    style={{ background: gradient }}
                  />

                  {/* Info */}
                  <div className="theme-card-content">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-xs font-medium text-foreground truncate">
                        {preset.name}
                      </span>
                      <div className="flex items-center gap-1">
                        {preset.recommended && (
                          <Star className="w-3 h-3 text-warning fill-warning" />
                        )}
                        {isSelected && (
                          <Check className="w-3.5 h-3.5 text-primary" />
                        )}
                      </div>
                    </div>
                  </div>

                  {/* Hover overlay */}
                  <div className="absolute inset-0 rounded-2xl bg-primary/5 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" />
                </button>
              );
            })}
          </div>
        </div>
      </GlassCard>

      {/* Custom Colors */}
      <GlassCard variant="tinted" padding={compact ? 'sm' : 'md'}>
        <div className="flex items-center justify-between mb-4">
          <div>
            <h3 className="font-semibold text-foreground flex items-center gap-2">
              <Palette className="w-4 h-4 text-primary" />
              Custom Colors
            </h3>
            <p className="text-sm text-muted-foreground">Fine-tune your brand colors</p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsCustomizing(!isCustomizing)}
          >
            {isCustomizing ? 'Collapse' : 'Customize'}
            <ChevronRight className={cn(
              'w-4 h-4 ml-1 transition-transform',
              isCustomizing && 'rotate-90'
            )} />
          </Button>
        </div>

        {/* Quick color preview */}
        <div className="flex items-center gap-2 mb-4">
          <div
            className="w-10 h-10 rounded-xl shadow-sm cursor-pointer hover:scale-105 transition-transform"
            style={{ background: `hsl(${localColors.primary})` }}
            onClick={() => primaryInputRef.current?.click()}
            title="Primary color"
          />
          <div
            className="w-10 h-10 rounded-xl shadow-sm cursor-pointer hover:scale-105 transition-transform"
            style={{ background: `hsl(${localColors.accent})` }}
            onClick={() => accentInputRef.current?.click()}
            title="Accent color"
          />
          <div
            className="flex-1 h-10 rounded-xl shadow-inner overflow-hidden"
            style={{ background: brandGradient }}
          />
        </div>

        {/* Hidden color inputs for native picker */}
        <input
          ref={primaryInputRef}
          type="color"
          className="sr-only"
          onChange={(e) => {
            // Convert hex to HSL (simplified)
            const hex = e.target.value;
            toast.info('Use HSL values for precise control');
          }}
        />
        <input
          ref={accentInputRef}
          type="color"
          className="sr-only"
        />

        {/* Expanded customization */}
        {isCustomizing && (
          <div className="space-y-4 pt-4 border-t border-border/50 animate-slide-up">
            {/* Primary & Accent */}
            <div className="grid grid-cols-2 gap-4">
              <div>
                <Label className="text-xs">Primary (HSL)</Label>
                <div className="flex gap-2 mt-1.5">
                  <Input
                    value={localColors.primary}
                    onChange={(e) => handleColorChange('primary', e.target.value)}
                    placeholder="217 91% 60%"
                    className="font-mono text-sm"
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => copyColorToClipboard(localColors.primary)}
                  >
                    <Copy className="w-4 h-4" />
                  </Button>
                </div>
              </div>
              <div>
                <Label className="text-xs">Accent (HSL)</Label>
                <div className="flex gap-2 mt-1.5">
                  <Input
                    value={localColors.accent}
                    onChange={(e) => handleColorChange('accent', e.target.value)}
                    placeholder="185 100% 50%"
                    className="font-mono text-sm"
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => copyColorToClipboard(localColors.accent)}
                  >
                    <Copy className="w-4 h-4" />
                  </Button>
                </div>
              </div>
            </div>

            {/* Module Colors */}
            <div>
              <Label className="text-xs mb-2 block">Module Colors</Label>
              <div className="grid grid-cols-4 gap-2">
                {[
                  { key: 'capture' as const, label: 'Capture' },
                  { key: 'processing' as const, label: 'Process' },
                  { key: 'vendor' as const, label: 'Vendor' },
                  { key: 'reporting' as const, label: 'Report' },
                ].map((module) => (
                  <div key={module.key} className="text-center">
                    <div
                      className="w-full h-8 rounded-lg mb-1 cursor-pointer hover:scale-105 transition-transform shadow-sm"
                      style={{ background: `hsl(${localColors[module.key]})` }}
                      title={module.label}
                    />
                    <span className="text-[10px] text-muted-foreground">{module.label}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </GlassCard>

      {/* Preview Banner */}
      <div
        className="h-16 rounded-2xl flex items-center justify-center text-white font-semibold shadow-lg overflow-hidden relative"
        style={{ background: brandGradient }}
      >
        <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/10 to-transparent animate-[shimmer_3s_infinite]" />
        <span className="relative">{organizationName}</span>
      </div>

      {/* Actions */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={handleReset}
          >
            <RotateCcw className="w-4 h-4 mr-1.5" />
            Reset
          </Button>
          <Button
            variant={isPreviewMode ? 'default' : 'secondary'}
            size="sm"
            onClick={togglePreview}
          >
            {isPreviewMode ? (
              <>
                <EyeOff className="w-4 h-4 mr-1.5" />
                Stop Preview
              </>
            ) : (
              <>
                <Eye className="w-4 h-4 mr-1.5" />
                Live Preview
              </>
            )}
          </Button>
        </div>

        {onSave && (
          <GradientButton
            onClick={handleSave}
            loading={isSaving}
            size="sm"
          >
            <Save className="w-4 h-4 mr-1.5" />
            Save Theme
          </GradientButton>
        )}
      </div>

      {/* Organization Theme Notice */}
      {isOrgThemeActive && (
        <div className="p-4 rounded-xl bg-primary/5 border border-primary/20">
          <p className="text-sm text-foreground font-medium">
            Organization theme is active
          </p>
          <p className="text-xs text-muted-foreground mt-1">
            Personal preferences are currently overridden by your organization&apos;s theme settings.
          </p>
        </div>
      )}
    </div>
  );
}

// Compact theme selector for quick switching
interface QuickThemeSelectorProps {
  className?: string;
}

export function QuickThemeSelector({ className }: QuickThemeSelectorProps) {
  const { presetId, setPreset, getCurrentColors, customColors } = useThemeStore();
  const colors = getCurrentColors();

  // Get top 6 recommended/popular presets
  const quickPresets = useMemo(() =>
    themePresets
      .filter(p => p.recommended || ['brilliant-blue', 'electric', 'emerald', 'violet', 'sunset-glow', 'neon'].includes(p.id))
      .slice(0, 6),
    []
  );

  return (
    <div className={cn('flex items-center gap-2', className)}>
      {quickPresets.map((preset) => {
        const isSelected = presetId === preset.id && !customColors;
        const gradient = generateGradient(preset);

        return (
          <button
            key={preset.id}
            onClick={() => setPreset(preset.id)}
            className={cn(
              'w-8 h-8 rounded-lg transition-all hover:scale-110',
              isSelected && 'ring-2 ring-primary ring-offset-2 ring-offset-background'
            )}
            style={{ background: gradient }}
            title={preset.name}
          />
        );
      })}
    </div>
  );
}

// Color Palette Display Component
interface ColorPaletteDisplayProps {
  colors: ThemeColors;
  showLabels?: boolean;
  className?: string;
}

export function ColorPaletteDisplay({ colors, showLabels = false, className }: ColorPaletteDisplayProps) {
  const colorEntries = [
    { key: 'primary', label: 'Primary' },
    { key: 'accent', label: 'Accent' },
    { key: 'capture', label: 'Capture' },
    { key: 'processing', label: 'Process' },
    { key: 'vendor', label: 'Vendor' },
    { key: 'reporting', label: 'Report' },
  ] as const;

  return (
    <div className={cn('flex gap-1', className)}>
      {colorEntries.map(({ key, label }) => (
        <div key={key} className="flex-1">
          <div
            className="w-full h-6 rounded-md first:rounded-l-lg last:rounded-r-lg"
            style={{ background: `hsl(${colors[key]})` }}
            title={`${label}: hsl(${colors[key]})`}
          />
          {showLabels && (
            <p className="text-[9px] text-muted-foreground text-center mt-1 truncate">
              {label}
            </p>
          )}
        </div>
      ))}
    </div>
  );
}
