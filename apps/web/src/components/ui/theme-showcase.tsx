'use client';

import { useState } from 'react';
import { useThemeStore, themePresets, ThemePreset, generateGradient } from '@/stores/theme';
import { Button } from './button';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from './card';
import { Badge } from './badge';
import {
  Check,
  Sun,
  Moon,
  Monitor,
  Palette,
  Download,
  Upload,
  Copy,
  FileText,
  Users,
  TrendingUp,
  CheckCircle,
  AlertCircle,
  Clock,
  ArrowRight,
  Plus,
  Search,
  Bell,
  Settings,
  Star,
  Heart,
  Zap,
  Sparkles,
} from 'lucide-react';
import { toast } from 'sonner';

const categoryLabels: Record<string, string> = {
  bright: 'Bright & Clean',
  vibrant: 'Vibrant & Bold',
  professional: 'Professional',
  modern: 'Modern & Dynamic',
};

const categoryDescriptions: Record<string, string> = {
  bright: 'Light, clean themes perfect for productivity apps',
  vibrant: 'Bold, colorful themes that make a statement',
  professional: 'Subdued, elegant themes for enterprise use',
  modern: 'Dynamic themes with gradient effects',
};

interface ThemeShowcaseProps {
  showPresetSelector?: boolean;
  showComponentDemo?: boolean;
  showColorPalette?: boolean;
  compact?: boolean;
}

export function ThemeShowcase({
  showPresetSelector = true,
  showComponentDemo = true,
  showColorPalette = true,
  compact = false,
}: ThemeShowcaseProps) {
  const {
    mode,
    setMode,
    presetId,
    setPreset,
    customColors,
    clearCustomColors,
    getCurrentColors,
    isOrgThemeActive,
  } = useThemeStore();

  const [selectedCategory, setSelectedCategory] = useState<string | 'all'>('all');
  const currentColors = getCurrentColors();

  const filteredPresets = selectedCategory === 'all'
    ? themePresets
    : themePresets.filter((p) => p.category === selectedCategory);

  const categories = Array.from(new Set(themePresets.map((p) => p.category)));

  const handlePresetSelect = (preset: ThemePreset) => {
    if (isOrgThemeActive) {
      toast.error('Organization theme is active. Switch to personal theme first.');
      return;
    }
    clearCustomColors();
    setPreset(preset.id);
    toast.success(`Applied ${preset.name} theme`);
  };

  const copyThemeCSS = () => {
    const css = `
:root {
  --primary: ${currentColors.primary};
  --accent: ${currentColors.accent};
  --capture: ${currentColors.capture};
  --processing: ${currentColors.processing};
  --vendor: ${currentColors.vendor};
  --reporting: ${currentColors.reporting};
}
    `.trim();
    navigator.clipboard.writeText(css);
    toast.success('Theme CSS copied to clipboard');
  };

  return (
    <div className={`space-y-8 ${compact ? 'space-y-4' : ''}`}>
      {/* Theme Mode Toggle */}
      {showPresetSelector && (
        <Card>
          <CardHeader className={compact ? 'p-4' : ''}>
            <CardTitle className="flex items-center gap-2">
              <Palette className="w-5 h-5" />
              Theme Mode
            </CardTitle>
            <CardDescription>Choose your preferred color scheme</CardDescription>
          </CardHeader>
          <CardContent className={compact ? 'p-4 pt-0' : ''}>
            <div className="flex gap-2">
              {[
                { id: 'light', icon: Sun, label: 'Light' },
                { id: 'dark', icon: Moon, label: 'Dark' },
                { id: 'system', icon: Monitor, label: 'System' },
              ].map((option) => (
                <Button
                  key={option.id}
                  variant={mode === option.id ? 'default' : 'secondary'}
                  onClick={() => setMode(option.id as any)}
                  className="flex-1"
                >
                  <option.icon className="w-4 h-4 mr-2" />
                  {option.label}
                </Button>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Theme Presets */}
      {showPresetSelector && (
        <Card>
          <CardHeader className={compact ? 'p-4' : ''}>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Color Themes</CardTitle>
                <CardDescription>Select a preset or customize your colors</CardDescription>
              </div>
              <Button variant="ghost" size="sm" onClick={copyThemeCSS}>
                <Copy className="w-4 h-4 mr-2" />
                Copy CSS
              </Button>
            </div>
          </CardHeader>
          <CardContent className={compact ? 'p-4 pt-0' : ''}>
            {/* Category Filter */}
            <div className="flex flex-wrap gap-2 mb-6">
              <Button
                variant={selectedCategory === 'all' ? 'default' : 'secondary'}
                size="sm"
                onClick={() => setSelectedCategory('all')}
              >
                All
              </Button>
              {categories.map((cat) => (
                <Button
                  key={cat}
                  variant={selectedCategory === cat ? 'default' : 'secondary'}
                  size="sm"
                  onClick={() => setSelectedCategory(cat)}
                >
                  {categoryLabels[cat] || cat}
                </Button>
              ))}
            </div>

            {/* Presets Grid */}
            {categories.map((category) => {
              const categoryPresets = filteredPresets.filter((p) => p.category === category);
              if (categoryPresets.length === 0) return null;

              return (
                <div key={category} className="mb-6 last:mb-0">
                  {selectedCategory === 'all' && (
                    <div className="mb-3">
                      <h3 className="text-sm font-semibold text-foreground">
                        {categoryLabels[category]}
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        {categoryDescriptions[category]}
                      </p>
                    </div>
                  )}
                  <div className={`grid gap-3 ${compact ? 'grid-cols-3 sm:grid-cols-4' : 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-4'}`}>
                    {categoryPresets.map((preset) => {
                      const isSelected = presetId === preset.id && !customColors;
                      const gradient = generateGradient(preset);

                      return (
                        <button
                          key={preset.id}
                          onClick={() => handlePresetSelect(preset)}
                          disabled={isOrgThemeActive}
                          className={`
                            relative p-3 rounded-xl border-2 text-left transition-all
                            ${isSelected ? 'border-primary bg-primary/5 shadow-sm' : 'border-border hover:border-primary/30'}
                            ${isOrgThemeActive ? 'opacity-60 cursor-not-allowed' : 'cursor-pointer'}
                          `}
                        >
                          {/* Gradient Preview */}
                          <div
                            className="w-full h-12 rounded-lg mb-2 shadow-inner"
                            style={{ background: gradient }}
                          />

                          {/* Color Dots */}
                          <div className="flex gap-1 mb-2">
                            {Object.values(preset.colors).slice(0, 4).map((color, i) => (
                              <div
                                key={i}
                                className="w-4 h-4 rounded-full border border-white/20"
                                style={{ backgroundColor: `hsl(${color})` }}
                              />
                            ))}
                          </div>

                          <p className="font-medium text-foreground text-sm">{preset.name}</p>
                          <p className="text-xs text-muted-foreground truncate">
                            {preset.description}
                          </p>

                          {isSelected && (
                            <div className="absolute top-2 right-2 w-5 h-5 rounded-full bg-primary flex items-center justify-center">
                              <Check className="w-3 h-3 text-white" />
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
      )}

      {/* Color Palette Display */}
      {showColorPalette && (
        <Card>
          <CardHeader className={compact ? 'p-4' : ''}>
            <CardTitle>Current Color Palette</CardTitle>
            <CardDescription>Your active theme colors</CardDescription>
          </CardHeader>
          <CardContent className={compact ? 'p-4 pt-0' : ''}>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
              {[
                { name: 'Primary', value: currentColors.primary, key: 'primary' },
                { name: 'Accent', value: currentColors.accent, key: 'accent' },
                { name: 'Capture', value: currentColors.capture, key: 'capture' },
                { name: 'Processing', value: currentColors.processing, key: 'processing' },
                { name: 'Vendor', value: currentColors.vendor, key: 'vendor' },
                { name: 'Reporting', value: currentColors.reporting, key: 'reporting' },
              ].map((color) => (
                <div
                  key={color.key}
                  className="group relative overflow-hidden rounded-xl border border-border"
                >
                  <div
                    className="h-20"
                    style={{ backgroundColor: `hsl(${color.value})` }}
                  />
                  <div className="p-3 bg-card">
                    <p className="font-medium text-foreground text-sm">{color.name}</p>
                    <p className="text-xs text-muted-foreground font-mono">hsl({color.value})</p>
                  </div>
                  <button
                    onClick={() => {
                      navigator.clipboard.writeText(`hsl(${color.value})`);
                      toast.success(`Copied ${color.name} color`);
                    }}
                    className="absolute top-2 right-2 p-1.5 rounded-md bg-black/20 text-white opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <Copy className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Component Demo */}
      {showComponentDemo && (
        <Card>
          <CardHeader className={compact ? 'p-4' : ''}>
            <CardTitle>Component Preview</CardTitle>
            <CardDescription>See how components look with the current theme</CardDescription>
          </CardHeader>
          <CardContent className={`space-y-6 ${compact ? 'p-4 pt-0' : ''}`}>
            {/* Buttons */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Buttons</h4>
              <div className="flex flex-wrap gap-2">
                <Button>Primary</Button>
                <Button variant="secondary">Secondary</Button>
                <Button variant="outline">Outline</Button>
                <Button variant="ghost">Ghost</Button>
                <Button variant="destructive">Destructive</Button>
              </div>
              <div className="flex flex-wrap gap-2 mt-2">
                <Button variant="capture">Capture</Button>
                <Button variant="processing">Processing</Button>
                <Button variant="vendor">Vendor</Button>
                <Button variant="reporting">Reporting</Button>
              </div>
            </div>

            {/* Badges */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Badges</h4>
              <div className="flex flex-wrap gap-2">
                <Badge>Default</Badge>
                <Badge variant="secondary">Secondary</Badge>
                <Badge variant="outline">Outline</Badge>
                <Badge variant="destructive">Destructive</Badge>
              </div>
              <div className="flex flex-wrap gap-2 mt-2">
                <span className="chip-primary chip">Primary</span>
                <span className="chip-capture chip">Capture</span>
                <span className="chip-processing chip">Processing</span>
                <span className="chip-vendor chip">Vendor</span>
                <span className="chip-reporting chip">Reporting</span>
              </div>
            </div>

            {/* Status Badges */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Status Badges</h4>
              <div className="flex flex-wrap gap-2">
                <span className="status-badge status-badge-pending">Pending</span>
                <span className="status-badge status-badge-approved">Approved</span>
                <span className="status-badge status-badge-rejected">Rejected</span>
                <span className="status-badge status-badge-processing">Processing</span>
              </div>
            </div>

            {/* Module Badges */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Module Badges</h4>
              <div className="flex flex-wrap gap-2">
                <span className="module-badge module-badge-capture">OCR</span>
                <span className="module-badge module-badge-processing">Processing</span>
                <span className="module-badge module-badge-vendor">Vendors</span>
                <span className="module-badge module-badge-reporting">Reports</span>
              </div>
            </div>

            {/* Cards */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Cards with Gradients</h4>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                <Card gradient="capture" className="p-4">
                  <FileText className="w-5 h-5 text-capture mb-2" />
                  <p className="text-sm font-medium">Capture</p>
                </Card>
                <Card gradient="processing" className="p-4">
                  <CheckCircle className="w-5 h-5 text-processing mb-2" />
                  <p className="text-sm font-medium">Processing</p>
                </Card>
                <Card gradient="vendor" className="p-4">
                  <Users className="w-5 h-5 text-vendor mb-2" />
                  <p className="text-sm font-medium">Vendors</p>
                </Card>
                <Card gradient="reporting" className="p-4">
                  <TrendingUp className="w-5 h-5 text-reporting mb-2" />
                  <p className="text-sm font-medium">Reports</p>
                </Card>
              </div>
            </div>

            {/* Stat Cards */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Stat Cards</h4>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                {[
                  { label: 'Pending', value: '24', icon: Clock, color: 'warning' },
                  { label: 'Approved', value: '156', icon: CheckCircle, color: 'success' },
                  { label: 'Processing', value: '8', icon: AlertCircle, color: 'primary' },
                  { label: 'Vendors', value: '42', icon: Users, color: 'vendor' },
                ].map((stat) => (
                  <div key={stat.label} className="stat-card">
                    <div className={`p-2 rounded-lg bg-${stat.color}/10 w-fit`}>
                      <stat.icon className={`w-4 h-4 text-${stat.color}`} />
                    </div>
                    <p className="stat-value mt-2">{stat.value}</p>
                    <p className="stat-label">{stat.label}</p>
                  </div>
                ))}
              </div>
            </div>

            {/* Progress Bars */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Progress Bars</h4>
              <div className="space-y-3">
                <div>
                  <div className="flex justify-between text-sm mb-1">
                    <span className="text-muted-foreground">Primary</span>
                    <span className="text-foreground font-medium">75%</span>
                  </div>
                  <div className="progress-bar">
                    <div className="progress-bar-fill" style={{ width: '75%' }} />
                  </div>
                </div>
                <div>
                  <div className="flex justify-between text-sm mb-1">
                    <span className="text-muted-foreground">Capture</span>
                    <span className="text-foreground font-medium">60%</span>
                  </div>
                  <div className="progress-bar progress-bar-capture">
                    <div className="progress-bar-fill" style={{ width: '60%' }} />
                  </div>
                </div>
              </div>
            </div>

            {/* Gradient Text */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Gradient Text</h4>
              <div className="space-y-2">
                <p className="text-2xl font-bold text-gradient-primary">Primary Gradient</p>
                <p className="text-2xl font-bold text-gradient-capture">Capture Gradient</p>
                <p className="text-2xl font-bold text-gradient-processing">Processing Gradient</p>
              </div>
            </div>

            {/* Icons with Containers */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Icon Containers</h4>
              <div className="flex gap-3">
                <div className="icon-container icon-container-md icon-container-primary">
                  <Star className="w-5 h-5" />
                </div>
                <div className="icon-container icon-container-md icon-container-capture">
                  <FileText className="w-5 h-5" />
                </div>
                <div className="icon-container icon-container-md icon-container-processing">
                  <CheckCircle className="w-5 h-5" />
                </div>
                <div className="icon-container icon-container-md icon-container-vendor">
                  <Users className="w-5 h-5" />
                </div>
                <div className="icon-container icon-container-md icon-container-reporting">
                  <TrendingUp className="w-5 h-5" />
                </div>
              </div>
            </div>

            {/* Action Cards */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Action Cards</h4>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="action-card group">
                  <div className="action-card-icon bg-capture/10">
                    <Upload className="w-5 h-5 text-capture" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Upload Invoice</p>
                    <p className="text-sm text-muted-foreground">Scan and process</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground transition-colors" />
                </div>
                <div className="action-card group">
                  <div className="action-card-icon bg-processing/10">
                    <CheckCircle className="w-5 h-5 text-processing" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Review Queue</p>
                    <p className="text-sm text-muted-foreground">5 items pending</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground transition-colors" />
                </div>
              </div>
            </div>

            {/* Keyboard Shortcuts */}
            <div>
              <h4 className="text-sm font-medium text-foreground mb-3">Keyboard Shortcuts</h4>
              <div className="flex gap-4">
                <div className="flex items-center gap-2">
                  <kbd className="kbd">⌘</kbd>
                  <kbd className="kbd">K</kbd>
                  <span className="text-sm text-muted-foreground">Command Palette</span>
                </div>
                <div className="flex items-center gap-2">
                  <kbd className="kbd">⌘</kbd>
                  <kbd className="kbd">S</kbd>
                  <span className="text-sm text-muted-foreground">Save</span>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// Compact theme selector for quick switching
export function ThemeSelector() {
  const { presetId, setPreset, clearCustomColors, isOrgThemeActive } = useThemeStore();

  return (
    <div className="flex flex-wrap gap-2">
      {themePresets.slice(0, 8).map((preset) => {
        const isSelected = presetId === preset.id;
        const gradient = generateGradient(preset);

        return (
          <button
            key={preset.id}
            onClick={() => {
              if (!isOrgThemeActive) {
                clearCustomColors();
                setPreset(preset.id);
              }
            }}
            disabled={isOrgThemeActive}
            className={`
              w-8 h-8 rounded-lg transition-all
              ${isSelected ? 'ring-2 ring-primary ring-offset-2' : ''}
              ${isOrgThemeActive ? 'opacity-50 cursor-not-allowed' : 'hover:scale-110'}
            `}
            style={{ background: gradient }}
            title={preset.name}
          >
            {isSelected && (
              <Check className="w-4 h-4 text-white m-auto drop-shadow" />
            )}
          </button>
        );
      })}
    </div>
  );
}
