'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { useThemeStore, generateGradient } from '@/stores/theme';
import { Building2, Check } from 'lucide-react';

interface OrgBrandingBannerProps extends React.HTMLAttributes<HTMLDivElement> {
  brandName?: string;
  logoUrl?: string;
  showThemeInfo?: boolean;
  variant?: 'default' | 'compact' | 'hero';
}

export function OrgBrandingBanner({
  brandName,
  logoUrl,
  showThemeInfo = true,
  variant = 'default',
  className,
  ...props
}: OrgBrandingBannerProps) {
  const { organizationTheme, getCurrentColors, getCurrentPreset, isOrgThemeActive } = useThemeStore();
  const colors = getCurrentColors();
  const currentPreset = getCurrentPreset();

  const displayName = brandName || organizationTheme?.branding?.brandName || 'Your Organization';
  const displayLogo = logoUrl || organizationTheme?.branding?.logoUrl;

  if (variant === 'compact') {
    return (
      <div
        className={cn(
          'flex items-center gap-3 px-3 py-2 rounded-lg',
          'bg-gradient-to-r from-primary/5 to-accent/5 border border-primary/10',
          className
        )}
        {...props}
      >
        {displayLogo ? (
          <img
            src={displayLogo}
            alt={displayName}
            className="h-6 w-auto object-contain"
          />
        ) : (
          <div
            className="w-6 h-6 rounded flex items-center justify-center text-white text-xs font-bold"
            style={{
              background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`,
            }}
          >
            {displayName[0]?.toUpperCase()}
          </div>
        )}
        <span className="text-sm font-medium text-foreground">{displayName}</span>
        {isOrgThemeActive && (
          <span className="text-xs text-primary flex items-center gap-1">
            <Check className="w-3 h-3" />
            Branded
          </span>
        )}
      </div>
    );
  }

  if (variant === 'hero') {
    return (
      <div
        className={cn(
          'relative overflow-hidden rounded-2xl p-8',
          'bg-gradient-to-br from-primary/10 via-accent/5 to-transparent',
          className
        )}
        style={{
          backgroundImage: `
            radial-gradient(circle at 20% 50%, hsl(${colors.primary} / 0.15) 0%, transparent 50%),
            radial-gradient(circle at 80% 50%, hsl(${colors.accent} / 0.1) 0%, transparent 50%)
          `,
        }}
        {...props}
      >
        <div className="relative z-10 flex items-center gap-6">
          {displayLogo ? (
            <img
              src={displayLogo}
              alt={displayName}
              className="h-16 w-auto object-contain"
            />
          ) : (
            <div
              className="w-16 h-16 rounded-xl flex items-center justify-center text-white text-2xl font-bold shadow-lg"
              style={{
                background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`,
              }}
            >
              {displayName[0]?.toUpperCase()}
            </div>
          )}
          <div>
            <h1 className="text-2xl font-bold text-foreground">{displayName}</h1>
            {showThemeInfo && currentPreset && (
              <p className="text-sm text-muted-foreground mt-1">
                Theme: {currentPreset.name}
              </p>
            )}
          </div>
        </div>
        {/* Decorative gradient orbs */}
        <div
          className="absolute top-0 right-0 w-64 h-64 opacity-30 blur-3xl pointer-events-none"
          style={{ background: `hsl(${colors.primary})` }}
        />
        <div
          className="absolute bottom-0 left-1/2 w-48 h-48 opacity-20 blur-3xl pointer-events-none"
          style={{ background: `hsl(${colors.accent})` }}
        />
      </div>
    );
  }

  // Default variant
  return (
    <div
      className={cn(
        'flex items-center justify-between px-4 py-3 rounded-xl',
        'bg-gradient-to-r from-primary/5 to-accent/5 border border-primary/10',
        className
      )}
      {...props}
    >
      <div className="flex items-center gap-4">
        {displayLogo ? (
          <img
            src={displayLogo}
            alt={displayName}
            className="h-10 w-auto object-contain"
          />
        ) : (
          <div
            className="w-10 h-10 rounded-lg flex items-center justify-center text-white text-lg font-bold shadow-sm"
            style={{
              background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`,
            }}
          >
            {displayName[0]?.toUpperCase()}
          </div>
        )}
        <div>
          <p className="font-semibold text-foreground">{displayName}</p>
          {showThemeInfo && currentPreset && (
            <p className="text-xs text-muted-foreground">
              {currentPreset.name} theme
            </p>
          )}
        </div>
      </div>
      {isOrgThemeActive && (
        <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 rounded-lg">
          <Building2 className="w-4 h-4 text-primary" />
          <span className="text-xs font-medium text-primary">Organization Theme</span>
        </div>
      )}
    </div>
  );
}

interface ThemePreviewCardProps extends React.HTMLAttributes<HTMLDivElement> {
  preset: {
    id: string;
    name: string;
    description: string;
    colors: {
      primary: string;
      accent: string;
    };
  };
  isSelected?: boolean;
  onSelect?: () => void;
}

export function ThemePreviewCard({
  preset,
  isSelected = false,
  onSelect,
  className,
  ...props
}: ThemePreviewCardProps) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        'relative p-3 rounded-xl border-2 text-left transition-all w-full',
        isSelected
          ? 'border-primary bg-primary/5 shadow-sm'
          : 'border-border hover:border-primary/30',
        className
      )}
      {...props}
    >
      <div
        className="w-full h-12 rounded-lg mb-2"
        style={{
          background: `linear-gradient(135deg, hsl(${preset.colors.primary}), hsl(${preset.colors.accent}))`,
        }}
      />
      <p className="font-medium text-foreground text-sm">{preset.name}</p>
      <p className="text-xs text-muted-foreground line-clamp-1">
        {preset.description}
      </p>
      {isSelected && (
        <div className="absolute top-2 right-2 w-5 h-5 rounded-full bg-primary flex items-center justify-center">
          <Check className="w-3 h-3 text-white" />
        </div>
      )}
    </button>
  );
}

interface ColorSwatchGridProps extends React.HTMLAttributes<HTMLDivElement> {
  colors: {
    primary: string;
    accent: string;
    capture: string;
    processing: string;
    vendor: string;
    reporting: string;
  };
  showLabels?: boolean;
  size?: 'sm' | 'md' | 'lg';
}

export function ColorSwatchGrid({
  colors,
  showLabels = true,
  size = 'md',
  className,
  ...props
}: ColorSwatchGridProps) {
  const colorEntries = [
    { key: 'primary', label: 'Primary', color: colors.primary },
    { key: 'accent', label: 'Accent', color: colors.accent },
    { key: 'capture', label: 'Capture', color: colors.capture },
    { key: 'processing', label: 'Processing', color: colors.processing },
    { key: 'vendor', label: 'Vendor', color: colors.vendor },
    { key: 'reporting', label: 'Reporting', color: colors.reporting },
  ];

  const sizeClasses = {
    sm: 'w-6 h-6',
    md: 'w-8 h-8',
    lg: 'w-12 h-12',
  };

  return (
    <div
      className={cn('flex flex-wrap gap-2', className)}
      {...props}
    >
      {colorEntries.map((entry) => (
        <div key={entry.key} className="flex items-center gap-2">
          <div
            className={cn('rounded-lg border border-border', sizeClasses[size])}
            style={{ backgroundColor: `hsl(${entry.color})` }}
            title={`${entry.label}: ${entry.color}`}
          />
          {showLabels && (
            <span className="text-xs text-muted-foreground">{entry.label}</span>
          )}
        </div>
      ))}
    </div>
  );
}
