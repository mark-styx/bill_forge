'use client';

import { useState, useRef, useCallback, useEffect } from 'react';
import {
  useThemeStore,
  themePresets,
  ThemeColors,
  ThemePreset,
  OrganizationThemeConfig,
  exportThemeConfig,
  importThemeConfig,
  generateGradient,
  getThemeByCategory,
} from '@/stores/theme';
import { organizationThemeApi } from '@/lib/api';
import { ColorPicker, GradientPicker, ColorSwatch } from './color-picker';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from './card';
import { Button } from './button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';
import { Switch } from './switch';
import { Input } from './input';
import { Label } from './label';
import { toast } from 'sonner';
import {
  Download,
  Upload,
  Save,
  RotateCcw,
  Eye,
  EyeOff,
  Check,
  Copy,
  Building2,
  Palette,
  Image,
  FileJson,
  ChevronDown,
  ChevronRight,
  Sparkles,
  Globe,
  Users,
  Shield,
  Paintbrush,
  Monitor,
  Smartphone,
  Layers,
  Trash2,
  RefreshCw,
  AlertTriangle,
  Wand2,
  Sun,
  Moon,
} from 'lucide-react';

interface OrganizationThemeSettingsProps {
  organizationId?: string;
  organizationName?: string;
  onSave?: (theme: OrganizationThemeConfig) => void;
  onCancel?: () => void;
}

export function OrganizationThemeSettings({
  organizationId,
  organizationName = 'Your Organization',
  onSave,
  onCancel,
}: OrganizationThemeSettingsProps) {
  const {
    presetId,
    organizationTheme,
    setOrganizationTheme,
    getCurrentColors,
  } = useThemeStore();

  const fileInputRef = useRef<HTMLInputElement>(null);
  const logoInputRef = useRef<HTMLInputElement>(null);
  const logoMarkInputRef = useRef<HTMLInputElement>(null);
  const faviconInputRef = useRef<HTMLInputElement>(null);

  // Form state
  const [activeTab, setActiveTab] = useState('presets');
  const [isPreviewMode, setIsPreviewMode] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

  // Branding
  const [brandName, setBrandName] = useState(organizationTheme?.branding?.brandName || organizationName);
  const [logoUrl, setLogoUrl] = useState(organizationTheme?.branding?.logoUrl || '');
  const [logoMarkUrl, setLogoMarkUrl] = useState(organizationTheme?.branding?.logoMark || '');
  const [faviconUrl, setFaviconUrl] = useState(organizationTheme?.branding?.faviconUrl || '');

  // Theme
  const [selectedPresetId, setSelectedPresetId] = useState(organizationTheme?.presetId || presetId);
  const [useCustomColors, setUseCustomColors] = useState(!!organizationTheme?.customColors);
  const [colors, setColors] = useState<ThemeColors>(
    organizationTheme?.customColors || getCurrentColors()
  );

  // Gradient
  const [gradientEnabled, setGradientEnabled] = useState(
    organizationTheme?.gradientConfig?.enabled || false
  );
  const [gradientConfig, setGradientConfig] = useState({
    from: colors.primary,
    to: colors.accent,
    via: '',
    angle: organizationTheme?.gradientConfig?.angle || 135,
  });

  // Permissions
  const [enabledForAllUsers, setEnabledForAllUsers] = useState(
    organizationTheme?.enabledForAllUsers ?? true
  );
  const [allowUserOverride, setAllowUserOverride] = useState(
    organizationTheme?.allowUserOverride ?? true
  );

  // Preview state
  const [previewDevice, setPreviewDevice] = useState<'desktop' | 'tablet' | 'mobile'>('desktop');
  const [previewDarkMode, setPreviewDarkMode] = useState(false);

  // Track changes
  useEffect(() => {
    setHasUnsavedChanges(true);
  }, [brandName, selectedPresetId, colors, gradientEnabled, gradientConfig, enabledForAllUsers, allowUserOverride]);

  // Preview colors in real-time
  const applyPreviewColors = useCallback((newColors: ThemeColors) => {
    if (!isPreviewMode) return;

    const root = document.documentElement;
    Object.entries(newColors).forEach(([key, value]) => {
      root.style.setProperty(`--${key}`, value);
    });
    root.style.setProperty('--ring', newColors.primary);
  }, [isPreviewMode]);

  const handleColorChange = (key: keyof ThemeColors, value: string) => {
    const newColors = { ...colors, [key]: value };
    setColors(newColors);
    setUseCustomColors(true);
    applyPreviewColors(newColors);
  };

  const handlePresetSelect = (preset: ThemePreset) => {
    setSelectedPresetId(preset.id);
    setColors(preset.colors);
    setUseCustomColors(false);
    if (preset.gradient) {
      setGradientConfig({
        from: preset.gradient.from,
        to: preset.gradient.to,
        via: preset.gradient.via || '',
        angle: preset.gradient.angle || 135,
      });
    }
    applyPreviewColors(preset.colors);
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      const themeConfig: OrganizationThemeConfig = {
        presetId: selectedPresetId,
        customColors: useCustomColors ? colors : undefined,
        branding: {
          brandName,
          logoUrl: logoUrl || undefined,
          logoMark: logoMarkUrl || undefined,
          faviconUrl: faviconUrl || undefined,
        },
        enabledForAllUsers,
        allowUserOverride,
        gradientConfig: gradientEnabled
          ? {
              enabled: true,
              type: 'linear',
              angle: gradientConfig.angle,
            }
          : undefined,
      };

      // Save to API
      await organizationThemeApi.saveTheme({
        preset_id: selectedPresetId,
        custom_colors: useCustomColors ? colors : undefined,
        branding: themeConfig.branding,
        enabled_for_all_users: enabledForAllUsers,
        allow_user_override: allowUserOverride,
        gradient_config: themeConfig.gradientConfig,
      });

      // Update local state
      setOrganizationTheme(themeConfig);
      setHasUnsavedChanges(false);
      toast.success('Organization theme saved successfully');
      onSave?.(themeConfig);
    } catch (error) {
      console.error('Failed to save theme:', error);
      toast.error('Failed to save organization theme');
    } finally {
      setIsSaving(false);
    }
  };

  const handleExport = () => {
    const config = exportThemeConfig({
      presetId: selectedPresetId,
      customColors: useCustomColors ? colors : null,
      organizationTheme: {
        presetId: selectedPresetId,
        customColors: useCustomColors ? colors : undefined,
        branding: { brandName, logoUrl, logoMark: logoMarkUrl, faviconUrl },
        enabledForAllUsers,
        allowUserOverride,
      },
    });

    const blob = new Blob([config], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${brandName.toLowerCase().replace(/\s+/g, '-')}-theme-${new Date().toISOString().split('T')[0]}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    toast.success('Theme configuration exported');
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
        if (imported.organizationTheme?.branding) {
          setBrandName(imported.organizationTheme.branding.brandName || brandName);
        }
        applyPreviewColors(imported.customColors || colors);
        toast.success('Theme configuration imported');
      } else {
        toast.error('Invalid theme configuration file');
      }
    };
    reader.readAsText(file);

    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleLogoUpload = async (
    event: React.ChangeEvent<HTMLInputElement>,
    type: 'logo' | 'logoMark' | 'favicon'
  ) => {
    const file = event.target.files?.[0];
    if (!file) return;

    try {
      const result = await organizationThemeApi.uploadLogo(file, type);
      const url = result.url || URL.createObjectURL(file);

      switch (type) {
        case 'logo':
          setLogoUrl(url);
          break;
        case 'logoMark':
          setLogoMarkUrl(url);
          break;
        case 'favicon':
          setFaviconUrl(url);
          break;
      }
      toast.success(`${type === 'logoMark' ? 'Logo mark' : type.charAt(0).toUpperCase() + type.slice(1)} uploaded`);
    } catch (error) {
      toast.error(`Failed to upload ${type}`);
    }
  };

  const handleReset = () => {
    const defaultPreset = themePresets[0];
    setSelectedPresetId(defaultPreset.id);
    setColors(defaultPreset.colors);
    setUseCustomColors(false);
    setGradientEnabled(false);
    applyPreviewColors(defaultPreset.colors);
    toast.info('Reset to default theme');
  };

  const handleDeleteLogo = async (type: 'logo' | 'logoMark' | 'favicon') => {
    try {
      await organizationThemeApi.deleteLogo(type);
      switch (type) {
        case 'logo':
          setLogoUrl('');
          break;
        case 'logoMark':
          setLogoMarkUrl('');
          break;
        case 'favicon':
          setFaviconUrl('');
          break;
      }
      toast.success(`${type === 'logoMark' ? 'Logo mark' : type.charAt(0).toUpperCase() + type.slice(1)} removed`);
    } catch (error) {
      toast.error(`Failed to remove ${type}`);
    }
  };

  const getCurrentPreset = () => themePresets.find((p) => p.id === selectedPresetId);
  const categories = ['modern', 'bright', 'vibrant', 'professional'] as const;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold text-foreground flex items-center gap-2">
            <Building2 className="w-5 h-5 text-primary" />
            Organization Theme
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            Customize the appearance for all users in {organizationName}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {hasUnsavedChanges && (
            <span className="text-xs text-warning flex items-center gap-1">
              <AlertTriangle className="w-3 h-3" />
              Unsaved changes
            </span>
          )}
          <Button
            variant={isPreviewMode ? 'default' : 'secondary'}
            size="sm"
            onClick={() => setIsPreviewMode(!isPreviewMode)}
          >
            {isPreviewMode ? <EyeOff className="w-4 h-4 mr-2" /> : <Eye className="w-4 h-4 mr-2" />}
            {isPreviewMode ? 'Exit Preview' : 'Live Preview'}
          </Button>
        </div>
      </div>

      {/* Main Content */}
      <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
        <TabsList className="bg-secondary/50 p-1">
          <TabsTrigger value="presets" className="flex items-center gap-2">
            <Palette className="w-4 h-4" />
            Presets
          </TabsTrigger>
          <TabsTrigger value="colors" className="flex items-center gap-2">
            <Paintbrush className="w-4 h-4" />
            Custom Colors
          </TabsTrigger>
          <TabsTrigger value="branding" className="flex items-center gap-2">
            <Building2 className="w-4 h-4" />
            Branding
          </TabsTrigger>
          <TabsTrigger value="settings" className="flex items-center gap-2">
            <Shield className="w-4 h-4" />
            Settings
          </TabsTrigger>
        </TabsList>

        {/* Presets Tab */}
        <TabsContent value="presets" className="space-y-6">
          {categories.map((category) => {
            const categoryPresets = getThemeByCategory(category);
            if (categoryPresets.length === 0) return null;

            const categoryLabels: Record<string, string> = {
              modern: 'Modern & Dynamic',
              bright: 'Bright & Clean',
              vibrant: 'Vibrant & Bold',
              professional: 'Professional',
            };

            return (
              <Card key={category}>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">{categoryLabels[category]}</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3">
                    {categoryPresets.map((preset) => {
                      const isSelected = selectedPresetId === preset.id && !useCustomColors;
                      const gradient = generateGradient(preset);

                      return (
                        <button
                          key={preset.id}
                          onClick={() => handlePresetSelect(preset)}
                          className={`
                            relative p-2.5 rounded-xl border-2 text-left transition-all
                            ${isSelected ? 'border-primary bg-primary/5 shadow-sm' : 'border-border hover:border-primary/30 hover:shadow-sm'}
                          `}
                        >
                          <div
                            className="w-full h-10 rounded-lg mb-2"
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
                </CardContent>
              </Card>
            );
          })}
        </TabsContent>

        {/* Custom Colors Tab */}
        <TabsContent value="colors" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Sparkles className="w-5 h-5" />
                Custom Color Palette
              </CardTitle>
              <CardDescription>
                Fine-tune colors to match your brand identity
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* Primary Colors */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div>
                  <Label className="mb-2 block">Primary Color</Label>
                  <ColorPicker
                    value={colors.primary}
                    onChange={(value) => handleColorChange('primary', value)}
                    showGradientPreview
                    gradientWith={colors.accent}
                  />
                </div>
                <div>
                  <Label className="mb-2 block">Accent Color</Label>
                  <ColorPicker
                    value={colors.accent}
                    onChange={(value) => handleColorChange('accent', value)}
                  />
                </div>
              </div>

              {/* Module Colors */}
              <div className="pt-4 border-t border-border">
                <h4 className="text-sm font-medium text-foreground mb-4">Module Colors</h4>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  {[
                    { key: 'capture', label: 'Capture', description: 'Invoice OCR' },
                    { key: 'processing', label: 'Processing', description: 'Workflows' },
                    { key: 'vendor', label: 'Vendors', description: 'Supplier mgmt' },
                    { key: 'reporting', label: 'Reports', description: 'Analytics' },
                  ].map((module) => (
                    <div key={module.key} className="p-3 bg-secondary/30 rounded-xl">
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm font-medium">{module.label}</span>
                        <div
                          className="w-6 h-6 rounded-md border border-border"
                          style={{ backgroundColor: `hsl(${colors[module.key as keyof ThemeColors]})` }}
                        />
                      </div>
                      <p className="text-xs text-muted-foreground mb-2">{module.description}</p>
                      <ColorSwatch
                        value={colors[module.key as keyof ThemeColors]}
                        onChange={(v) => handleColorChange(module.key as keyof ThemeColors, v)}
                      />
                    </div>
                  ))}
                </div>
              </div>

              {/* Gradient Configuration */}
              <div className="pt-4 border-t border-border">
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h4 className="text-sm font-medium text-foreground">Brand Gradient</h4>
                    <p className="text-xs text-muted-foreground">Enable gradient effects in the UI</p>
                  </div>
                  <Switch
                    checked={gradientEnabled}
                    onCheckedChange={setGradientEnabled}
                  />
                </div>
                {gradientEnabled && (
                  <div className="animate-scale-in">
                    <GradientPicker
                      fromColor={gradientConfig.from}
                      toColor={gradientConfig.to}
                      viaColor={gradientConfig.via || undefined}
                      angle={gradientConfig.angle}
                      onChange={(config) => setGradientConfig({ ...config, via: config.via || '' })}
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Branding Tab */}
        <TabsContent value="branding" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Building2 className="w-5 h-5" />
                Brand Identity
              </CardTitle>
              <CardDescription>
                Upload your logo and customize your brand name
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* Brand Name */}
              <div>
                <Label htmlFor="brandName">Brand Name</Label>
                <Input
                  id="brandName"
                  value={brandName}
                  onChange={(e) => setBrandName(e.target.value)}
                  placeholder="Your Company Name"
                  className="max-w-md mt-1.5"
                />
                <p className="text-xs text-muted-foreground mt-1">
                  Displayed in the navigation and login screens
                </p>
              </div>

              {/* Logo Upload */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                {/* Full Logo */}
                <div className="space-y-3">
                  <Label>Full Logo</Label>
                  <div className="p-4 bg-secondary/30 rounded-xl">
                    <div className="w-full h-16 rounded-lg bg-background border border-border flex items-center justify-center mb-3">
                      {logoUrl ? (
                        <img src={logoUrl} alt="Logo" className="max-h-12 max-w-full object-contain" />
                      ) : (
                        <div className="flex items-center gap-2 text-muted-foreground">
                          <Image className="w-5 h-5" />
                          <span className="text-sm">No logo</span>
                        </div>
                      )}
                    </div>
                    <input
                      ref={logoInputRef}
                      type="file"
                      accept="image/*"
                      onChange={(e) => handleLogoUpload(e, 'logo')}
                      className="hidden"
                    />
                    <div className="flex gap-2">
                      <Button
                        variant="secondary"
                        size="sm"
                        className="flex-1"
                        onClick={() => logoInputRef.current?.click()}
                      >
                        <Upload className="w-4 h-4 mr-1" />
                        Upload
                      </Button>
                      {logoUrl && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteLogo('logo')}
                        >
                          <Trash2 className="w-4 h-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Used in sidebars. Recommended: 200x50px
                  </p>
                </div>

                {/* Logo Mark */}
                <div className="space-y-3">
                  <Label>Logo Mark / Icon</Label>
                  <div className="p-4 bg-secondary/30 rounded-xl">
                    <div className="w-16 h-16 mx-auto rounded-xl bg-background border border-border flex items-center justify-center mb-3">
                      {logoMarkUrl ? (
                        <img src={logoMarkUrl} alt="Logo Mark" className="max-h-12 max-w-12 object-contain" />
                      ) : (
                        <div
                          className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold text-xl"
                          style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
                        >
                          {brandName?.[0]?.toUpperCase() || 'B'}
                        </div>
                      )}
                    </div>
                    <input
                      ref={logoMarkInputRef}
                      type="file"
                      accept="image/*"
                      onChange={(e) => handleLogoUpload(e, 'logoMark')}
                      className="hidden"
                    />
                    <div className="flex gap-2">
                      <Button
                        variant="secondary"
                        size="sm"
                        className="flex-1"
                        onClick={() => logoMarkInputRef.current?.click()}
                      >
                        <Upload className="w-4 h-4 mr-1" />
                        Upload
                      </Button>
                      {logoMarkUrl && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteLogo('logoMark')}
                        >
                          <Trash2 className="w-4 h-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    For collapsed sidebar. Recommended: 64x64px
                  </p>
                </div>

                {/* Favicon */}
                <div className="space-y-3">
                  <Label>Favicon</Label>
                  <div className="p-4 bg-secondary/30 rounded-xl">
                    <div className="w-16 h-16 mx-auto rounded-xl bg-background border border-border flex items-center justify-center mb-3">
                      {faviconUrl ? (
                        <img src={faviconUrl} alt="Favicon" className="w-8 h-8 object-contain" />
                      ) : (
                        <div
                          className="w-8 h-8 rounded flex items-center justify-center text-white font-bold text-sm"
                          style={{ background: `hsl(${colors.primary})` }}
                        >
                          {brandName?.[0]?.toUpperCase() || 'B'}
                        </div>
                      )}
                    </div>
                    <input
                      ref={faviconInputRef}
                      type="file"
                      accept="image/*,.ico"
                      onChange={(e) => handleLogoUpload(e, 'favicon')}
                      className="hidden"
                    />
                    <div className="flex gap-2">
                      <Button
                        variant="secondary"
                        size="sm"
                        className="flex-1"
                        onClick={() => faviconInputRef.current?.click()}
                      >
                        <Upload className="w-4 h-4 mr-1" />
                        Upload
                      </Button>
                      {faviconUrl && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteLogo('favicon')}
                        >
                          <Trash2 className="w-4 h-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Browser tab icon. Recommended: 32x32px
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Preview Card */}
          <Card>
            <CardHeader>
              <CardTitle>Brand Preview</CardTitle>
              <CardDescription>See how your branding looks in context</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="p-6 bg-card border border-border rounded-xl">
                {/* Header preview */}
                <div className="flex items-center gap-3 pb-4 border-b border-border">
                  {logoMarkUrl ? (
                    <img src={logoMarkUrl} alt="Logo" className="w-10 h-10 object-contain" />
                  ) : (
                    <div
                      className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold"
                      style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
                    >
                      {brandName?.[0]?.toUpperCase() || 'B'}
                    </div>
                  )}
                  <div>
                    <p className="font-semibold text-foreground">{brandName}</p>
                    <p className="text-xs text-muted-foreground">Enterprise Dashboard</p>
                  </div>
                </div>

                {/* Sample UI */}
                <div className="mt-4 space-y-3">
                  <div className="flex gap-2">
                    <button
                      className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                      style={{ backgroundColor: `hsl(${colors.primary})` }}
                    >
                      Primary Action
                    </button>
                    <button
                      className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                      style={{ backgroundColor: `hsl(${colors.accent})` }}
                    >
                      Secondary
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    {['capture', 'processing', 'vendor', 'reporting'].map((mod) => (
                      <span
                        key={mod}
                        className="px-2.5 py-1 rounded-full text-xs font-medium"
                        style={{
                          backgroundColor: `hsl(${colors[mod as keyof ThemeColors]} / 0.15)`,
                          color: `hsl(${colors[mod as keyof ThemeColors]})`,
                        }}
                      >
                        {mod.charAt(0).toUpperCase() + mod.slice(1)}
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Settings Tab */}
        <TabsContent value="settings" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Shield className="w-5 h-5" />
                Theme Permissions
              </CardTitle>
              <CardDescription>
                Control how the organization theme is applied
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              <div className="flex items-center justify-between p-4 bg-secondary/30 rounded-xl">
                <div>
                  <div className="flex items-center gap-2">
                    <Globe className="w-4 h-4 text-muted-foreground" />
                    <span className="font-medium text-foreground">Apply to All Users</span>
                  </div>
                  <p className="text-sm text-muted-foreground mt-1">
                    This theme will be the default for all organization members
                  </p>
                </div>
                <Switch
                  checked={enabledForAllUsers}
                  onCheckedChange={setEnabledForAllUsers}
                />
              </div>

              <div className="flex items-center justify-between p-4 bg-secondary/30 rounded-xl">
                <div>
                  <div className="flex items-center gap-2">
                    <Users className="w-4 h-4 text-muted-foreground" />
                    <span className="font-medium text-foreground">Allow User Customization</span>
                  </div>
                  <p className="text-sm text-muted-foreground mt-1">
                    Users can override with their own theme preferences
                  </p>
                </div>
                <Switch
                  checked={allowUserOverride}
                  onCheckedChange={setAllowUserOverride}
                />
              </div>
            </CardContent>
          </Card>

          {/* Import/Export */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FileJson className="w-5 h-5" />
                Import / Export
              </CardTitle>
              <CardDescription>
                Backup or transfer your theme configuration
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="flex gap-3">
                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".json"
                  onChange={handleImport}
                  className="hidden"
                />
                <Button
                  variant="secondary"
                  onClick={() => fileInputRef.current?.click()}
                >
                  <Upload className="w-4 h-4 mr-2" />
                  Import Theme
                </Button>
                <Button
                  variant="secondary"
                  onClick={handleExport}
                >
                  <Download className="w-4 h-4 mr-2" />
                  Export Theme
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Footer Actions */}
      <div className="flex items-center justify-between pt-4 border-t border-border">
        <Button variant="ghost" onClick={handleReset}>
          <RotateCcw className="w-4 h-4 mr-2" />
          Reset to Defaults
        </Button>
        <div className="flex gap-3">
          {onCancel && (
            <Button variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
          )}
          <Button onClick={handleSave} loading={isSaving}>
            <Save className="w-4 h-4 mr-2" />
            Save Theme
          </Button>
        </div>
      </div>
    </div>
  );
}
