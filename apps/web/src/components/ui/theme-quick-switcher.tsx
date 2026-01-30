'use client';

import * as React from 'react';
import { useState, useRef, useEffect } from 'react';
import { cn } from '@/lib/utils';
import {
  useThemeStore,
  themePresets,
  generateGradient,
  ThemePreset,
} from '@/stores/theme';
import {
  Palette,
  Sun,
  Moon,
  Monitor,
  Check,
  ChevronDown,
  Sparkles,
} from 'lucide-react';
import { toast } from 'sonner';

interface ThemeQuickSwitcherProps {
  showModeToggle?: boolean;
  showPresetPicker?: boolean;
  compact?: boolean;
  className?: string;
}

export function ThemeQuickSwitcher({
  showModeToggle = true,
  showPresetPicker = true,
  compact = false,
  className,
}: ThemeQuickSwitcherProps) {
  const { mode, setMode, presetId, setPreset, customColors, clearCustomColors, getCurrentColors } = useThemeStore();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const colors = getCurrentColors();

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handlePresetSelect = (preset: ThemePreset) => {
    clearCustomColors();
    setPreset(preset.id);
    toast.success(`Applied ${preset.name} theme`);
  };

  const currentPreset = themePresets.find((p) => p.id === presetId);
  const currentGradient = currentPreset ? generateGradient(currentPreset) : `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`;

  // Group presets by category
  const categories = ['modern', 'bright', 'vibrant', 'professional'] as const;
  const categoryLabels: Record<string, string> = {
    modern: 'Modern',
    bright: 'Bright',
    vibrant: 'Vibrant',
    professional: 'Professional',
  };

  if (compact) {
    return (
      <div className={cn('flex items-center gap-1', className)} ref={dropdownRef}>
        {/* Mode Toggle */}
        {showModeToggle && (
          <div className="flex items-center gap-0.5 p-0.5 bg-secondary/50 rounded-md">
            {[
              { id: 'light', icon: Sun },
              { id: 'dark', icon: Moon },
            ].map((option) => (
              <button
                key={option.id}
                onClick={() => setMode(option.id as 'light' | 'dark')}
                className={cn(
                  'p-1.5 rounded transition-all',
                  mode === option.id
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                )}
              >
                <option.icon className="w-3.5 h-3.5" />
              </button>
            ))}
          </div>
        )}

        {/* Preset Picker */}
        {showPresetPicker && (
          <div className="relative">
            <button
              onClick={() => setIsOpen(!isOpen)}
              className="w-6 h-6 rounded-md border border-border hover:border-primary/30 transition-all hover:scale-110"
              style={{ background: currentGradient }}
              title={currentPreset?.name || 'Custom theme'}
            />

            {isOpen && (
              <div className="absolute right-0 top-full mt-2 p-2 bg-card border border-border rounded-xl shadow-soft-lg z-50 animate-scale-in">
                <div className="grid grid-cols-5 gap-1.5 w-40">
                  {themePresets.slice(0, 15).map((preset) => {
                    const isSelected = presetId === preset.id && !customColors;
                    const gradient = generateGradient(preset);

                    return (
                      <button
                        key={preset.id}
                        onClick={() => {
                          handlePresetSelect(preset);
                          setIsOpen(false);
                        }}
                        className={cn(
                          'w-6 h-6 rounded-md transition-all hover:scale-110',
                          isSelected && 'ring-2 ring-primary ring-offset-1 ring-offset-background'
                        )}
                        style={{ background: gradient }}
                        title={preset.name}
                      />
                    );
                  })}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={cn('relative', className)} ref={dropdownRef}>
      {/* Trigger Button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-secondary/50 hover:bg-secondary transition-colors"
      >
        <div
          className="w-5 h-5 rounded-md"
          style={{ background: currentGradient }}
        />
        <span className="text-sm font-medium text-foreground hidden sm:inline">
          {currentPreset?.name || 'Custom'}
        </span>
        <ChevronDown className={cn('w-4 h-4 text-muted-foreground transition-transform', isOpen && 'rotate-180')} />
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-80 bg-card border border-border rounded-xl shadow-soft-lg z-50 overflow-hidden animate-scale-in">
          {/* Mode Toggle */}
          {showModeToggle && (
            <div className="p-3 border-b border-border">
              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                Appearance
              </p>
              <div className="flex items-center gap-1 p-1 bg-secondary/50 rounded-lg">
                {[
                  { id: 'light', icon: Sun, label: 'Light' },
                  { id: 'dark', icon: Moon, label: 'Dark' },
                  { id: 'system', icon: Monitor, label: 'System' },
                ].map((option) => (
                  <button
                    key={option.id}
                    onClick={() => setMode(option.id as 'light' | 'dark' | 'system')}
                    className={cn(
                      'flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-all',
                      mode === option.id
                        ? 'bg-background shadow-sm text-foreground'
                        : 'text-muted-foreground hover:text-foreground'
                    )}
                  >
                    <option.icon className="w-4 h-4" />
                    {option.label}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Preset Picker */}
          {showPresetPicker && (
            <div className="p-3 max-h-72 overflow-y-auto">
              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                Color Theme
              </p>

              {categories.map((category) => {
                const categoryPresets = themePresets.filter((p) => p.category === category);
                if (categoryPresets.length === 0) return null;

                return (
                  <div key={category} className="mb-3 last:mb-0">
                    <p className="text-[10px] font-medium text-muted-foreground uppercase mb-1.5">
                      {categoryLabels[category]}
                    </p>
                    <div className="grid grid-cols-6 gap-1.5">
                      {categoryPresets.slice(0, 6).map((preset) => {
                        const isSelected = presetId === preset.id && !customColors;
                        const gradient = generateGradient(preset);

                        return (
                          <button
                            key={preset.id}
                            onClick={() => {
                              handlePresetSelect(preset);
                              setIsOpen(false);
                            }}
                            className={cn(
                              'relative aspect-square rounded-lg transition-all hover:scale-105',
                              isSelected && 'ring-2 ring-primary ring-offset-2 ring-offset-background'
                            )}
                            style={{ background: gradient }}
                            title={preset.name}
                          >
                            {isSelected && (
                              <div className="absolute inset-0 flex items-center justify-center">
                                <Check className="w-4 h-4 text-white drop-shadow-md" />
                              </div>
                            )}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {/* Footer */}
          <div className="p-3 border-t border-border bg-secondary/30">
            <a
              href="/settings/theme"
              className="flex items-center justify-center gap-2 text-sm font-medium text-primary hover:text-primary/80 transition-colors"
              onClick={() => setIsOpen(false)}
            >
              <Sparkles className="w-4 h-4" />
              Customize Theme
            </a>
          </div>
        </div>
      )}
    </div>
  );
}

// Mini theme toggle for tight spaces
export function ThemeModeToggle({ className }: { className?: string }) {
  const { mode, setMode } = useThemeStore();

  return (
    <div className={cn('flex items-center gap-0.5 p-0.5 bg-secondary/50 rounded-lg', className)}>
      {[
        { id: 'light', icon: Sun },
        { id: 'dark', icon: Moon },
        { id: 'system', icon: Monitor },
      ].map((option) => (
        <button
          key={option.id}
          onClick={() => setMode(option.id as 'light' | 'dark' | 'system')}
          className={cn(
            'p-1.5 rounded-md transition-all',
            mode === option.id
              ? 'bg-background shadow-sm text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          )}
          title={option.id.charAt(0).toUpperCase() + option.id.slice(1)}
        >
          <option.icon className="w-4 h-4" />
        </button>
      ))}
    </div>
  );
}

// Color preset row for inline display
export function ThemePresetRow({
  count = 8,
  size = 'md',
  className,
}: {
  count?: number;
  size?: 'sm' | 'md' | 'lg';
  className?: string;
}) {
  const { presetId, setPreset, customColors, clearCustomColors } = useThemeStore();

  const sizeClasses = {
    sm: 'w-5 h-5',
    md: 'w-7 h-7',
    lg: 'w-9 h-9',
  };

  return (
    <div className={cn('flex gap-2', className)}>
      {themePresets.slice(0, count).map((preset) => {
        const isSelected = presetId === preset.id && !customColors;
        const gradient = generateGradient(preset);

        return (
          <button
            key={preset.id}
            onClick={() => {
              clearCustomColors();
              setPreset(preset.id);
            }}
            className={cn(
              'rounded-full transition-transform hover:scale-110',
              sizeClasses[size],
              isSelected && 'ring-2 ring-offset-2 ring-offset-background ring-primary'
            )}
            style={{ background: gradient }}
            title={preset.name}
          />
        );
      })}
    </div>
  );
}
