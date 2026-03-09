'use client';

import { useState } from 'react';
import { ThemeShowcase } from '@/components/ui/theme-showcase';
import { ThemeSettings } from '@/components/ui/theme-settings';
import { OrganizationThemeSettings } from '@/components/ui/organization-theme-settings';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets, generateGradient } from '@/stores/theme';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  Palette,
  Building2,
  Layers,
  Sun,
  Moon,
  Monitor,
  Check,
  Info,
  Wand2,
} from 'lucide-react';
import { toast } from 'sonner';

export default function ThemePage() {
  const { hasRole, tenant } = useAuthStore();
  const {
    mode,
    setMode,
    presetId,
    setPreset,
    customColors,
    clearCustomColors,
    isOrgThemeActive,
    organizationTheme,
    clearOrganizationTheme,
    getCurrentColors,
  } = useThemeStore();

  const isAdmin = hasRole('tenant_admin');
  const currentColors = getCurrentColors();
  const [selectedTab, setSelectedTab] = useState('personal');
  const [showQuickPicker, setShowQuickPicker] = useState(false);

  const handleQuickPresetSelect = (preset: typeof themePresets[0]) => {
    if (isOrgThemeActive && !organizationTheme?.allowUserOverride) {
      toast.error('Organization theme cannot be overridden');
      return;
    }
    clearCustomColors();
    setPreset(preset.id);
    toast.success(`Applied ${preset.name} theme`);
  };

  const handleDisableOrgTheme = () => {
    clearOrganizationTheme();
    toast.success('Switched to personal theme');
  };

  return (
    <div className="max-w-6xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
            <Palette className="w-6 h-6 text-primary" />
            Theme Settings
          </h1>
          <p className="text-muted-foreground mt-1">
            Customize the appearance of BillForge to match your brand and preferences
          </p>
        </div>

        {/* Quick Theme Mode Toggle */}
        <div className="flex items-center gap-2 p-1 bg-secondary/50 rounded-lg">
          {[
            { id: 'light', icon: Sun, label: 'Light' },
            { id: 'dark', icon: Moon, label: 'Dark' },
            { id: 'system', icon: Monitor, label: 'System' },
          ].map((option) => (
            <button
              key={option.id}
              onClick={() => setMode(option.id as 'light' | 'dark' | 'system')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-all ${
                mode === option.id
                  ? 'bg-background shadow-sm text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              <option.icon className="w-4 h-4" />
              <span className="hidden sm:inline">{option.label}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Organization Theme Notice */}
      {isOrgThemeActive && organizationTheme && (
        <Card className="border-primary/30 bg-primary/5">
          <CardContent className="py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-primary/10">
                  <Building2 className="w-5 h-5 text-primary" />
                </div>
                <div>
                  <p className="font-medium text-foreground">Organization Theme Active</p>
                  <p className="text-sm text-muted-foreground">
                    {organizationTheme.branding?.brandName || tenant?.settings?.company_name || 'Your organization'} theme is being applied
                    {organizationTheme.allowUserOverride && ' (you can override with personal preferences)'}
                  </p>
                </div>
              </div>
              {organizationTheme.allowUserOverride && (
                <Button variant="secondary" size="sm" onClick={handleDisableOrgTheme}>
                  Use Personal Theme
                </Button>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Quick Theme Picker */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Wand2 className="w-5 h-5" />
                Quick Theme Selection
              </CardTitle>
              <CardDescription>Select a preset theme instantly</CardDescription>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowQuickPicker(!showQuickPicker)}
            >
              {showQuickPicker ? 'Show Less' : 'Show All'}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className={`grid gap-2 ${showQuickPicker ? 'grid-cols-4 sm:grid-cols-6 lg:grid-cols-8' : 'grid-cols-6 sm:grid-cols-8 lg:grid-cols-10'}`}>
            {(showQuickPicker ? themePresets : themePresets.slice(0, 10)).map((preset) => {
              const isSelected = presetId === preset.id && !customColors;
              const gradient = generateGradient(preset);

              return (
                <button
                  key={preset.id}
                  onClick={() => handleQuickPresetSelect(preset)}
                  disabled={isOrgThemeActive && !organizationTheme?.allowUserOverride}
                  className={`relative aspect-square rounded-xl transition-all ${
                    isSelected
                      ? 'ring-2 ring-primary ring-offset-2 ring-offset-background'
                      : 'hover:scale-105 hover:shadow-md'
                  } ${isOrgThemeActive && !organizationTheme?.allowUserOverride ? 'opacity-50 cursor-not-allowed' : ''}`}
                  style={{ background: gradient }}
                  title={preset.name}
                >
                  {isSelected && (
                    <div className="absolute inset-0 flex items-center justify-center">
                      <Check className="w-5 h-5 text-white drop-shadow-md" />
                    </div>
                  )}
                </button>
              );
            })}
          </div>
          <p className="mt-3 text-xs text-muted-foreground">
            Current theme: <span className="font-medium text-foreground">{themePresets.find(p => p.id === presetId)?.name || 'Custom'}</span>
            {customColors && ' (customized)'}
          </p>
        </CardContent>
      </Card>

      {/* Main Tabs */}
      <Tabs value={selectedTab} onValueChange={setSelectedTab} className="space-y-6">
        <TabsList className="bg-secondary/50 p-1">
          <TabsTrigger value="personal" className="flex items-center gap-2">
            <Palette className="w-4 h-4" />
            Personal Theme
          </TabsTrigger>
          {isAdmin && (
            <TabsTrigger value="organization" className="flex items-center gap-2">
              <Building2 className="w-4 h-4" />
              Organization Theme
            </TabsTrigger>
          )}
          <TabsTrigger value="showcase" className="flex items-center gap-2">
            <Layers className="w-4 h-4" />
            Component Showcase
          </TabsTrigger>
        </TabsList>

        {/* Personal Theme Tab */}
        <TabsContent value="personal" className="space-y-0">
          <ThemeSettings mode="user" showAdvanced />
        </TabsContent>

        {/* Organization Theme Tab (Admin Only) */}
        {isAdmin && (
          <TabsContent value="organization" className="space-y-0">
            <OrganizationThemeSettings
              organizationId={tenant?.id}
              organizationName={tenant?.settings?.company_name || 'Your Organization'}
            />
          </TabsContent>
        )}

        {/* Theme Showcase Tab */}
        <TabsContent value="showcase" className="space-y-6">
          {/* Color Palette */}
          <Card>
            <CardHeader>
              <CardTitle>Current Color Palette</CardTitle>
              <CardDescription>Your active theme colors with copy functionality</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4">
                {[
                  { name: 'Primary', key: 'primary' },
                  { name: 'Accent', key: 'accent' },
                  { name: 'Capture', key: 'capture' },
                  { name: 'Processing', key: 'processing' },
                  { name: 'Vendor', key: 'vendor' },
                  { name: 'Reporting', key: 'reporting' },
                ].map((color) => {
                  const value = currentColors[color.key as keyof typeof currentColors];
                  return (
                    <button
                      key={color.key}
                      onClick={() => {
                        navigator.clipboard.writeText(`hsl(${value})`);
                        toast.success(`Copied ${color.name} color`);
                      }}
                      className="group relative overflow-hidden rounded-xl border border-border hover:border-primary/30 transition-all"
                    >
                      <div
                        className="h-16"
                        style={{ backgroundColor: `hsl(${value})` }}
                      />
                      <div className="p-3 bg-card">
                        <p className="font-medium text-foreground text-sm">{color.name}</p>
                        <p className="text-xs text-muted-foreground font-mono truncate">
                          {value}
                        </p>
                      </div>
                      <div className="absolute inset-0 flex items-center justify-center bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity">
                        <span className="text-white text-xs font-medium">Click to copy</span>
                      </div>
                    </button>
                  );
                })}
              </div>
            </CardContent>
          </Card>

          {/* Gradient Previews */}
          <Card>
            <CardHeader>
              <CardTitle>Gradient Previews</CardTitle>
              <CardDescription>See how gradients look with your current colors</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                {[
                  { name: 'Primary Gradient', from: currentColors.primary, to: currentColors.accent },
                  { name: 'Capture Gradient', from: currentColors.capture, to: `${currentColors.capture.split(' ')[0]} 100% 55%` },
                  { name: 'Processing Gradient', from: currentColors.processing, to: `${currentColors.processing.split(' ')[0]} 84% 55%` },
                  { name: 'Rainbow', from: currentColors.primary, via: currentColors.capture, to: currentColors.vendor },
                ].map((gradient, i) => (
                  <div key={i} className="space-y-2">
                    <div
                      className="h-16 rounded-xl shadow-inner"
                      style={{
                        background: gradient.via
                          ? `linear-gradient(135deg, hsl(${gradient.from}), hsl(${gradient.via}), hsl(${gradient.to}))`
                          : `linear-gradient(135deg, hsl(${gradient.from}), hsl(${gradient.to}))`,
                      }}
                    />
                    <p className="text-sm font-medium text-foreground">{gradient.name}</p>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {/* Full Showcase */}
          <ThemeShowcase
            showPresetSelector={false}
            showComponentDemo
            showColorPalette={false}
          />
        </TabsContent>
      </Tabs>

      {/* Theme Info Footer */}
      <Card className="bg-secondary/30">
        <CardContent className="py-4">
          <div className="flex items-start gap-3">
            <Info className="w-5 h-5 text-muted-foreground mt-0.5" />
            <div className="text-sm text-muted-foreground">
              <p>
                Theme preferences are saved locally and synced with your account.
                {isAdmin && ' Organization themes will be applied to all users unless they have override permissions.'}
              </p>
              <p className="mt-2">
                Use <kbd className="kbd">Cmd/Ctrl</kbd> + <kbd className="kbd">Shift</kbd> + <kbd className="kbd">T</kbd> to quickly toggle between light and dark modes.
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
