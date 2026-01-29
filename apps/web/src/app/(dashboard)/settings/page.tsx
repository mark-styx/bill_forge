'use client';

import { useState } from 'react';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets, ThemeColors } from '@/stores/theme';
import { toast } from 'sonner';
import { ColorPicker, ColorSwatch } from '@/components/ui/color-picker';
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
} from 'lucide-react';

const tabs = [
  { id: 'appearance', name: 'Appearance', icon: Palette },
  { id: 'branding', name: 'Branding', icon: Sparkles },
  { id: 'organization', name: 'Organization', icon: Building2 },
  { id: 'profile', name: 'Profile', icon: User },
  { id: 'notifications', name: 'Notifications', icon: Bell },
  { id: 'security', name: 'Security', icon: Shield },
];

const categoryLabels = {
  bright: 'Bright & Clean',
  vibrant: 'Vibrant & Bold',
  professional: 'Professional',
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
  const [orgBrandName, setOrgBrandName] = useState(organizationTheme?.brandName || tenant?.settings?.company_name || '');
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
      brandName: orgBrandName,
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
                {Object.entries(groupedPresets).map(([category, presets]) => (
                  <div key={category} className="mb-6 last:mb-0">
                    <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3">
                      {categoryLabels[category as keyof typeof categoryLabels]}
                    </p>
                    <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                      {presets.map((preset) => (
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
                          <div className={`w-full h-10 rounded-lg bg-gradient-to-r ${preset.preview} mb-2`} />
                          <div className="flex items-center justify-between">
                            <div>
                              <p className="font-medium text-foreground text-sm">{preset.name}</p>
                              <p className="text-xs text-muted-foreground truncate max-w-[120px]">
                                {preset.description}
                              </p>
                            </div>
                            {presetId === preset.id && !customColors && !isOrgThemeActive && (
                              <Check className="w-4 h-4 text-primary flex-shrink-0" />
                            )}
                          </div>
                        </button>
                      ))}
                    </div>
                  </div>
                ))}

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
                    defaultValue={tenant?.settings?.company_name || ''}
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
                  <select className="input" defaultValue={tenant?.settings?.timezone || 'UTC'}>
                    <option value="UTC">UTC</option>
                    <option value="America/New_York">Eastern Time</option>
                    <option value="America/Chicago">Central Time</option>
                    <option value="America/Denver">Mountain Time</option>
                    <option value="America/Los_Angeles">Pacific Time</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-foreground mb-1.5">Default Currency</label>
                  <select className="input" defaultValue={tenant?.settings?.default_currency || 'USD'}>
                    <option value="USD">USD - US Dollar</option>
                    <option value="EUR">EUR - Euro</option>
                    <option value="GBP">GBP - British Pound</option>
                    <option value="CAD">CAD - Canadian Dollar</option>
                  </select>
                </div>
              </div>

              <div className="mt-6 pt-6 border-t border-border flex justify-end">
                <button className="btn btn-primary btn-md">
                  <Save className="w-4 h-4 mr-1.5" />
                  Save Changes
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
