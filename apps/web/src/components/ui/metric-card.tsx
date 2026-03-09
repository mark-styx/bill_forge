'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { TrendingUp, TrendingDown, Minus } from 'lucide-react';

interface MetricCardProps extends React.HTMLAttributes<HTMLDivElement> {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: React.ReactNode;
  trend?: {
    value: number;
    direction: 'up' | 'down' | 'neutral';
    label?: string;
  };
  variant?: 'default' | 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting';
  size?: 'sm' | 'md' | 'lg';
  loading?: boolean;
}

const variantStyles = {
  default: {
    indicator: 'bg-primary',
    iconBg: 'bg-primary/10',
    iconColor: 'text-primary',
  },
  primary: {
    indicator: 'bg-primary',
    iconBg: 'bg-primary/10',
    iconColor: 'text-primary',
  },
  capture: {
    indicator: 'bg-capture',
    iconBg: 'bg-capture/10',
    iconColor: 'text-capture',
  },
  processing: {
    indicator: 'bg-processing',
    iconBg: 'bg-processing/10',
    iconColor: 'text-processing',
  },
  vendor: {
    indicator: 'bg-vendor',
    iconBg: 'bg-vendor/10',
    iconColor: 'text-vendor',
  },
  reporting: {
    indicator: 'bg-reporting',
    iconBg: 'bg-reporting/10',
    iconColor: 'text-reporting',
  },
};

const sizeStyles = {
  sm: {
    padding: 'p-3',
    title: 'text-xs',
    value: 'text-xl',
    icon: 'w-8 h-8',
  },
  md: {
    padding: 'p-4',
    title: 'text-sm',
    value: 'text-2xl',
    icon: 'w-10 h-10',
  },
  lg: {
    padding: 'p-6',
    title: 'text-sm',
    value: 'text-3xl',
    icon: 'w-12 h-12',
  },
};

export function MetricCard({
  title,
  value,
  subtitle,
  icon,
  trend,
  variant = 'default',
  size = 'md',
  loading = false,
  className,
  ...props
}: MetricCardProps) {
  const styles = variantStyles[variant];
  const sizes = sizeStyles[size];

  const TrendIcon = trend?.direction === 'up'
    ? TrendingUp
    : trend?.direction === 'down'
      ? TrendingDown
      : Minus;

  const trendColor = trend?.direction === 'up'
    ? 'text-success'
    : trend?.direction === 'down'
      ? 'text-error'
      : 'text-muted-foreground';

  return (
    <div
      className={cn(
        'relative rounded-xl bg-card border border-border overflow-hidden card-shine transition-all duration-200 hover:shadow-soft',
        sizes.padding,
        className
      )}
      {...props}
    >
      {/* Colored indicator bar */}
      <div className={cn('absolute top-0 left-0 w-1 h-full', styles.indicator)} />

      <div className="flex items-start justify-between gap-4 pl-3">
        <div className="flex-1 min-w-0">
          <p className={cn('font-medium text-muted-foreground', sizes.title)}>
            {title}
          </p>

          {loading ? (
            <div className="animate-pulse mt-1">
              <div className="h-8 w-24 bg-secondary rounded" />
            </div>
          ) : (
            <p className={cn('font-bold text-foreground mt-1', sizes.value)}>
              {value}
            </p>
          )}

          {subtitle && (
            <p className="text-xs text-muted-foreground mt-1">{subtitle}</p>
          )}

          {trend && !loading && (
            <div className={cn('flex items-center gap-1 mt-2', trendColor)}>
              <TrendIcon className="w-3.5 h-3.5" />
              <span className="text-xs font-medium">
                {trend.value > 0 && '+'}
                {trend.value}%
              </span>
              {trend.label && (
                <span className="text-xs text-muted-foreground ml-1">
                  {trend.label}
                </span>
              )}
            </div>
          )}
        </div>

        {icon && (
          <div className={cn(
            'flex items-center justify-center rounded-lg flex-shrink-0',
            styles.iconBg,
            sizes.icon
          )}>
            <div className={styles.iconColor}>{icon}</div>
          </div>
        )}
      </div>
    </div>
  );
}

interface MetricGridProps extends React.HTMLAttributes<HTMLDivElement> {
  columns?: 1 | 2 | 3 | 4 | 5;
}

export function MetricGrid({
  columns = 4,
  className,
  children,
  ...props
}: MetricGridProps) {
  const colClasses = {
    1: 'grid-cols-1',
    2: 'grid-cols-1 sm:grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
    5: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-5',
  };

  return (
    <div
      className={cn('grid gap-4', colClasses[columns], className)}
      {...props}
    >
      {children}
    </div>
  );
}
