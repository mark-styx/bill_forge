'use client';

import { useState } from 'react';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets, ThemeColors } from '@/stores/theme';
import { toast } from 'sonner';
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
  Sliders,
} from 'lucide-react';

const tabs = [
  { id: 'appearance', name: 'Appearance', icon: Palette },
  { id: 'organization', name: 'Organization', icon: Building2 },
  { id: 'profile', name: 'Profile', icon: User },
  { id: 'notifications', name: 'Notifications', icon: Bell },
  { id: 'security', name: 'Security', icon: Shield },
];

export default function SettingsPage() {
  const { user, tenant } = useAuthStore();
  const { mode, setMode, presetId, setPreset, customColors, setCustomColors, clearCustomColors } = useThemeStore();
  const [activeTab, setActiveTab] = useState('appearance');
  const [showCustomPicker, setShowCustomPicker] = useState(false);
  const [tempColor, setTempColor] = useState('210');

  const handleSaveCustomColor = () => {
    const newColors: ThemeColors = {
      primary: `${tempColor} 100% 50%`,
      accent: `${parseInt(tempColor) + 20} 90% 50%`,
      capture: '195 100% 45%',
      processing: '160 84% 39%',
      vendor: '270 70% 55%',
      reporting: '35 95% 55%',
    };
    setCustomColors(newColors);
    setShowCustomPicker(false);
    toast.success('Custom theme applied');
  };

  return (
    <div className="max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-semibold text-foreground">Settings</h1>
        <p className="text-muted-foreground mt-1">Manage your account and preferences</p>
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
          {activeTab === 'appearance' && (
            <>
              {/* Theme Mode */}
              <div className="card p-6">
                <h2 className="text-lg font-semibold text-foreground mb-1">Theme Mode</h2>
                <p className="text-sm text-muted-foreground mb-4">Choose how BillForge looks to you</p>
                
                <div className="grid grid-cols-3 gap-3">
                  {[
                    { id: 'light', name: 'Light', icon: Sun },
                    { id: 'dark', name: 'Dark', icon: Moon },
                    { id: 'system', name: 'System', icon: Monitor },
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
                    </button>
                  ))}
                </div>
              </div>

              {/* Color Theme */}
              <div className="card p-6">
                <h2 className="text-lg font-semibold text-foreground mb-1">Color Theme</h2>
                <p className="text-sm text-muted-foreground mb-4">Select a color scheme for your organization</p>

                <div className="grid grid-cols-2 sm:grid-cols-3 gap-3 mb-6">
                  {themePresets.map((preset) => (
                    <button
                      key={preset.id}
                      onClick={() => {
                        clearCustomColors();
                        setPreset(preset.id);
                        toast.success(`${preset.name} theme applied`);
                      }}
                      className={`p-4 rounded-xl border-2 text-left transition-all ${
                        presetId === preset.id && !customColors
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:border-primary/30'
                      }`}
                    >
                      <div className={`w-full h-12 rounded-lg bg-gradient-to-r ${preset.preview} mb-3`} />
                      <div className="flex items-center justify-between">
                        <div>
                          <p className="font-medium text-foreground text-sm">{preset.name}</p>
                          <p className="text-xs text-muted-foreground">{preset.description.slice(0, 30)}...</p>
                        </div>
                        {presetId === preset.id && !customColors && (
                          <Check className="w-5 h-5 text-primary" />
                        )}
                      </div>
                    </button>
                  ))}
                </div>

                {/* Custom Color */}
                <div className="pt-4 border-t border-border">
                  <div className="flex items-center justify-between mb-4">
                    <div>
                      <p className="font-medium text-foreground">Custom Brand Color</p>
                      <p className="text-sm text-muted-foreground">Use your organization's brand color</p>
                    </div>
                    <button
                      onClick={() => setShowCustomPicker(!showCustomPicker)}
                      className="btn btn-secondary btn-sm"
                    >
                      {showCustomPicker ? 'Cancel' : 'Customize'}
                    </button>
                  </div>

                  {showCustomPicker && (
                    <div className="p-4 bg-secondary rounded-xl space-y-4 animate-scale-in">
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-2">
                          Primary Color Hue ({tempColor}°)
                        </label>
                        <input
                          type="range"
                          min="0"
                          max="360"
                          value={tempColor}
                          onChange={(e) => setTempColor(e.target.value)}
                          className="w-full h-3 rounded-full appearance-none cursor-pointer"
                          style={{
                            background: `linear-gradient(to right, 
                              hsl(0, 100%, 50%), 
                              hsl(60, 100%, 50%), 
                              hsl(120, 100%, 50%), 
                              hsl(180, 100%, 50%), 
                              hsl(240, 100%, 50%), 
                              hsl(300, 100%, 50%), 
                              hsl(360, 100%, 50%)
                            )`,
                          }}
                        />
                      </div>
                      <div className="flex items-center gap-4">
                        <div
                          className="w-16 h-16 rounded-xl shadow-soft"
                          style={{ backgroundColor: `hsl(${tempColor}, 100%, 50%)` }}
                        />
                        <div className="flex-1">
                          <p className="text-sm text-foreground font-medium">Preview</p>
                          <p className="text-xs text-muted-foreground">This will be your primary brand color</p>
                        </div>
                        <button
                          onClick={handleSaveCustomColor}
                          className="btn btn-primary btn-sm"
                        >
                          Apply Color
                        </button>
                      </div>
                    </div>
                  )}

                  {customColors && (
                    <div className="mt-4 flex items-center justify-between p-3 bg-primary/5 rounded-lg border border-primary/20">
                      <div className="flex items-center gap-3">
                        <div
                          className="w-8 h-8 rounded-lg"
                          style={{ backgroundColor: `hsl(${customColors.primary})` }}
                        />
                        <span className="text-sm font-medium text-foreground">Custom color active</span>
                      </div>
                      <button
                        onClick={() => {
                          clearCustomColors();
                          toast.success('Reset to default theme');
                        }}
                        className="text-sm text-primary hover:underline"
                      >
                        Reset
                      </button>
                    </div>
                  )}
                </div>
              </div>
            </>
          )}

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
                    <button className="btn btn-secondary btn-sm">Upload Logo</button>
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
                <button className="btn btn-primary btn-md">Save Changes</button>
              </div>
            </div>
          )}

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
                    <button className="btn btn-secondary btn-sm">Change Avatar</button>
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
                    className="input"
                    disabled
                  />
                </div>
              </div>

              <div className="mt-6 pt-6 border-t border-border flex justify-end">
                <button className="btn btn-primary btn-md">Save Profile</button>
              </div>
            </div>
          )}

          {activeTab === 'notifications' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Notification Preferences</h2>
              <p className="text-sm text-muted-foreground mb-6">Configure how you receive notifications</p>

              <div className="space-y-4">
                {[
                  { label: 'Invoice received', description: 'When a new invoice is uploaded', default: true },
                  { label: 'Approval required', description: 'When an invoice needs your approval', default: true },
                  { label: 'Invoice approved', description: 'When your submitted invoice is approved', default: true },
                  { label: 'Invoice rejected', description: 'When an invoice is rejected', default: true },
                  { label: 'Weekly digest', description: 'Summary of weekly activity', default: false },
                ].map((item) => (
                  <div key={item.label} className="flex items-center justify-between py-3 border-b border-border last:border-0">
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

          {activeTab === 'security' && (
            <div className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-1">Security Settings</h2>
              <p className="text-sm text-muted-foreground mb-6">Manage your account security</p>

              <div className="space-y-6">
                <div>
                  <h3 className="font-medium text-foreground mb-2">Change Password</h3>
                  <div className="space-y-3">
                    <input type="password" placeholder="Current password" className="input" />
                    <input type="password" placeholder="New password" className="input" />
                    <input type="password" placeholder="Confirm new password" className="input" />
                  </div>
                  <button className="btn btn-secondary btn-sm mt-3">Update Password</button>
                </div>

                <div className="pt-6 border-t border-border">
                  <h3 className="font-medium text-foreground mb-2">Two-Factor Authentication</h3>
                  <p className="text-sm text-muted-foreground mb-3">
                    Add an extra layer of security to your account
                  </p>
                  <button className="btn btn-secondary btn-sm">Enable 2FA</button>
                </div>

                <div className="pt-6 border-t border-border">
                  <h3 className="font-medium text-foreground mb-2">Active Sessions</h3>
                  <p className="text-sm text-muted-foreground mb-3">
                    Manage devices where you're logged in
                  </p>
                  <div className="p-3 bg-secondary rounded-lg flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <Monitor className="w-5 h-5 text-muted-foreground" />
                      <div>
                        <p className="text-sm font-medium text-foreground">Current session</p>
                        <p className="text-xs text-muted-foreground">Last active: now</p>
                      </div>
                    </div>
                    <span className="text-xs bg-success/15 text-success px-2 py-1 rounded-full">Active</span>
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
