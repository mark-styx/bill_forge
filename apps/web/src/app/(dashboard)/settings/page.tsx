'use client';

import { useState, useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets, ThemeColors, generateGradient } from '@/stores/theme';
import { api, invoiceStatusApi, InvoiceStatusConfig, InvoiceStatusConfigInput } from '@/lib/api';
import { toast } from 'sonner';
import { ColorPicker, ColorSwatch, GradientPicker } from '@/components/ui/color-picker';
import {
  Palette,
  Sun,
  Moon,
  Monitor,
  Check,
  Building2,
  User,
  Bell,
  Shield,
  Sparkles,
  Eye,
  RotateCcw,
  Upload,
  Save,
  Download,
  Copy,
  Tags,
  Plus,
  Trash2,
  GripVertical,
  ArrowUp,
  ArrowDown,
} from 'lucide-react';

const tabs = [
  { id: 'appearance', name: 'Appearance', icon: Palette },
  { id: 'branding', name: 'Branding', icon: Sparkles },
  { id: 'statuses', name: 'Invoice Statuses', icon: Tags },
  { id: 'organization', name: 'Organization', icon: Building2 },
  { id: 'profile', name: 'Profile', icon: User },
  { id: 'notifications', name: 'Notifications', icon: Bell },
  { id: 'security', name: 'Security', icon: Shield },
];

const categoryLabels: Record<string, string> = {
  bright: 'Bright & Clean',
  vibrant: 'Vibrant & Bold',
  professional: 'Professional',
  modern: 'Modern & Dynamic',
};

export default function SettingsPage() {
  const { user, tenant } = useAuthStore();
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
    setOrganizationTheme,
    clearOrganizationTheme,
    updateOrganizationTheme,
  } = useThemeStore();

  const [activeTab, setActiveTab] = useState('appearance');
  const { refreshTenantContext } = useAuthStore();

  // Organization settings form state
  const [orgCompanyName, setOrgCompanyName] = useState(tenant?.settings?.company_name || '');
  const [orgTimezone, setOrgTimezone] = useState(tenant?.settings?.timezone || 'UTC');
  const [orgCurrency, setOrgCurrency] = useState(tenant?.settings?.default_currency || 'USD');

  const saveSettingsMutation = useMutation({
    mutationFn: (data: { company_name?: string; timezone?: string; default_currency?: string }) =>
      api.put('/api/v1/settings', data),
    onSuccess: () => {
      refreshTenantContext();
      toast.success('Organization settings saved');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to save settings');
    },
  });
  const [orgBrandName, setOrgBrandName] = useState(organizationTheme?.branding?.brandName || tenant?.settings?.company_name || '');
  const [previewMode, setPreviewMode] = useState(false);

  // Organization branding state
  const [brandColors, setBrandColors] = useState<ThemeColors>({
    primary: organizationTheme?.customColors?.primary || '210 100% 50%',
    accent: organizationTheme?.customColors?.accent || '190 95% 45%',
    capture: organizationTheme?.customColors?.capture || '195 100% 45%',
    processing: organizationTheme?.customColors?.processing || '160 84% 39%',
    vendor: organizationTheme?.customColors?.vendor || '270 70% 55%',
    reporting: organizationTheme?.customColors?.reporting || '35 95% 55%',
  });

  const handleSaveOrganizationTheme = () => {
    setOrganizationTheme({
      presetId: presetId,
      customColors: brandColors,
      branding: { brandName: orgBrandName },
    });
    toast.success('Organization theme saved');
  };

  const handlePreviewTheme = (colors: ThemeColors) => {
    if (typeof window === 'undefined') return;
    const root = document.documentElement;
    root.style.setProperty('--primary', colors.primary);
    root.style.setProperty('--accent', colors.accent);
    root.style.setProperty('--capture', colors.capture);
    root.style.setProperty('--processing', colors.processing);
    root.style.setProperty('--vendor', colors.vendor);
    root.style.setProperty('--reporting', colors.reporting);
    root.style.setProperty('--ring', colors.primary);
  };

  const handlePresetSelect = (preset: typeof themePresets[0]) => {
    clearCustomColors();
    setPreset(preset.id);
    setBrandColors(preset.colors);
    toast.success(`${preset.name} theme applied`);
  };

  // Group presets by category
  const groupedPresets = themePresets.reduce((acc, preset) => {
    const category = preset.category || 'bright';
    if (!acc[category]) acc[category] = [];
    acc[category].push(preset);
    return acc;
  }, {} as Record<string, typeof themePresets>);

  return (
    <div className="max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-semibold text-foreground">Settings</h1>
        <p className="text-muted-foreground mt-1">Manage your account and organization preferences</p>
      </div>

      <div className="flex flex-col lg:flex-row gap-6">
        {/* Sidebar */}
        <nav className="lg:w-56 flex-shrink-0">
          <div className="card p-2 lg:sticky lg:top-20">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                  activeTab === tab.id
                    ? 'bg-primary/10 text-primary'
                    : 'text-muted-foreground hover:text-foreground hover:bg-secondary'
                }`}
              >
                <tab.icon className="w-4 h-4" />
                {tab.name}
              </button>
            ))}
          </div>
        </nav>

        {/* Content */}
        <div className="flex-1 space-y-6">
          {/* Appearance Tab */}
          {activeTab === 'appearance' && (
            <>
              {/* Theme Mode */}
              <div className="card p-6">
                <h2 className="text-lg font-semibold text-foreground mb-1">Theme Mode</h2>
                <p className="text-sm text-muted-foreground mb-4">Choose how BillForge looks to you</p>

                <div className="grid grid-cols-3 gap-3">
                  {[
                    { id: 'light', name: 'Light', icon: Sun, desc: 'Bright and clean' },
                    { id: 'dark', name: 'Dark', icon: Moon, desc: 'Easy on the eyes' },
                    { id: 'system', name: 'System', icon: Monitor, desc: 'Match device' },
                  ].map((option) => (
                    <button
                      key={option.id}
                      onClick={() => setMode(option.id as any)}
                      className={`p-4 rounded-xl border-2 transition-all ${
                        mode === option.id
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:border-primary/30'
                      }`}
                    >
                      <option.icon className={`w-6 h-6 mx-auto mb-2 ${mode === option.id ? 'text-primary' : 'text-muted-foreground'}`} />
                      <p className={`text-sm font-medium ${mode === option.id ? 'text-primary' : 'text-foreground'}`}>
                        {option.name}
                      </p>
                      <p className="text-xs text-muted-foreground mt-0.5">{option.desc}</p>
                    </button>
                  ))}
                </div>
              </div>

              {/* Color Theme */}
              <div className="card p-6">
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">Color Theme</h2>
                    <p className="text-sm text-muted-foreground">Select a color scheme for your workspace</p>
                  </div>
                  {isOrgThemeActive && (
                    <span className="text-xs bg-primary/10 text-primary px-2.5 py-1 rounded-full font-medium">
                      Organization theme active
                    </span>
                  )}
                </div>

                {/* Theme Categories */}
                {['modern', 'bright', 'vibrant', 'professional'].map((category) => {
                  const presets = groupedPresets[category];
                  if (!presets || presets.length === 0) return null;

                  return (
                    <div key={category} className="mb-6 last:mb-0">
                      <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                        {categoryLabels[category]}
                      </p>
                      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
                        {presets.map((preset) => {
                          const gradient = generateGradient(preset);
                          return (
                            <button
                              key={preset.id}
                              onClick={() => handlePresetSelect(preset)}
                              disabled={isOrgThemeActive}
                              className={`p-3 rounded-xl border-2 text-left transition-all ${
                                presetId === preset.id && !customColors && !isOrgThemeActive
                                  ? 'border-primary bg-primary/5'
                                  : 'border-border hover:border-primary/30'
                              } ${isOrgThemeActive ? 'opacity-60 cursor-not-allowed' : ''}`}
                            >
                              <div
                                className="w-full h-10 rounded-lg mb-2"
                                style={{ background: gradient }}
                              />
                              <div className="flex items-center justify-between">
                                <div>
                                  <p className="font-medium text-foreground text-sm">{preset.name}</p>
                                  <p className="text-xs text-muted-foreground truncate max-w-[100px]">
                                    {preset.description}
                                  </p>
                                </div>
                                {presetId === preset.id && !customColors && !isOrgThemeActive && (
                                  <Check className="w-4 h-4 text-primary flex-shrink-0" />
                                )}
                              </div>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}

                {isOrgThemeActive && (
                  <div className="mt-4 p-4 bg-primary/5 rounded-xl border border-primary/20">
                    <p className="text-sm text-foreground font-medium">Organization theme is active</p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Personal theme preferences are disabled. Contact your admin to customize the organization theme.
                    </p>
                    <button
                      onClick={() => {
                        clearOrganizationTheme();
                        toast.success('Switched to personal theme');
                      }}
                      className="mt-3 text-sm text-primary hover:underline"
                    >
                      Use personal theme instead
                    </button>
                  </div>
                )}
              </div>
            </>
          )}

          {/* Branding Tab - Organization Theme Customization */}
          {activeTab === 'branding' && (
            <>
              <div className="card p-6">
                <div className="flex items-center justify-between mb-6">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">Organization Branding</h2>
                    <p className="text-sm text-muted-foreground">Customize the look and feel for your entire organization</p>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setPreviewMode(!previewMode)}
                      className={`btn btn-sm ${previewMode ? 'btn-primary' : 'btn-secondary'}`}
                    >
                      <Eye className="w-4 h-4 mr-1.5" />
                      {previewMode ? 'Previewing' : 'Preview'}
                    </button>
                  </div>
                </div>

                {/* Brand Name */}
                <div className="mb-6">
                  <label className="block text-sm font-medium text-foreground mb-1.5">Brand Name</label>
                  <input
                    type="text"
                    value={orgBrandName}
                    onChange={(e) => setOrgBrandName(e.target.value)}
                    className="input max-w-md"
                    placeholder="Your Company Name"
                  />
                  <p className="text-xs text-muted-foreground mt-1">This will appear in the sidebar and other areas</p>
                </div>

                {/* Logo Upload */}
                <div className="mb-6 pb-6 border-b border-border">
                  <label className="block text-sm font-medium text-foreground mb-3">Organization Logo</label>
                  <div className="flex items-center gap-4">
                    <div className="w-20 h-20 rounded-xl bg-gradient-to-br from-primary to-accent flex items-center justify-center">
                      <Building2 className="w-10 h-10 text-white" />
                    </div>
                    <div>
                      <button className="btn btn-secondary btn-sm">
                        <Upload className="w-4 h-4 mr-1.5" />
                        Upload Logo
                      </button>
                      <p className="text-xs text-muted-foreground mt-1.5">PNG, SVG or JPG. Max 2MB. Recommended: 256x256px</p>
                    </div>
                  </div>
                </div>

                {/* Primary Brand Color */}
                <div className="mb-6">
                  <h3 className="text-sm font-semibold text-foreground mb-3">Primary Brand Color</h3>
                  <ColorPicker
                    value={brandColors.primary}
                    onChange={(value) => {
                      const newColors = { ...brandColors, primary: value };
                      setBrandColors(newColors);
                      if (previewMode) handlePreviewTheme(newColors);
                    }}
                  />
                </div>

                {/* Module Colors */}
                <div className="space-y-4">
                  <h3 className="text-sm font-semibold text-foreground">Module Colors</h3>
                  <p className="text-xs text-muted-foreground -mt-2">
                    Customize colors for different modules (optional)
                  </p>

                  <div className="grid grid-cols-2 gap-4">
                    {[
                      { key: 'capture', label: 'Invoice Capture', icon: '📥' },
                      { key: 'processing', label: 'Processing', icon: '⚡' },
                      { key: 'vendor', label: 'Vendors', icon: '🏢' },
                      { key: 'reporting', label: 'Reporting', icon: '📊' },
                    ].map((module) => (
                      <div key={module.key} className="p-4 bg-secondary/50 rounded-xl">
                        <div className="flex items-center gap-2 mb-3">
                          <span>{module.icon}</span>
                          <span className="text-sm font-medium text-foreground">{module.label}</span>
                        </div>
                        <ColorSwatch
                          value={brandColors[module.key as keyof ThemeColors]}
                          onChange={(value) => {
                            const newColors = { ...brandColors, [module.key]: value };
                            setBrandColors(newColors);
                            if (previewMode) handlePreviewTheme(newColors);
                          }}
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Theme Preview */}
                <div className="mt-6 pt-6 border-t border-border">
                  <h3 className="text-sm font-semibold text-foreground mb-4">Live Preview</h3>
                  <div className="p-6 bg-background border border-border rounded-xl space-y-4">
                    {/* Preview Header */}
                    <div className="flex items-center gap-3">
                      <div
                        className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold"
                        style={{ backgroundColor: `hsl(${brandColors.primary})` }}
                      >
                        {orgBrandName?.[0]?.toUpperCase() || 'B'}
                      </div>
                      <div>
                        <p className="font-semibold text-foreground">{orgBrandName || 'Your Company'}</p>
                        <p className="text-xs text-muted-foreground">Organization Dashboard</p>
                      </div>
                    </div>

                    {/* Preview Buttons */}
                    <div className="flex flex-wrap gap-2">
                      <button
                        className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                        style={{ backgroundColor: `hsl(${brandColors.primary})` }}
                      >
                        Primary Button
                      </button>
                      <button
                        className="px-4 py-2 rounded-lg text-white text-sm font-medium"
                        style={{ backgroundColor: `hsl(${brandColors.accent})` }}
                      >
                        Accent Button
                      </button>
                    </div>

                    {/* Preview Badges */}
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
                            backgroundColor: `hsl(${brandColors[badge.key as keyof ThemeColors]} / 0.15)`,
                            color: `hsl(${brandColors[badge.key as keyof ThemeColors]})`,
                          }}
                        >
                          {badge.label}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>

                {/* Save Actions */}
                <div className="mt-6 pt-6 border-t border-border flex items-center justify-between">
                  <button
                    onClick={() => {
                      const defaultPreset = themePresets[0];
                      setBrandColors(defaultPreset.colors);
                      setOrgBrandName(tenant?.settings?.company_name || '');
                      if (previewMode) handlePreviewTheme(defaultPreset.colors);
                      toast.info('Reset to defaults');
                    }}
                    className="btn btn-ghost btn-sm"
                  >
                    <RotateCcw className="w-4 h-4 mr-1.5" />
                    Reset to Defaults
                  </button>
                  <button onClick={handleSaveOrganizationTheme} className="btn btn-primary btn-md">
                    <Save className="w-4 h-4 mr-1.5" />
                    Save Organization Theme
                  </button>
                </div>
              </div>
            </>
          )}

          {/* Invoice Statuses Tab */}
          {activeTab === 'statuses' && (
            <InvoiceStatusesTab />
          )}

          {/* Organization Tab */}
          {activeTab === 'organization' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Organization Settings</h2>
              <p className="text-sm text-muted-foreground mb-6">Manage your company information</p>

              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Company Name</label>
                  <input
                    type="text"
                    value={orgCompanyName}
                    onChange={(e) => setOrgCompanyName(e.target.value)}
                    className="input"
                    placeholder="Acme Corporation"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Company Logo</label>
                  <div className="flex items-center gap-4">
                    <div className="w-16 h-16 rounded-xl bg-secondary flex items-center justify-center">
                      <Building2 className="w-8 h-8 text-muted-foreground" />
                    </div>
                    <button className="btn btn-secondary btn-sm">
                      <Upload className="w-4 h-4 mr-1.5" />
                      Upload Logo
                    </button>
                  </div>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Timezone</label>
                  <select className="input" value={orgTimezone} onChange={(e) => setOrgTimezone(e.target.value)}>
                    <option value="UTC">UTC</option>
                    <option value="America/New_York">Eastern Time</option>
                    <option value="America/Chicago">Central Time</option>
                    <option value="America/Denver">Mountain Time</option>
                    <option value="America/Los_Angeles">Pacific Time</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Default Currency</label>
                  <select className="input" value={orgCurrency} onChange={(e) => setOrgCurrency(e.target.value)}>
                    <option value="USD">USD - US Dollar</option>
                    <option value="EUR">EUR - Euro</option>
                    <option value="GBP">GBP - British Pound</option>
                    <option value="CAD">CAD - Canadian Dollar</option>
                  </select>
                </div>
              </div>

              <div className="mt-6 pt-6 border-t border-border flex justify-end">
                <button
                  onClick={() => saveSettingsMutation.mutate({
                    company_name: orgCompanyName,
                    timezone: orgTimezone,
                    default_currency: orgCurrency,
                  })}
                  disabled={saveSettingsMutation.isPending}
                  className="btn btn-primary btn-md"
                >
                  <Save className="w-4 h-4 mr-1.5" />
                  {saveSettingsMutation.isPending ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </div>
          )}

          {/* Profile Tab */}
          {activeTab === 'profile' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Profile Settings</h2>
              <p className="text-sm text-muted-foreground mb-6">Update your personal information</p>

              <div className="space-y-4">
                <div className="flex items-center gap-4 mb-6">
                  <div className="w-20 h-20 rounded-full bg-gradient-to-br from-primary to-accent flex items-center justify-center text-2xl font-bold text-white">
                    {user?.name?.[0]?.toUpperCase() || 'U'}
                  </div>
                  <div>
                    <button className="btn btn-secondary btn-sm">
                      <Upload className="w-4 h-4 mr-1.5" />
                      Change Avatar
                    </button>
                    <p className="text-xs text-muted-foreground mt-1">JPG, PNG or GIF. Max 2MB.</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-sm font-medium text-foreground mb-1.5">First Name</label>
                    <input
                      type="text"
                      defaultValue={user?.name?.split(' ')[0] || ''}
                      className="input"
                    />
                  </div>
                  <div>
                    <label className="block text-sm font-medium text-foreground mb-1.5">Last Name</label>
                    <input
                      type="text"
                      defaultValue={user?.name?.split(' ').slice(1).join(' ') || ''}
                      className="input"
                    />
                  </div>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Email</label>
                  <input
                    type="email"
                    defaultValue={user?.email || ''}
                    className="input bg-secondary/50"
                    disabled
                  />
                </div>
              </div>

              <div className="mt-6 pt-6 border-t border-border flex justify-end">
                <button className="btn btn-primary btn-md">
                  <Save className="w-4 h-4 mr-1.5" />
                  Save Profile
                </button>
              </div>
            </div>
          )}

          {/* Notifications Tab */}
          {activeTab === 'notifications' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Notification Preferences</h2>
              <p className="text-sm text-muted-foreground mb-6">Configure how you receive notifications</p>

              <div className="space-y-1">
                {[
                  { label: 'Invoice received', description: 'When a new invoice is uploaded', default: true },
                  { label: 'Approval required', description: 'When an invoice needs your approval', default: true },
                  { label: 'Invoice approved', description: 'When your submitted invoice is approved', default: true },
                  { label: 'Invoice rejected', description: 'When an invoice is rejected', default: true },
                  { label: 'Weekly digest', description: 'Summary of weekly activity', default: false },
                ].map((item) => (
                  <div key={item.label} className="flex items-center justify-between py-4 border-b border-border last:border-0">
                    <div>
                      <p className="font-medium text-foreground">{item.label}</p>
                      <p className="text-sm text-muted-foreground">{item.description}</p>
                    </div>
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input type="checkbox" defaultChecked={item.default} className="sr-only peer" />
                      <div className="w-11 h-6 bg-secondary rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary" />
                    </label>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Security Tab */}
          {activeTab === 'security' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Security Settings</h2>
              <p className="text-sm text-muted-foreground mb-6">Manage your account security</p>

              <div className="space-y-6">
                <div>
                  <h3 className="font-medium text-foreground mb-3">Change Password</h3>
                  <div className="space-y-3 max-w-md">
                    <input type="password" placeholder="Current password" className="input" />
                    <input type="password" placeholder="New password" className="input" />
                    <input type="password" placeholder="Confirm new password" className="input" />
                  </div>
                  <button className="btn btn-secondary btn-sm mt-4">Update Password</button>
                </div>

                <div className="pt-6 border-t border-border">
                  <h3 className="font-medium text-foreground mb-2">Two-Factor Authentication</h3>
                  <p className="text-sm text-muted-foreground mb-3">
                    Add an extra layer of security to your account
                  </p>
                  <button className="btn btn-secondary btn-sm">Enable 2FA</button>
                </div>

                <div className="pt-6 border-t border-border">
                  <h3 className="font-medium text-foreground mb-3">Active Sessions</h3>
                  <div className="p-4 bg-secondary/50 rounded-xl flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <div className="w-10 h-10 rounded-lg bg-background flex items-center justify-center">
                        <Monitor className="w-5 h-5 text-muted-foreground" />
                      </div>
                      <div>
                        <p className="text-sm font-medium text-foreground">Current session</p>
                        <p className="text-xs text-muted-foreground">Last active: now</p>
                      </div>
                    </div>
                    <span className="text-xs bg-success/15 text-success px-2.5 py-1 rounded-full font-medium">
                      Active
                    </span>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Color presets for status configuration
const colorPresets = [
  { name: 'Green', color: 'green', bg: 'bg-success/10', text: 'text-success' },
  { name: 'Blue', color: 'blue', bg: 'bg-primary/10', text: 'text-primary' },
  { name: 'Yellow', color: 'yellow', bg: 'bg-warning/10', text: 'text-warning' },
  { name: 'Red', color: 'red', bg: 'bg-error/10', text: 'text-error' },
  { name: 'Gray', color: 'gray', bg: 'bg-secondary', text: 'text-muted-foreground' },
  { name: 'Purple', color: 'purple', bg: 'bg-violet-500/10', text: 'text-violet-600' },
  { name: 'Orange', color: 'orange', bg: 'bg-orange-500/10', text: 'text-orange-600' },
  { name: 'Teal', color: 'teal', bg: 'bg-teal-500/10', text: 'text-teal-600' },
];

interface StatusFormRow {
  status_key: string;
  display_label: string;
  color: string;
  bg_color: string;
  text_color: string;
  sort_order: number;
  is_terminal: boolean;
  is_active: boolean;
  category: string;
  isNew?: boolean;
}

function InvoiceStatusesTab() {
  const queryClient = useQueryClient();
  const [activeCategory, setActiveCategory] = useState<'processing' | 'capture'>('processing');
  const [statuses, setStatuses] = useState<StatusFormRow[]>([]);
  const [hasChanges, setHasChanges] = useState(false);

  const { data: statusConfigs, isLoading } = useQuery({
    queryKey: ['invoice-status-config'],
    queryFn: () => invoiceStatusApi.list(),
  });

  // Sync fetched data into local state
  useEffect(() => {
    if (statusConfigs && !hasChanges) {
      setStatuses(statusConfigs.map(s => ({
        status_key: s.status_key,
        display_label: s.display_label,
        color: s.color,
        bg_color: s.bg_color,
        text_color: s.text_color,
        sort_order: s.sort_order,
        is_terminal: s.is_terminal,
        is_active: s.is_active,
        category: s.category,
      })));
    }
  }, [statusConfigs, hasChanges]);

  const saveMutation = useMutation({
    mutationFn: (data: InvoiceStatusConfigInput[]) => invoiceStatusApi.update(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice-status-config'] });
      setHasChanges(false);
      toast.success('Invoice statuses saved');
    },
    onError: () => toast.error('Failed to save statuses'),
  });

  const seedMutation = useMutation({
    mutationFn: () => invoiceStatusApi.seedDefaults(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice-status-config'] });
      setHasChanges(false);
      toast.success('Default statuses restored');
    },
    onError: () => toast.error('Failed to seed defaults'),
  });

  const filteredStatuses = statuses
    .filter(s => s.category === activeCategory)
    .sort((a, b) => a.sort_order - b.sort_order);

  const updateStatus = (index: number, field: keyof StatusFormRow, value: unknown) => {
    const globalIndex = statuses.findIndex(
      s => s.status_key === filteredStatuses[index].status_key && s.category === activeCategory
    );
    if (globalIndex === -1) return;
    const updated = [...statuses];
    (updated[globalIndex] as unknown as Record<string, unknown>)[field] = value;
    setStatuses(updated);
    setHasChanges(true);
  };

  const applyColorPreset = (index: number, preset: typeof colorPresets[0]) => {
    const globalIndex = statuses.findIndex(
      s => s.status_key === filteredStatuses[index].status_key && s.category === activeCategory
    );
    if (globalIndex === -1) return;
    const updated = [...statuses];
    updated[globalIndex] = {
      ...updated[globalIndex],
      color: preset.color,
      bg_color: preset.bg,
      text_color: preset.text,
    };
    setStatuses(updated);
    setHasChanges(true);
  };

  const moveStatus = (index: number, direction: 'up' | 'down') => {
    const swapIndex = direction === 'up' ? index - 1 : index + 1;
    if (swapIndex < 0 || swapIndex >= filteredStatuses.length) return;

    const globalA = statuses.findIndex(
      s => s.status_key === filteredStatuses[index].status_key && s.category === activeCategory
    );
    const globalB = statuses.findIndex(
      s => s.status_key === filteredStatuses[swapIndex].status_key && s.category === activeCategory
    );
    if (globalA === -1 || globalB === -1) return;

    const updated = [...statuses];
    const tmpOrder = updated[globalA].sort_order;
    updated[globalA] = { ...updated[globalA], sort_order: updated[globalB].sort_order };
    updated[globalB] = { ...updated[globalB], sort_order: tmpOrder };
    setStatuses(updated);
    setHasChanges(true);
  };

  const addStatus = () => {
    const maxOrder = filteredStatuses.reduce((max, s) => Math.max(max, s.sort_order), -1);
    const newKey = `custom_${Date.now()}`;
    setStatuses(prev => [...prev, {
      status_key: newKey,
      display_label: 'New Status',
      color: 'blue',
      bg_color: 'bg-primary/10',
      text_color: 'text-primary',
      sort_order: maxOrder + 1,
      is_terminal: false,
      is_active: true,
      category: activeCategory,
      isNew: true,
    }]);
    setHasChanges(true);
  };

  const removeStatus = (index: number) => {
    const target = filteredStatuses[index];
    setStatuses(prev => prev.filter(s => !(s.status_key === target.status_key && s.category === target.category)));
    setHasChanges(true);
  };

  const handleSave = () => {
    saveMutation.mutate(statuses.map(s => ({
      status_key: s.status_key,
      display_label: s.display_label,
      color: s.color,
      bg_color: s.bg_color,
      text_color: s.text_color,
      sort_order: s.sort_order,
      is_terminal: s.is_terminal,
      is_active: s.is_active,
      category: s.category,
    })));
  };

  if (isLoading) {
    return (
      <div className="card p-6">
        <div className="animate-pulse space-y-4">
          <div className="h-6 bg-secondary rounded w-48" />
          <div className="h-4 bg-secondary rounded w-72" />
          <div className="space-y-3 mt-6">
            {[1, 2, 3, 4, 5].map(i => (
              <div key={i} className="h-14 bg-secondary rounded" />
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="card p-6">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h2 className="text-lg font-semibold text-foreground">Invoice Statuses</h2>
            <p className="text-sm text-muted-foreground">
              Customize status labels, colors, and ordering for your organization
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => seedMutation.mutate()}
              disabled={seedMutation.isPending}
              className="btn btn-ghost btn-sm"
            >
              <RotateCcw className="w-4 h-4 mr-1.5" />
              Reset Defaults
            </button>
          </div>
        </div>

        {/* Category Tabs */}
        <div className="flex gap-1 p-1 bg-secondary/50 rounded-lg mb-6 w-fit">
          {(['processing', 'capture'] as const).map(cat => (
            <button
              key={cat}
              onClick={() => setActiveCategory(cat)}
              className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                activeCategory === cat
                  ? 'bg-background text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {cat === 'processing' ? 'Processing Statuses' : 'Capture Statuses'}
            </button>
          ))}
        </div>

        {/* Status List */}
        <div className="space-y-2">
          {/* Header */}
          <div className="grid grid-cols-[40px_1fr_180px_120px_80px_80px_60px] gap-3 px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
            <div>Order</div>
            <div>Label</div>
            <div>Color</div>
            <div>Status Key</div>
            <div>Terminal</div>
            <div>Active</div>
            <div></div>
          </div>

          {filteredStatuses.map((status, index) => (
            <div
              key={status.status_key}
              className="grid grid-cols-[40px_1fr_180px_120px_80px_80px_60px] gap-3 items-center px-3 py-3 bg-secondary/30 rounded-lg hover:bg-secondary/50 transition-colors"
            >
              {/* Reorder */}
              <div className="flex flex-col gap-0.5">
                <button
                  onClick={() => moveStatus(index, 'up')}
                  disabled={index === 0}
                  className="p-0.5 hover:bg-secondary rounded disabled:opacity-30"
                >
                  <ArrowUp className="w-3 h-3" />
                </button>
                <button
                  onClick={() => moveStatus(index, 'down')}
                  disabled={index === filteredStatuses.length - 1}
                  className="p-0.5 hover:bg-secondary rounded disabled:opacity-30"
                >
                  <ArrowDown className="w-3 h-3" />
                </button>
              </div>

              {/* Label */}
              <div className="flex items-center gap-3">
                <span
                  className={`px-2.5 py-1 rounded-full text-xs font-medium ${status.bg_color} ${status.text_color}`}
                >
                  {status.display_label}
                </span>
                <input
                  type="text"
                  value={status.display_label}
                  onChange={(e) => updateStatus(index, 'display_label', e.target.value)}
                  className="input input-sm flex-1 max-w-[200px]"
                />
              </div>

              {/* Color Preset */}
              <div className="flex items-center gap-1.5 flex-wrap">
                {colorPresets.map(preset => (
                  <button
                    key={preset.color}
                    onClick={() => applyColorPreset(index, preset)}
                    className={`w-5 h-5 rounded-full border-2 transition-all ${
                      status.color === preset.color
                        ? 'border-foreground scale-110'
                        : 'border-transparent hover:border-muted-foreground/50'
                    }`}
                    style={{
                      backgroundColor: preset.color === 'gray' ? '#9ca3af'
                        : preset.color === 'green' ? '#22c55e'
                        : preset.color === 'blue' ? '#3b82f6'
                        : preset.color === 'yellow' ? '#eab308'
                        : preset.color === 'red' ? '#ef4444'
                        : preset.color === 'purple' ? '#8b5cf6'
                        : preset.color === 'orange' ? '#f97316'
                        : preset.color === 'teal' ? '#14b8a6'
                        : '#6b7280'
                    }}
                  />
                ))}
              </div>

              {/* Status Key */}
              <div>
                {status.isNew ? (
                  <input
                    type="text"
                    value={status.status_key}
                    onChange={(e) => updateStatus(index, 'status_key', e.target.value.toLowerCase().replace(/\s+/g, '_'))}
                    className="input input-sm w-full font-mono text-xs"
                    placeholder="status_key"
                  />
                ) : (
                  <span className="text-xs font-mono text-muted-foreground">{status.status_key}</span>
                )}
              </div>

              {/* Terminal Toggle */}
              <div className="flex justify-center">
                <label className="relative inline-flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={status.is_terminal}
                    onChange={(e) => updateStatus(index, 'is_terminal', e.target.checked)}
                    className="sr-only peer"
                  />
                  <div className="w-9 h-5 bg-secondary rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary" />
                </label>
              </div>

              {/* Active Toggle */}
              <div className="flex justify-center">
                <label className="relative inline-flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={status.is_active}
                    onChange={(e) => updateStatus(index, 'is_active', e.target.checked)}
                    className="sr-only peer"
                  />
                  <div className="w-9 h-5 bg-secondary rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-success" />
                </label>
              </div>

              {/* Delete */}
              <div className="flex justify-center">
                <button
                  onClick={() => removeStatus(index)}
                  className="p-1.5 text-muted-foreground hover:text-error rounded transition-colors"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}

          {filteredStatuses.length === 0 && (
            <div className="text-center py-8 text-muted-foreground">
              <Tags className="w-8 h-8 mx-auto mb-2 opacity-50" />
              <p className="text-sm">No statuses configured for this category.</p>
              <p className="text-xs mt-1">Click "Reset Defaults" to seed the default statuses, or add a custom one.</p>
            </div>
          )}
        </div>

        {/* Add Status Button */}
        <button
          onClick={addStatus}
          className="mt-4 flex items-center gap-2 px-4 py-2 border-2 border-dashed border-border rounded-lg text-sm text-muted-foreground hover:text-foreground hover:border-primary/30 transition-colors w-full justify-center"
        >
          <Plus className="w-4 h-4" />
          Add Custom Status
        </button>

        {/* Save Bar */}
        {hasChanges && (
          <div className="mt-6 pt-6 border-t border-border flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              You have unsaved changes
            </p>
            <div className="flex items-center gap-3">
              <button
                onClick={() => {
                  setHasChanges(false);
                  // Re-sync from server data
                  if (statusConfigs) {
                    setStatuses(statusConfigs.map(s => ({
                      status_key: s.status_key,
                      display_label: s.display_label,
                      color: s.color,
                      bg_color: s.bg_color,
                      text_color: s.text_color,
                      sort_order: s.sort_order,
                      is_terminal: s.is_terminal,
                      is_active: s.is_active,
                      category: s.category,
                    })));
                  }
                }}
                className="btn btn-ghost btn-sm"
              >
                Discard
              </button>
              <button
                onClick={handleSave}
                disabled={saveMutation.isPending}
                className="btn btn-primary btn-md"
              >
                <Save className="w-4 h-4 mr-1.5" />
                {saveMutation.isPending ? 'Saving...' : 'Save Changes'}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Info Card */}
      <div className="card p-4 bg-primary/5 border-primary/20">
        <h3 className="text-sm font-semibold text-foreground mb-1">About Status Configuration</h3>
        <ul className="text-xs text-muted-foreground space-y-1">
          <li><strong>Status Key</strong> - Internal identifier used in the system (cannot be changed for existing statuses)</li>
          <li><strong>Terminal</strong> - Terminal statuses represent end states (e.g., Paid, Voided, Rejected)</li>
          <li><strong>Active</strong> - Inactive statuses are hidden from dropdowns and filters</li>
          <li><strong>Custom statuses</strong> - Add organization-specific statuses beyond the defaults</li>
        </ul>
      </div>
    </div>
  );
}
