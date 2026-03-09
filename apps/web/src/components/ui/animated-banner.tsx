'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { X, ArrowRight, Sparkles, Bell, Info, AlertTriangle, CheckCircle } from 'lucide-react';
import Link from 'next/link';

type BannerVariant = 'gradient' | 'info' | 'success' | 'warning' | 'announcement';

interface AnimatedBannerProps {
  title: string;
  description?: string;
  variant?: BannerVariant;
  action?: {
    label: string;
    href?: string;
    onClick?: () => void;
  };
  dismissible?: boolean;
  onDismiss?: () => void;
  className?: string;
  icon?: React.ReactNode;
  animated?: boolean;
}

const variantStyles: Record<BannerVariant, { bg: string; border: string; icon: typeof Info }> = {
  gradient: {
    bg: 'bg-gradient-to-r from-primary/10 via-accent/5 to-transparent',
    border: 'border-primary/20',
    icon: Sparkles,
  },
  info: {
    bg: 'bg-primary/5',
    border: 'border-primary/20',
    icon: Info,
  },
  success: {
    bg: 'bg-success/5',
    border: 'border-success/20',
    icon: CheckCircle,
  },
  warning: {
    bg: 'bg-warning/5',
    border: 'border-warning/20',
    icon: AlertTriangle,
  },
  announcement: {
    bg: 'bg-secondary/50',
    border: 'border-border',
    icon: Bell,
  },
};

export function AnimatedBanner({
  title,
  description,
  variant = 'gradient',
  action,
  dismissible = false,
  onDismiss,
  className,
  icon,
  animated = true,
}: AnimatedBannerProps) {
  const [isVisible, setIsVisible] = React.useState(true);
  const styles = variantStyles[variant];
  const Icon = styles.icon;

  const handleDismiss = () => {
    setIsVisible(false);
    onDismiss?.();
  };

  if (!isVisible) return null;

  return (
    <div
      className={cn(
        'relative overflow-hidden rounded-xl border p-4',
        styles.bg,
        styles.border,
        animated && 'animate-fade-in',
        className
      )}
    >
      {/* Animated background effect */}
      {variant === 'gradient' && animated && (
        <div className="absolute inset-0 bg-gradient-to-r from-primary/5 via-accent/10 to-primary/5 animate-gradient-shift opacity-50" />
      )}

      <div className="relative flex items-start gap-3">
        {/* Icon */}
        <div
          className={cn(
            'flex-shrink-0 p-2 rounded-lg',
            variant === 'gradient' && 'bg-primary/10 text-primary',
            variant === 'info' && 'bg-primary/10 text-primary',
            variant === 'success' && 'bg-success/10 text-success',
            variant === 'warning' && 'bg-warning/10 text-warning',
            variant === 'announcement' && 'bg-secondary text-muted-foreground'
          )}
        >
          {icon || <Icon className="w-5 h-5" />}
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <h4 className="font-semibold text-foreground">{title}</h4>
          {description && (
            <p className="text-sm text-muted-foreground mt-0.5">{description}</p>
          )}
          {action && (
            <div className="mt-2">
              {action.href ? (
                <Link
                  href={action.href}
                  className="inline-flex items-center gap-1 text-sm font-medium text-primary hover:underline"
                >
                  {action.label}
                  <ArrowRight className="w-3.5 h-3.5" />
                </Link>
              ) : (
                <button
                  onClick={action.onClick}
                  className="inline-flex items-center gap-1 text-sm font-medium text-primary hover:underline"
                >
                  {action.label}
                  <ArrowRight className="w-3.5 h-3.5" />
                </button>
              )}
            </div>
          )}
        </div>

        {/* Dismiss button */}
        {dismissible && (
          <button
            onClick={handleDismiss}
            className="flex-shrink-0 p-1 text-muted-foreground hover:text-foreground rounded-lg hover:bg-secondary/50 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>
    </div>
  );
}

interface OrganizationBannerProps {
  organizationName: string;
  logoUrl?: string;
  tagline?: string;
  primaryColor?: string;
  accentColor?: string;
  className?: string;
}

export function OrganizationBanner({
  organizationName,
  logoUrl,
  tagline,
  primaryColor,
  accentColor,
  className,
}: OrganizationBannerProps) {
  const gradientStyle = primaryColor && accentColor
    ? { background: `linear-gradient(135deg, hsl(${primaryColor}), hsl(${accentColor}))` }
    : undefined;

  return (
    <div
      className={cn(
        'relative overflow-hidden rounded-2xl p-6 text-white',
        !gradientStyle && 'bg-gradient-to-r from-primary to-accent',
        className
      )}
      style={gradientStyle}
    >
      {/* Pattern overlay */}
      <div
        className="absolute inset-0 opacity-10"
        style={{
          backgroundImage: `url("data:image/svg+xml,%3Csvg width='60' height='60' viewBox='0 0 60 60' xmlns='http://www.w3.org/2000/svg'%3E%3Cg fill='none' fill-rule='evenodd'%3E%3Cg fill='%23ffffff' fill-opacity='0.4'%3E%3Cpath d='M36 34v-4h-2v4h-4v2h4v4h2v-4h4v-2h-4zm0-30V0h-2v4h-4v2h4v4h2V6h4V4h-4zM6 34v-4H4v4H0v2h4v4h2v-4h4v-2H6zM6 4V0H4v4H0v2h4v4h2V6h4V4H6z'/%3E%3C/g%3E%3C/g%3E%3C/svg%3E")`,
        }}
      />

      <div className="relative flex items-center gap-4">
        {/* Logo */}
        {logoUrl ? (
          <img
            src={logoUrl}
            alt={organizationName}
            className="w-12 h-12 rounded-xl object-contain bg-white/10 p-1"
          />
        ) : (
          <div className="w-12 h-12 rounded-xl bg-white/20 flex items-center justify-center text-2xl font-bold">
            {organizationName.charAt(0)}
          </div>
        )}

        {/* Text */}
        <div>
          <h2 className="text-xl font-bold">{organizationName}</h2>
          {tagline && <p className="text-white/80 text-sm">{tagline}</p>}
        </div>
      </div>
    </div>
  );
}

interface FeatureBannerProps {
  title: string;
  description: string;
  features: string[];
  action?: {
    label: string;
    href?: string;
    onClick?: () => void;
  };
  image?: string;
  className?: string;
}

export function FeatureBanner({
  title,
  description,
  features,
  action,
  image,
  className,
}: FeatureBannerProps) {
  return (
    <div
      className={cn(
        'relative overflow-hidden rounded-2xl bg-gradient-to-br from-card via-card to-secondary/50 border border-border p-6',
        className
      )}
    >
      <div className="flex flex-col lg:flex-row gap-6">
        {/* Content */}
        <div className="flex-1">
          <h3 className="text-xl font-bold text-foreground mb-2">{title}</h3>
          <p className="text-muted-foreground mb-4">{description}</p>

          <ul className="space-y-2 mb-4">
            {features.map((feature, index) => (
              <li key={index} className="flex items-center gap-2 text-sm text-foreground">
                <CheckCircle className="w-4 h-4 text-success flex-shrink-0" />
                {feature}
              </li>
            ))}
          </ul>

          {action && (
            action.href ? (
              <Link
                href={action.href}
                className="btn btn-primary btn-sm inline-flex"
              >
                {action.label}
                <ArrowRight className="w-4 h-4 ml-1" />
              </Link>
            ) : (
              <button
                onClick={action.onClick}
                className="btn btn-primary btn-sm inline-flex"
              >
                {action.label}
                <ArrowRight className="w-4 h-4 ml-1" />
              </button>
            )
          )}
        </div>

        {/* Image */}
        {image && (
          <div className="flex-shrink-0 lg:w-1/3">
            <img
              src={image}
              alt={title}
              className="w-full h-full object-cover rounded-xl"
            />
          </div>
        )}
      </div>

      {/* Decorative gradient */}
      <div className="absolute top-0 right-0 w-64 h-64 bg-gradient-radial from-primary/10 to-transparent rounded-full -translate-y-1/2 translate-x-1/2" />
    </div>
  );
}

interface StatusBannerProps {
  status: 'online' | 'maintenance' | 'degraded' | 'offline';
  message?: string;
  className?: string;
}

const statusConfig = {
  online: {
    bg: 'bg-success/10',
    border: 'border-success/30',
    text: 'text-success',
    label: 'All systems operational',
    dot: 'bg-success',
  },
  maintenance: {
    bg: 'bg-warning/10',
    border: 'border-warning/30',
    text: 'text-warning',
    label: 'Scheduled maintenance',
    dot: 'bg-warning',
  },
  degraded: {
    bg: 'bg-warning/10',
    border: 'border-warning/30',
    text: 'text-warning',
    label: 'Degraded performance',
    dot: 'bg-warning animate-pulse',
  },
  offline: {
    bg: 'bg-error/10',
    border: 'border-error/30',
    text: 'text-error',
    label: 'System offline',
    dot: 'bg-error animate-pulse',
  },
};

export function StatusBanner({ status, message, className }: StatusBannerProps) {
  const config = statusConfig[status];

  return (
    <div
      className={cn(
        'flex items-center gap-3 px-4 py-2 rounded-lg border',
        config.bg,
        config.border,
        className
      )}
    >
      <div className={cn('w-2 h-2 rounded-full', config.dot)} />
      <span className={cn('text-sm font-medium', config.text)}>
        {message || config.label}
      </span>
    </div>
  );
}
