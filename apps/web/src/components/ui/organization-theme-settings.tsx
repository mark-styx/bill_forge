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
  GradientConfig,
} from '@/stores/theme';
import { useOrganizationTheme } from '@/components/organization-theme-provider';
import { organizationThemeApi } from '@/lib/api';
import { ColorPicker, ColorSwatch } from './color-picker';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from './card';
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
  Building2,
  Palette,
  FileJson,
  Sparkles,
  Globe,
  Users,
  Shield,
  Paintbrush,
  Trash2,
  AlertTriangle,
  Star,
  Zap,
  Layers,
  RefreshCw,
} from 'lucide-react';

interface OrganizationThemeSettingsProps {
  organizationId?: string;
  organizationName?: string;
  onSave?: (theme: OrganizationThemeConfig) => void;
  onCancel?: () => void;
}

export function OrganizationThemeSettings({
  organizationId: _organizationId,
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

  const {
    isPreviewMode,
    startPreview,
    stopPreview,
    applyPreviewColors,
  } = useOrganizationTheme();

  const fileInputRef = useRef<HTMLInputElement>(null);
  const logoInputRef = useRef<HTMLInputElement>(null);
  const logoMarkInputRef = useRef<HTMLInputElement>(null);
  const faviconInputRef = useRef<HTMLInputElement>(null);

  // Form state
  const [activeTab, setActiveTab] = useState('presets');
  const [isSaving, setIsSaving] = useState(false);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

  // Branding
  const [brandName, setBrandName] = useState(organizationTheme?.branding?.brandName || organizationName);
  const [logoUrl, setLogoUrl] = useState(organizationTheme?.branding?.logoUrl || '');
  const [logoMarkUrl, setLogoMarkUrl] = useState(organizationTheme?.branding?.logoMark || '');
  const [faviconUrl, setFaviconUrl] = useState(organizationTheme?.branding?.faviconUrl || '');

  // Theme
  const [selectedPresetId, setSelectedPresetId] = useState(organizationTheme?.preset_id || presetId);
  const [useCustomColors, setUseCustomColors] = useState(!!organizationTheme?.custom_colors);
  const [colors, setColors] = useState<ThemeColors>(
    organizationTheme?.custom_colors || getCurrentColors()
  );

  // Gradient
  const [gradientEnabled, setGradientEnabled] = useState<boolean>(
    organizationTheme?.gradient_config?.enabled ?? true
  );
  const [gradientType, setGradientType] = useState<'linear' | 'radial' | 'conic'>(
    (organizationTheme?.gradient_config?.type as 'linear' | 'radial' | 'conic') || 'linear'
  );
  const [gradientAngle, setGradientAngle] = useState(
    organizationTheme?.gradient_config?.angle || 135
  );

  // Permissions
  const [enabledForAllUsers, setEnabledForAllUsers] = useState(
    organizationTheme?.enabled_for_all_users ?? true
  );
  const [allowUserOverride, setAllowUserOverride] = useState(
    organizationTheme?.allow_user_override ?? true
  );

  // Track changes
  useEffect(() => {
    setHasUnsavedChanges(true);
  }, [brandName, selectedPresetId, colors, gradientEnabled, gradientType, gradientAngle, enabledForAllUsers, allowUserOverride, logoUrl, logoMarkUrl, faviconUrl]);

  // Handle color change with live preview
  const handleColorChange = useCallback((key: keyof ThemeColors, value: string) => {
    const newColors = { ...colors, [key]: value };
    setColors(newColors);
    setUseCustomColors(true);

    if (isPreviewMode) {
      applyPreviewColors(newColors);
    }
  }, [colors, isPreviewMode, applyPreviewColors]);

  // Handle preset selection
  const handlePresetSelect = useCallback((preset: ThemePreset) => {
    setSelectedPresetId(preset.id);
    setColors(preset.colors);
    setUseCustomColors(false);

    if (isPreviewMode) {
      applyPreviewColors(preset.colors);
    }
  }, [isPreviewMode, applyPreviewColors]);

  // Toggle preview mode
  const togglePreview = useCallback(() => {
    if (isPreviewMode) {
      stopPreview();
    } else {
      startPreview(colors);
    }
  }, [isPreviewMode, colors, startPreview, stopPreview]);

  // Save theme
  const handleSave = async () => {
    setIsSaving(true);
    try {
      const gradientConfig: GradientConfig = {
        enabled: gradientEnabled,
        type: gradientType,
        angle: gradientAngle,
      };

      const themeConfig: OrganizationThemeConfig = {
        preset_id: selectedPresetId,
        custom_colors: useCustomColors ? colors : undefined,
        branding: {
          brandName,
          logoUrl: logoUrl || undefined,
          logoMark: logoMarkUrl || undefined,
          faviconUrl: faviconUrl || undefined,
        },
        enabled_for_all_users: enabledForAllUsers,
        allow_user_override: allowUserOverride,
        gradient_config: gradientConfig,
      };

      // Save to API
      await organizationThemeApi.saveTheme({
        preset_id: selectedPresetId,
        custom_colors: useCustomColors ? colors : undefined,
        branding: themeConfig.branding,
        enabled_for_all_users: enabledForAllUsers,
        allow_user_override: allowUserOverride,
        gradient_config: gradientConfig,
      });

      // Update local state
      setOrganizationTheme(themeConfig);
      setHasUnsavedChanges(false);

      // Stop preview mode after save
      if (isPreviewMode) {
        stopPreview();
      }

      toast.success('Organization theme saved');
      onSave?.(themeConfig);
    } catch (error) {
      console.error('Failed to save theme:', error);
      toast.error('Failed to save theme');
    } finally {
      setIsSaving(false);
    }
  };

  // Export theme
  const handleExport = () => {
    const config = exportThemeConfig({
      preset_id: selectedPresetId,
      custom_colors: useCustomColors ? colors : null,
      organizationTheme: {
        preset_id: selectedPresetId,
        custom_colors: useCustomColors ? colors : undefined,
        branding: { brandName, logoUrl, logoMark: logoMarkUrl, faviconUrl },
        enabled_for_all_users: enabledForAllUsers,
        allow_user_override: allowUserOverride,
        gradient_config: { enabled: gradientEnabled, type: gradientType, angle: gradientAngle },
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

    toast.success('Theme exported');
  };

  // Import theme
  const handleImport = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const imported = importThemeConfig(content);

      if (imported) {
        setSelectedPresetId(imported.preset_id);
        if (imported.custom_colors) {
          setColors(imported.custom_colors);
          setUseCustomColors(true);
        } else {
          const preset = themePresets.find((p) => p.id === imported.preset_id);
          if (preset) {
            setColors(preset.colors);
          }
          setUseCustomColors(false);
        }
        if (imported.organizationTheme?.branding) {
          setBrandName(imported.organizationTheme.branding.brandName || brandName);
        }
        toast.success('Theme imported');
      } else {
        toast.error('Invalid theme file');
      }
    };
    reader.readAsText(file);

    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // Handle logo upload
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
      toast.success('Logo uploaded');
    } catch (error) {
      toast.error('Upload failed');
    }
  };

  // Reset to defaults
  const handleReset = () => {
    const defaultPreset = themePresets.find(p => p.recommended) || themePresets[0];
    setSelectedPresetId(defaultPreset.id);
    setColors(defaultPreset.colors);
    setUseCustomColors(false);
    setGradientEnabled(true);
    setGradientType('linear');
    setGradientAngle(135);

    if (isPreviewMode) {
      applyPreviewColors(defaultPreset.colors);
    }

    toast.info('Reset to default');
  };

  // Delete logo
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
      toast.success('Logo removed');
    } catch (error) {
      toast.error('Failed to remove logo');
    }
  };

  const categories = ['bright', 'modern', 'vibrant', 'professional'] as const;
  const categoryLabels: Record<string, { label: string; icon: typeof Star }> = {
    bright: { label: 'Bright & Clean', icon: Zap },
    modern: { label: 'Modern', icon: Layers },
    vibrant: { label: 'Vibrant', icon: Sparkles },
    professional: { label: 'Professional', icon: Building2 },
  };

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
            Customize appearance for all {organizationName} users
          </p>
        </div>
        <div className="flex items-center gap-2">
          {hasUnsavedChanges && (
            <span className="text-xs text-warning flex items-center gap-1 px-2 py-1 bg-warning/10 rounded-lg">
              <AlertTriangle className="w-3 h-3" />
              Unsaved
            </span>
          )}
          <Button
            variant={isPreviewMode ? 'default' : 'secondary'}
            size="sm"
            onClick={togglePreview}
            className={isPreviewMode ? 'btn-bright' : ''}
          >
            {isPreviewMode ? <EyeOff className="w-4 h-4 mr-1.5" /> : <Eye className="w-4 h-4 mr-1.5" />}
            {isPreviewMode ? 'Stop Preview' : 'Live Preview'}
          </Button>
        </div>
      </div>

      {/* Preview indicator */}
      {isPreviewMode && (
        <div className="live-indicator">
          Live preview active
        </div>
      )}

      {/* Main Content */}
      <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
        <TabsList className="bg-secondary/50 p-1 rounded-xl">
          <TabsTrigger value="presets" className="rounded-lg data-[state=active]:bg-background data-[state=active]:shadow-sm">
            <Palette className="w-4 h-4 mr-2" />
            Presets
          </TabsTrigger>
          <TabsTrigger value="colors" className="rounded-lg data-[state=active]:bg-background data-[state=active]:shadow-sm">
            <Paintbrush className="w-4 h-4 mr-2" />
            Colors
          </TabsTrigger>
          <TabsTrigger value="branding" className="rounded-lg data-[state=active]:bg-background data-[state=active]:shadow-sm">
            <Building2 className="w-4 h-4 mr-2" />
            Branding
          </TabsTrigger>
          <TabsTrigger value="settings" className="rounded-lg data-[state=active]:bg-background data-[state=active]:shadow-sm">
            <Shield className="w-4 h-4 mr-2" />
            Settings
          </TabsTrigger>
        </TabsList>

        {/* Presets Tab */}
        <TabsContent value="presets" className="space-y-6 animate-fade-in">
          {categories.map((category) => {
            const categoryPresets = getThemeByCategory(category);
            if (categoryPresets.length === 0) return null;

            const CategoryIcon = categoryLabels[category].icon;

            return (
              <Card key={category} className="overflow-hidden">
                <CardHeader className="pb-3 border-b border-border/50">
                  <CardTitle className="text-sm font-medium flex items-center gap-2">
                    <CategoryIcon className="w-4 h-4 text-muted-foreground" />
                    {categoryLabels[category].label}
                  </CardTitle>
                </CardHeader>
                <CardContent className="pt-4">
                  <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
                    {categoryPresets.map((preset) => {
                      const isSelected = selectedPresetId === preset.id && !useCustomColors;
                      const gradient = generateGradient(preset);

                      return (
                        <button
                          key={preset.id}
                          onClick={() => handlePresetSelect(preset)}
                          className={`
                            relative group rounded-xl overflow-hidden transition-all duration-200
                            ${isSelected
                              ? 'ring-2 ring-primary ring-offset-2 ring-offset-background shadow-bright'
                              : 'hover:shadow-lg hover:scale-[1.02]'
                            }
                          `}
                        >
                          <div
                            className="h-14 w-full"
                            style={{ background: gradient }}
                          />
                          <div className="p-2.5 bg-card border-t border-border/50">
                            <div className="flex items-center justify-between">
                              <span className="text-xs font-medium text-foreground truncate">
                                {preset.name}
                              </span>
                              {preset.recommended && (
                                <Star className="w-3 h-3 text-warning fill-warning" />
                              )}
                            </div>
                          </div>
                          {isSelected && (
                            <div className="absolute top-2 right-2 w-5 h-5 rounded-full bg-white shadow-lg flex items-center justify-center">
                              <Check className="w-3 h-3 text-primary" />
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

        {/* Colors Tab */}
        <TabsContent value="colors" className="space-y-6 animate-fade-in">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Sparkles className="w-5 h-5 text-primary" />
                Custom Colors
              </CardTitle>
              <CardDescription>
                Fine-tune colors to match your brand
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* Primary Colors */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div>
                  <Label className="mb-2 block text-sm font-medium">Primary</Label>
                  <ColorPicker
                    value={colors.primary}
                    onChange={(value) => handleColorChange('primary', value)}
                    showGradientPreview
                    gradientWith={colors.accent}
                  />
                </div>
                <div>
                  <Label className="mb-2 block text-sm font-medium">Accent</Label>
                  <ColorPicker
                    value={colors.accent}
                    onChange={(value) => handleColorChange('accent', value)}
                  />
                </div>
              </div>

              {/* Module Colors */}
              <div className="pt-4 border-t border-border">
                <h4 className="text-sm font-medium text-foreground mb-4">Module Colors</h4>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                  {[
                    { key: 'capture', label: 'Capture', icon: '📥' },
                    { key: 'processing', label: 'Processing', icon: '⚡' },
                    { key: 'vendor', label: 'Vendors', icon: '👥' },
                    { key: 'reporting', label: 'Reports', icon: '📊' },
                  ].map((module) => (
                    <div key={module.key} className="p-3 bg-secondary/30 rounded-xl">
                      <div className="flex items-center gap-2 mb-2">
                        <span>{module.icon}</span>
                        <span className="text-sm font-medium">{module.label}</span>
                      </div>
                      <ColorSwatch
                        value={colors[module.key as keyof ThemeColors]}
                        onChange={(v) => handleColorChange(module.key as keyof ThemeColors, v)}
                      />
                    </div>
                  ))}
                </div>
              </div>

              {/* Gradient Config */}
              <div className="pt-4 border-t border-border">
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h4 className="text-sm font-medium">Brand Gradient</h4>
                    <p className="text-xs text-muted-foreground">Enable gradient effects</p>
                  </div>
                  <Switch
                    checked={gradientEnabled}
                    onCheckedChange={setGradientEnabled}
                  />
                </div>
                {gradientEnabled && (
                  <div className="space-y-4 animate-scale-in">
                    <div className="flex gap-2">
                      {(['linear', 'radial', 'conic'] as const).map((type) => (
                        <button
                          key={type}
                          onClick={() => setGradientType(type)}
                          className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                            gradientType === type
                              ? 'bg-primary text-primary-foreground'
                              : 'bg-secondary text-muted-foreground hover:text-foreground'
                          }`}
                        >
                          {type.charAt(0).toUpperCase() + type.slice(1)}
                        </button>
                      ))}
                    </div>
                    {(gradientType === 'linear' || gradientType === 'conic') && (
                      <div>
                        <Label className="text-xs">Angle: {gradientAngle}°</Label>
                        <input
                          type="range"
                          min="0"
                          max="360"
                          value={gradientAngle}
                          onChange={(e) => setGradientAngle(parseInt(e.target.value))}
                          className="w-full h-2 bg-secondary rounded-lg appearance-none cursor-pointer"
                        />
                      </div>
                    )}
                    <div
                      className="h-12 rounded-xl shadow-inner"
                      style={{
                        background: gradientType === 'linear'
                          ? `linear-gradient(${gradientAngle}deg, hsl(${colors.primary}), hsl(${colors.accent}))`
                          : gradientType === 'radial'
                          ? `radial-gradient(circle, hsl(${colors.primary}), hsl(${colors.accent}))`
                          : `conic-gradient(from ${gradientAngle}deg, hsl(${colors.primary}), hsl(${colors.accent}), hsl(${colors.primary}))`
                      }}
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Branding Tab */}
        <TabsContent value="branding" className="space-y-6 animate-fade-in">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Building2 className="w-5 h-5 text-primary" />
                Brand Identity
              </CardTitle>
              <CardDescription>
                Logo and brand name settings
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* Brand Name */}
              <div>
                <Label htmlFor="brandName" className="text-sm font-medium">Brand Name</Label>
                <Input
                  id="brandName"
                  value={brandName}
                  onChange={(e) => setBrandName(e.target.value)}
                  placeholder="Company Name"
                  className="max-w-md mt-1.5"
                />
              </div>

              {/* Logos */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                {[
                  { type: 'logo', label: 'Full Logo', ref: logoInputRef, url: logoUrl, size: 'h-16', desc: '200x50px' },
                  { type: 'logoMark', label: 'Logo Mark', ref: logoMarkInputRef, url: logoMarkUrl, size: 'w-16 h-16', desc: '64x64px' },
                  { type: 'favicon', label: 'Favicon', ref: faviconInputRef, url: faviconUrl, size: 'w-8 h-8', desc: '32x32px' },
                ].map((item) => (
                  <div key={item.type} className="p-4 bg-secondary/30 rounded-xl">
                    <Label className="text-sm font-medium mb-3 block">{item.label}</Label>
                    <div className="flex items-center justify-center p-4 bg-background rounded-lg border border-border min-h-[80px] mb-3">
                      {item.url ? (
                        /* eslint-disable-next-line @next/next/no-img-element */
                        <img src={item.url} alt={item.label} className={`${item.size} object-contain`} />
                      ) : (
                        <div
                          className={`${item.type === 'logo' ? 'px-4 py-2' : 'w-10 h-10'} rounded-lg flex items-center justify-center text-white font-bold`}
                          style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
                        >
                          {item.type === 'logo' ? brandName : brandName?.[0]?.toUpperCase() || 'B'}
                        </div>
                      )}
                    </div>
                    <input
                      ref={item.ref}
                      type="file"
                      accept="image/*"
                      onChange={(e) => handleLogoUpload(e, item.type as 'logo' | 'logoMark' | 'favicon')}
                      className="hidden"
                    />
                    <div className="flex gap-2">
                      <Button
                        variant="secondary"
                        size="sm"
                        className="flex-1"
                        onClick={() => item.ref.current?.click()}
                      >
                        <Upload className="w-3.5 h-3.5 mr-1" />
                        Upload
                      </Button>
                      {item.url && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteLogo(item.type as 'logo' | 'logoMark' | 'favicon')}
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </Button>
                      )}
                    </div>
                    <p className="text-xs text-muted-foreground mt-2">{item.desc}</p>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {/* Live Preview */}
          <Card className="org-theme-preview">
            <div className="org-theme-preview-header" style={{ background: `linear-gradient(90deg, hsl(${colors.primary}), hsl(${colors.accent}))` }} />
            <CardContent className="p-6">
              <div className="flex items-center gap-3 mb-4">
                {logoMarkUrl ? (
                  /* eslint-disable-next-line @next/next/no-img-element */
                  <img src={logoMarkUrl} alt="Logo" className="w-10 h-10 object-contain" />
                ) : (
                  <div className="org-brand-mark">{brandName?.[0]?.toUpperCase() || 'B'}</div>
                )}
                <div>
                  <p className="font-semibold text-foreground">{brandName}</p>
                  <p className="text-xs text-muted-foreground">Theme Preview</p>
                </div>
              </div>
              <div className="flex gap-2">
                <button className="btn-bright px-4 py-2 text-sm">Primary</button>
                <button
                  className="px-4 py-2 rounded-xl text-sm font-medium"
                  style={{ backgroundColor: `hsl(${colors.accent})`, color: 'white' }}
                >
                  Accent
                </button>
              </div>
              <div className="flex flex-wrap gap-2 mt-4">
                {(['capture', 'processing', 'vendor', 'reporting'] as const).map((mod) => (
                  <span
                    key={mod}
                    className="bright-chip"
                    style={{
                      background: `linear-gradient(135deg, hsl(${colors[mod]} / 0.15), hsl(${colors[mod]} / 0.1))`,
                      color: `hsl(${colors[mod]})`,
                    }}
                  >
                    {mod.charAt(0).toUpperCase() + mod.slice(1)}
                  </span>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Settings Tab */}
        <TabsContent value="settings" className="space-y-6 animate-fade-in">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Shield className="w-5 h-5 text-primary" />
                Permissions
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between p-4 bg-secondary/30 rounded-xl">
                <div className="flex items-center gap-3">
                  <Globe className="w-5 h-5 text-muted-foreground" />
                  <div>
                    <span className="font-medium text-foreground text-sm">Apply to All Users</span>
                    <p className="text-xs text-muted-foreground">Default theme for all members</p>
                  </div>
                </div>
                <Switch checked={enabledForAllUsers} onCheckedChange={setEnabledForAllUsers} />
              </div>

              <div className="flex items-center justify-between p-4 bg-secondary/30 rounded-xl">
                <div className="flex items-center gap-3">
                  <Users className="w-5 h-5 text-muted-foreground" />
                  <div>
                    <span className="font-medium text-foreground text-sm">Allow User Override</span>
                    <p className="text-xs text-muted-foreground">Users can set personal themes</p>
                  </div>
                </div>
                <Switch checked={allowUserOverride} onCheckedChange={setAllowUserOverride} />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FileJson className="w-5 h-5 text-primary" />
                Import / Export
              </CardTitle>
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
                <Button variant="secondary" onClick={() => fileInputRef.current?.click()}>
                  <Upload className="w-4 h-4 mr-2" />
                  Import
                </Button>
                <Button variant="secondary" onClick={handleExport}>
                  <Download className="w-4 h-4 mr-2" />
                  Export
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Footer */}
      <div className="flex items-center justify-between pt-4 border-t border-border">
        <Button variant="ghost" onClick={handleReset}>
          <RotateCcw className="w-4 h-4 mr-2" />
          Reset
        </Button>
        <div className="flex gap-3">
          {onCancel && (
            <Button variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
          )}
          <Button onClick={handleSave} disabled={isSaving} className="btn-bright">
            {isSaving ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Save className="w-4 h-4 mr-2" />
            )}
            Save Theme
          </Button>
        </div>
      </div>
    </div>
  );
}
