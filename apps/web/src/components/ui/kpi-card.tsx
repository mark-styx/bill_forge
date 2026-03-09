'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { LucideIcon, TrendingUp, TrendingDown, ArrowUpRight } from 'lucide-react';
import Link from 'next/link';
import { BillForgeSparkline } from './charts';

type ModuleColor = 'primary' | 'accent' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'success' | 'warning' | 'error';

interface KPICardProps {
  title: string;
  value: string | number;
  icon?: LucideIcon;
  color?: ModuleColor;
  trend?: {
    value: string;
    isPositive: boolean;
    label?: string;
  };
  sparklineData?: number[];
  href?: string;
  className?: string;
  loading?: boolean;
  description?: string;
  animationDelay?: number;
}

const colorStyles: Record<ModuleColor, { bg: string; text: string; gradient: string }> = {
  primary: {
    bg: 'bg-primary/10',
    text: 'text-primary',
    gradient: 'from-primary to-primary/50',
  },
  accent: {
    bg: 'bg-accent/10',
    text: 'text-accent',
    gradient: 'from-accent to-accent/50',
  },
  capture: {
    bg: 'bg-capture/10',
    text: 'text-capture',
    gradient: 'from-capture to-capture/50',
  },
  processing: {
    bg: 'bg-processing/10',
    text: 'text-processing',
    gradient: 'from-processing to-processing/50',
  },
  vendor: {
    bg: 'bg-vendor/10',
    text: 'text-vendor',
    gradient: 'from-vendor to-vendor/50',
  },
  reporting: {
    bg: 'bg-reporting/10',
    text: 'text-reporting',
    gradient: 'from-reporting to-reporting/50',
  },
  success: {
    bg: 'bg-success/10',
    text: 'text-success',
    gradient: 'from-success to-success/50',
  },
  warning: {
    bg: 'bg-warning/10',
    text: 'text-warning',
    gradient: 'from-warning to-warning/50',
  },
  error: {
    bg: 'bg-error/10',
    text: 'text-error',
    gradient: 'from-error to-error/50',
  },
};

export function KPICard({
  title,
  value,
  icon: Icon,
  color = 'primary',
  trend,
  sparklineData,
  href,
  className,
  loading = false,
  description,
  animationDelay = 0,
}: KPICardProps) {
  const styles = colorStyles[color];

  const content = (
    <>
      {/* Gradient accent bar */}
      <div
        className={cn('absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b', styles.gradient)}
      />

      {/* Header row */}
      <div className="flex items-center justify-between mb-3">
        {Icon && (
          <div className={cn('p-2.5 rounded-xl', styles.bg)}>
            <Icon className={cn('w-5 h-5', styles.text)} />
          </div>
        )}
        <div className="flex items-center gap-2">
          {sparklineData && sparklineData.length > 0 && (
            <BillForgeSparkline data={sparklineData} color={color} showArea />
          )}
          {trend && (
            <div
              className={cn(
                'flex items-center gap-0.5 text-xs font-medium',
                trend.isPositive ? 'text-success' : 'text-error'
              )}
            >
              {trend.isPositive ? (
                <TrendingUp className="w-3 h-3" />
              ) : (
                <TrendingDown className="w-3 h-3" />
              )}
              {trend.value}
            </div>
          )}
          {href && (
            <ArrowUpRight className="w-4 h-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
          )}
        </div>
      </div>

      {/* Value */}
      <div>
        {loading ? (
          <div className="h-8 w-20 bg-secondary animate-pulse rounded mb-1" />
        ) : (
          <p className="text-2xl font-bold text-foreground">{value}</p>
        )}
        <p className="text-sm text-muted-foreground">{title}</p>
        {description && (
          <p className="text-xs text-muted-foreground/70 mt-1">{description}</p>
        )}
      </div>

      {/* Trend label */}
      {trend?.label && (
        <p className="text-xs text-muted-foreground mt-2">{trend.label}</p>
      )}
    </>
  );

  const cardClasses = cn(
    'bright-stat-card group animate-fade-in-up relative',
    href && 'cursor-pointer',
    className
  );

  const style = animationDelay > 0 ? { animationDelay: `${animationDelay}ms` } : undefined;

  if (href) {
    return (
      <Link href={href} className={cardClasses} style={style}>
        {content}
      </Link>
    );
  }

  return (
    <div className={cardClasses} style={style}>
      {content}
    </div>
  );
}

interface KPIGridProps {
  children: React.ReactNode;
  columns?: 2 | 3 | 4;
  className?: string;
}

export function KPIGrid({ children, columns = 4, className }: KPIGridProps) {
  const gridCols = {
    2: 'sm:grid-cols-2',
    3: 'sm:grid-cols-2 lg:grid-cols-3',
    4: 'sm:grid-cols-2 lg:grid-cols-4',
  };

  return (
    <div className={cn('grid grid-cols-1 gap-4', gridCols[columns], className)}>
      {children}
    </div>
  );
}

interface MiniKPIProps {
  label: string;
  value: string | number;
  color?: ModuleColor;
  icon?: LucideIcon;
}

export function MiniKPI({ label, value, color = 'primary', icon: Icon }: MiniKPIProps) {
  const styles = colorStyles[color];

  return (
    <div className="flex items-center justify-between p-3 bg-secondary/50 rounded-xl">
      <div className="flex items-center gap-3">
        {Icon ? (
          <div className={cn('p-1.5 rounded-lg', styles.bg)}>
            <Icon className={cn('w-4 h-4', styles.text)} />
          </div>
        ) : (
          <div className={cn('w-2 h-8 rounded-full', styles.bg.replace('/10', ''))} />
        )}
        <span className="text-sm text-muted-foreground">{label}</span>
      </div>
      <span className="text-lg font-semibold text-foreground">{value}</span>
    </div>
  );
}

interface ComparisonKPIProps {
  title: string;
  current: number;
  previous: number;
  format?: (value: number) => string;
  color?: ModuleColor;
  icon?: LucideIcon;
}

export function ComparisonKPI({
  title,
  current,
  previous,
  format = (v) => v.toString(),
  color = 'primary',
  icon: Icon,
}: ComparisonKPIProps) {
  const styles = colorStyles[color];
  const change = previous > 0 ? ((current - previous) / previous) * 100 : 0;
  const isPositive = change >= 0;

  return (
    <div className="card p-4">
      <div className="flex items-center gap-2 mb-3">
        {Icon && (
          <div className={cn('p-2 rounded-lg', styles.bg)}>
            <Icon className={cn('w-4 h-4', styles.text)} />
          </div>
        )}
        <span className="text-sm font-medium text-muted-foreground">{title}</span>
      </div>
      <div className="flex items-end justify-between">
        <div>
          <p className="text-2xl font-bold text-foreground">{format(current)}</p>
          <p className="text-xs text-muted-foreground">
            vs {format(previous)} last period
          </p>
        </div>
        <div
          className={cn(
            'flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium',
            isPositive ? 'bg-success/10 text-success' : 'bg-error/10 text-error'
          )}
        >
          {isPositive ? (
            <TrendingUp className="w-3 h-3" />
          ) : (
            <TrendingDown className="w-3 h-3" />
          )}
          {Math.abs(change).toFixed(1)}%
        </div>
      </div>
    </div>
  );
}

interface ProgressKPIProps {
  title: string;
  value: number;
  target: number;
  color?: ModuleColor;
  icon?: LucideIcon;
  showPercentage?: boolean;
}

export function ProgressKPI({
  title,
  value,
  target,
  color = 'primary',
  icon: Icon,
  showPercentage = true,
}: ProgressKPIProps) {
  const styles = colorStyles[color];
  const percentage = Math.min((value / target) * 100, 100);

  return (
    <div className="card p-4">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          {Icon && (
            <div className={cn('p-2 rounded-lg', styles.bg)}>
              <Icon className={cn('w-4 h-4', styles.text)} />
            </div>
          )}
          <span className="text-sm font-medium text-foreground">{title}</span>
        </div>
        {showPercentage && (
          <span className={cn('text-sm font-semibold', styles.text)}>
            {percentage.toFixed(0)}%
          </span>
        )}
      </div>
      <div className="space-y-2">
        <div className="h-2 bg-secondary rounded-full overflow-hidden">
          <div
            className={cn('h-full rounded-full transition-all duration-500 bg-gradient-to-r', styles.gradient)}
            style={{ width: `${percentage}%` }}
          />
        </div>
        <div className="flex justify-between text-xs text-muted-foreground">
          <span>{value.toLocaleString()} completed</span>
          <span>Target: {target.toLocaleString()}</span>
        </div>
      </div>
    </div>
  );
}
