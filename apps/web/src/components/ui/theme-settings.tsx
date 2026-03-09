'use client';

import { useState, useRef } from 'react';
import {
  useThemeStore,
  themePresets,
  ThemeColors,
  exportThemeConfig,
  importThemeConfig,
  generateGradient,
} from '@/stores/theme';
import { organizationThemeApi, userThemeApi } from '@/lib/api';
import { ColorPicker, GradientPicker } from './color-picker';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from './card';
import { Button } from './button';
import {
  Download,
  Upload,
  Save,
  RotateCcw,
  Eye,
  EyeOff,
  Check,
  Copy,
  AlertCircle,
  Building2,
  Palette,
  Image,
  FileJson,
  ChevronDown,
  Sparkles,
} from 'lucide-react';
import { toast } from 'sonner';

interface ThemeSettingsProps {
  mode: 'organization' | 'user';
  onSave?: () => void;
  showAdvanced?: boolean;
}

export function ThemeSettings({ mode, onSave, showAdvanced = true }: ThemeSettingsProps) {
  const {
    presetId,
    setPreset,
    customColors,
    setCustomColors,
    clearCustomColors,
    organizationTheme,
    setOrganizationTheme,
    clearOrganizationTheme,
    getCurrentColors,
    isOrgThemeActive,
  } = useThemeStore();

  const fileInputRef = useRef<HTMLInputElement>(null);
  const logoInputRef = useRef<HTMLInputElement>(null);

  const [isPreviewMode, setIsPreviewMode] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [showAdvancedOptions, setShowAdvancedOptions] = useState(false);

  // Form state
  const [brandName, setBrandName] = useState(organizationTheme?.branding?.brandName || 'BillForge');
  const [selectedPresetId, setSelectedPresetId] = useState(presetId);
  const [colors, setColors] = useState<ThemeColors>(getCurrentColors());
  const [useCustomColors, setUseCustomColors] = useState(!!customColors);
  const [gradientEnabled, setGradientEnabled] = useState(false);
  const [gradientConfig, setGradientConfig] = useState({
    from: colors.primary,
    to: colors.accent,
    angle: 135,
  });

  // Preview colors in real-time
  const previewColors = (newColors: ThemeColors) => {
    if (!isPreviewMode) return;

    const root = document.documentElement;
    Object.entries(newColors).forEach(([key, value]) => {
      root.style.setProperty(`--${key}`, value);
    });
    root.style.setProperty('--ring', newColors.primary);
  };

  const handleColorChange = (key: keyof ThemeColors, value: string) => {
    const newColors = { ...colors, [key]: value };
    setColors(newColors);
    setUseCustomColors(true);
    previewColors(newColors);
  };

  const handlePresetSelect = (preset: typeof themePresets[0]) => {
    setSelectedPresetId(preset.id);
    setColors(preset.colors);
    setUseCustomColors(false);
    if (preset.gradient) {
      setGradientConfig({
        from: preset.gradient.from,
        to: preset.gradient.to,
        angle: preset.gradient.angle || 135,
      });
    }
    previewColors(preset.colors);
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      if (mode === 'organization') {
        // Save organization theme
        await organizationThemeApi.saveTheme({
          preset_id: selectedPresetId,
          custom_colors: useCustomColors ? colors : undefined,
          branding: {
            brandName,
          },
          enabled_for_all_users: true,
          allow_user_override: true,
          gradient_config: gradientEnabled
            ? {
                enabled: true,
                type: 'linear',
                angle: gradientConfig.angle,
              }
            : undefined,
        });

        // Update local state
        setOrganizationTheme({
          presetId: selectedPresetId,
          customColors: useCustomColors ? colors : undefined,
          branding: { brandName },
          enabledForAllUsers: true,
          allowUserOverride: true,
        });

        toast.success('Organization theme saved');
      } else {
        // Save user theme preference
        await userThemeApi.savePreferences({
          preset_id: selectedPresetId,
          custom_colors: useCustomColors ? colors : undefined,
          mode: 'system',
        });

        // Update local state
        if (useCustomColors) {
          setCustomColors(colors);
        } else {
          clearCustomColors();
          setPreset(selectedPresetId);
        }

        toast.success('Theme preferences saved');
      }

      onSave?.();
    } catch (error) {
      toast.error('Failed to save theme');
      console.error(error);
    } finally {
      setIsSaving(false);
    }
  };

  const handleExport = () => {
    const config = exportThemeConfig({
      presetId: selectedPresetId,
      customColors: useCustomColors ? colors : null,
      organizationTheme: mode === 'organization'
        ? {
            presetId: selectedPresetId,
            customColors: useCustomColors ? colors : undefined,
            branding: { brandName },
          }
        : null,
    });

    const blob = new Blob([config], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `billforge-theme-${new Date().toISOString().split('T')[0]}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    toast.success('Theme exported');
  };

  const handleImport = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const imported = importThemeConfig(content);

      if (imported) {
        setSelectedPresetId(imported.presetId);
        if (imported.customColors) {
          setColors(imported.customColors);
          setUseCustomColors(true);
        } else {
          const preset = themePresets.find((p) => p.id === imported.presetId);
          if (preset) {
            setColors(preset.colors);
          }
          setUseCustomColors(false);
        }
        previewColors(imported.customColors || colors);
        toast.success('Theme imported');
      } else {
        toast.error('Invalid theme file');
      }
    };
    reader.readAsText(file);

    // Reset input
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleLogoUpload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    try {
      const result = await organizationThemeApi.uploadLogo(file, 'logo');
      toast.success('Logo uploaded');
      // Update branding state with new logo URL
    } catch (error) {
      toast.error('Failed to upload logo');
    }

    if (logoInputRef.current) {
      logoInputRef.current.value = '';
    }
  };

  const handleReset = () => {
    const defaultPreset = themePresets[0];
    setSelectedPresetId(defaultPreset.id);
    setColors(defaultPreset.colors);
    setUseCustomColors(false);
    setBrandName('BillForge');
    setGradientEnabled(false);
    previewColors(defaultPreset.colors);
    toast.info('Reset to defaults');
  };

  const copyCSS = () => {
    const css = Object.entries(colors)
      .map(([key, value]) => `  --${key}: ${value};`)
      .join('\n');
    navigator.clipboard.writeText(`:root {\n${css}\n}`);
    toast.success('CSS copied to clipboard');
  };

  return (
    <div className="space-y-6">
      {/* Header Actions */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Button
            variant={isPreviewMode ? 'default' : 'secondary'}
            size="sm"
            onClick={() => setIsPreviewMode(!isPreviewMode)}
          >
            {isPreviewMode ? <EyeOff className="w-4 h-4 mr-2" /> : <Eye className="w-4 h-4 mr-2" />}
            {isPreviewMode ? 'Exit Preview' : 'Live Preview'}
          </Button>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={handleReset}>
            <RotateCcw className="w-4 h-4 mr-2" />
            Reset
          </Button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".json"
            onChange={handleImport}
            className="hidden"
          />
          <Button variant="secondary" size="sm" onClick={() => fileInputRef.current?.click()}>
            <Upload className="w-4 h-4 mr-2" />
            Import
          </Button>
          <Button variant="secondary" size="sm" onClick={handleExport}>
            <Download className="w-4 h-4 mr-2" />
            Export
          </Button>
        </div>
      </div>

      {/* Organization Branding */}
      {mode === 'organization' && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Building2 className="w-5 h-5" />
              Organization Branding
            </CardTitle>
            <CardDescription>
              Customize your organization's brand identity
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* Brand Name */}
            <div>
              <label className="text-sm font-medium text-foreground block mb-1.5">
                Brand Name
              </label>
              <input
                type="text"
                value={brandName}
                onChange={(e) => setBrandName(e.target.value)}
                className="input max-w-sm"
                placeholder="Your Company Name"
              />
            </div>

            {/* Logo Upload */}
            <div>
              <label className="text-sm font-medium text-foreground block mb-1.5">
                Logo
              </label>
              <div className="flex items-center gap-4">
                <div className="w-16 h-16 rounded-xl bg-gradient-to-br from-primary to-accent flex items-center justify-center">
                  <span className="text-2xl font-bold text-white">
                    {brandName?.[0]?.toUpperCase() || 'B'}
                  </span>
                </div>
                <div>
                  <input
                    ref={logoInputRef}
                    type="file"
                    accept="image/*"
                    onChange={handleLogoUpload}
                    className="hidden"
                  />
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => logoInputRef.current?.click()}
                  >
                    <Image className="w-4 h-4 mr-2" />
                    Upload Logo
                  </Button>
                  <p className="text-xs text-muted-foreground mt-1">
                    PNG, SVG, or JPG. Recommended: 256x256px
                  </p>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Theme Presets */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Palette className="w-5 h-5" />
            Color Theme
          </CardTitle>
          <CardDescription>
            Select a preset theme or customize your colors
          </CardDescription>
        </CardHeader>
        <CardContent>
          {/* Category Groups */}
          {['modern', 'bright', 'vibrant', 'professional'].map((category) => {
            const categoryPresets = themePresets.filter((p) => p.category === category);
            if (categoryPresets.length === 0) return null;

            return (
              <div key={category} className="mb-6 last:mb-0">
                <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                  {category === 'modern' && 'Modern & Dynamic'}
                  {category === 'bright' && 'Bright & Clean'}
                  {category === 'vibrant' && 'Vibrant & Bold'}
                  {category === 'professional' && 'Professional'}
                </p>
                <div className="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-5 gap-3">
                  {categoryPresets.map((preset) => {
                    const isSelected = selectedPresetId === preset.id && !useCustomColors;
                    const gradient = generateGradient(preset);

                    return (
                      <button
                        key={preset.id}
                        onClick={() => handlePresetSelect(preset)}
                        className={`
                          relative p-2 rounded-xl border-2 text-left transition-all
                          ${isSelected ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/30'}
                        `}
                      >
                        <div
                          className="w-full h-10 rounded-lg mb-1.5"
                          style={{ background: gradient }}
                        />
                        <p className="font-medium text-foreground text-xs truncate">
                          {preset.name}
                        </p>
                        {isSelected && (
                          <div className="absolute top-1.5 right-1.5 w-4 h-4 rounded-full bg-primary flex items-center justify-center">
                            <Check className="w-2.5 h-2.5 text-white" />
                          </div>
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </CardContent>
      </Card>

      {/* Custom Colors */}
      {showAdvanced && (
        <Card>
          <CardHeader>
            <button
              onClick={() => setShowAdvancedOptions(!showAdvancedOptions)}
              className="w-full flex items-center justify-between"
            >
              <div className="text-left">
                <CardTitle className="flex items-center gap-2">
                  <Sparkles className="w-5 h-5" />
                  Custom Colors
                </CardTitle>
                <CardDescription>
                  Fine-tune individual colors for your theme
                </CardDescription>
              </div>
              <ChevronDown
                className={`w-5 h-5 text-muted-foreground transition-transform ${
                  showAdvancedOptions ? 'rotate-180' : ''
                }`}
              />
            </button>
          </CardHeader>
          {showAdvancedOptions && (
            <CardContent className="space-y-6">
              {/* Main Colors */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div>
                  <label className="text-sm font-medium text-foreground block mb-2">
                    Primary Color
                  </label>
                  <ColorPicker
                    value={colors.primary}
                    onChange={(value) => handleColorChange('primary', value)}
                    showGradientPreview
                    gradientWith={colors.accent}
                  />
                </div>
                <div>
                  <label className="text-sm font-medium text-foreground block mb-2">
                    Accent Color
                  </label>
                  <ColorPicker
                    value={colors.accent}
                    onChange={(value) => handleColorChange('accent', value)}
                  />
                </div>
              </div>

              {/* Module Colors */}
              <div>
                <h4 className="text-sm font-medium text-foreground mb-3">Module Colors</h4>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  {[
                    { key: 'capture', label: 'Capture', icon: '📥' },
                    { key: 'processing', label: 'Processing', icon: '⚡' },
                    { key: 'vendor', label: 'Vendors', icon: '🏢' },
                    { key: 'reporting', label: 'Reports', icon: '📊' },
                  ].map((module) => (
                    <div key={module.key} className="p-3 bg-secondary/30 rounded-xl">
                      <div className="flex items-center gap-2 mb-2">
                        <span>{module.icon}</span>
                        <span className="text-sm font-medium">{module.label}</span>
                      </div>
                      <div className="flex items-center gap-2">
                        <div
                          className="w-8 h-8 rounded-lg border border-border"
                          style={{ backgroundColor: `hsl(${colors[module.key as keyof ThemeColors]})` }}
                        />
                        <input
                          type="text"
                          value={colors[module.key as keyof ThemeColors]}
                          onChange={(e) => handleColorChange(module.key as keyof ThemeColors, e.target.value)}
                          className="input text-xs font-mono flex-1"
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* Gradient Configuration */}
              <div>
                <div className="flex items-center justify-between mb-3">
                  <h4 className="text-sm font-medium text-foreground">Brand Gradient</h4>
                  <button
                    onClick={() => setGradientEnabled(!gradientEnabled)}
                    className={`
                      relative w-11 h-6 rounded-full transition-colors
                      ${gradientEnabled ? 'bg-primary' : 'bg-secondary'}
                    `}
                  >
                    <span
                      className={`
                        absolute top-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform
                        ${gradientEnabled ? 'translate-x-5 left-0.5' : 'left-0.5'}
                      `}
                    />
                  </button>
                </div>
                {gradientEnabled && (
                  <div className="animate-scale-in">
                    <GradientPicker
                      fromColor={gradientConfig.from}
                      toColor={gradientConfig.to}
                      angle={gradientConfig.angle}
                      onChange={(config) => setGradientConfig(config)}
                    />
                  </div>
                )}
              </div>

              {/* CSS Output */}
              <div className="pt-4 border-t border-border">
                <div className="flex items-center justify-between mb-2">
                  <h4 className="text-sm font-medium text-foreground flex items-center gap-2">
                    <FileJson className="w-4 h-4" />
                    CSS Variables
                  </h4>
                  <Button variant="ghost" size="sm" onClick={copyCSS}>
                    <Copy className="w-4 h-4 mr-2" />
                    Copy
                  </Button>
                </div>
                <pre className="p-4 bg-secondary/50 rounded-lg text-xs font-mono text-muted-foreground overflow-x-auto">
                  {`:root {
${Object.entries(colors)
  .map(([key, value]) => `  --${key}: ${value};`)
  .join('\n')}
}`}
                </pre>
              </div>
            </CardContent>
          )}
        </Card>
      )}

      {/* Preview */}
      <Card>
        <CardHeader>
          <CardTitle>Preview</CardTitle>
          <CardDescription>See how your theme looks in context</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="p-6 bg-background border border-border rounded-xl space-y-4">
            {/* Header Preview */}
            <div className="flex items-center gap-3">
              <div
                className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold"
                style={{ backgroundColor: `hsl(${colors.primary})` }}
              >
                {brandName?.[0]?.toUpperCase() || 'B'}
              </div>
              <div>
                <p className="font-semibold text-foreground">{brandName || 'Your Company'}</p>
                <p className="text-xs text-muted-foreground">Organization Dashboard</p>
              </div>
            </div>

            {/* Buttons */}
            <div className="flex flex-wrap gap-2">
              <button
                className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                style={{ backgroundColor: `hsl(${colors.primary})` }}
              >
                Primary
              </button>
              <button
                className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                style={{ backgroundColor: `hsl(${colors.accent})` }}
              >
                Accent
              </button>
              <button className="px-4 py-2 rounded-lg bg-secondary text-secondary-foreground text-sm font-medium">
                Secondary
              </button>
            </div>

            {/* Module Badges */}
            <div className="flex flex-wrap gap-2">
              {[
                { key: 'capture', label: 'Capture' },
                { key: 'processing', label: 'Processing' },
                { key: 'vendor', label: 'Vendor' },
                { key: 'reporting', label: 'Reporting' },
              ].map((badge) => (
                <span
                  key={badge.key}
                  className="px-2.5 py-1 rounded-full text-xs font-medium"
                  style={{
                    backgroundColor: `hsl(${colors[badge.key as keyof ThemeColors]} / 0.15)`,
                    color: `hsl(${colors[badge.key as keyof ThemeColors]})`,
                  }}
                >
                  {badge.label}
                </span>
              ))}
            </div>

            {/* Gradient Preview */}
            {gradientEnabled && (
              <div
                className="h-16 rounded-lg"
                style={{
                  background: `linear-gradient(${gradientConfig.angle}deg, hsl(${gradientConfig.from}), hsl(${gradientConfig.to}))`,
                }}
              />
            )}
          </div>
        </CardContent>
      </Card>

      {/* Save Button */}
      <div className="flex justify-end gap-3">
        <Button variant="secondary" onClick={handleReset}>
          Cancel
        </Button>
        <Button onClick={handleSave} loading={isSaving}>
          <Save className="w-4 h-4 mr-2" />
          Save {mode === 'organization' ? 'Organization' : ''} Theme
        </Button>
      </div>
    </div>
  );
}
