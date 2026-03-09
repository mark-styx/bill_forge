'use client';

import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface AnimatedCounterProps {
  value: number;
  duration?: number;
  decimals?: number;
  prefix?: string;
  suffix?: string;
  className?: string;
  formatValue?: (value: number) => string;
  onComplete?: () => void;
}

export function AnimatedCounter({
  value,
  duration = 1000,
  decimals = 0,
  prefix = '',
  suffix = '',
  className,
  formatValue,
  onComplete,
}: AnimatedCounterProps) {
  const [displayValue, setDisplayValue] = useState(0);
  const previousValue = useRef(0);
  const animationRef = useRef<number | null>(null);

  useEffect(() => {
    const startValue = previousValue.current;
    const endValue = value;
    const startTime = performance.now();

    const animate = (currentTime: number) => {
      const elapsed = currentTime - startTime;
      const progress = Math.min(elapsed / duration, 1);

      // Ease out cubic for smooth deceleration
      const easeOut = 1 - Math.pow(1 - progress, 3);

      const currentValue = startValue + (endValue - startValue) * easeOut;
      setDisplayValue(currentValue);

      if (progress < 1) {
        animationRef.current = requestAnimationFrame(animate);
      } else {
        setDisplayValue(endValue);
        previousValue.current = endValue;
        onComplete?.();
      }
    };

    animationRef.current = requestAnimationFrame(animate);

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, [value, duration, onComplete]);

  const formattedValue = formatValue
    ? formatValue(displayValue)
    : displayValue.toFixed(decimals);

  return (
    <span className={cn('tabular-nums', className)}>
      {prefix}
      {formattedValue}
      {suffix}
    </span>
  );
}

interface AnimatedPercentageProps {
  value: number;
  duration?: number;
  className?: string;
  showBar?: boolean;
  barClassName?: string;
  colorScheme?: 'primary' | 'success' | 'warning' | 'error' | 'capture' | 'processing' | 'vendor' | 'reporting';
}

export function AnimatedPercentage({
  value,
  duration = 1000,
  className,
  showBar = false,
  barClassName,
  colorScheme = 'primary',
}: AnimatedPercentageProps) {
  const [displayValue, setDisplayValue] = useState(0);

  useEffect(() => {
    const startTime = performance.now();
    const animate = (currentTime: number) => {
      const elapsed = currentTime - startTime;
      const progress = Math.min(elapsed / duration, 1);
      const easeOut = 1 - Math.pow(1 - progress, 3);
      setDisplayValue(value * easeOut);

      if (progress < 1) {
        requestAnimationFrame(animate);
      }
    };
    requestAnimationFrame(animate);
  }, [value, duration]);

  const colorClasses = {
    primary: 'bg-primary',
    success: 'bg-success',
    warning: 'bg-warning',
    error: 'bg-error',
    capture: 'bg-capture',
    processing: 'bg-processing',
    vendor: 'bg-vendor',
    reporting: 'bg-reporting',
  };

  const gradientClasses = {
    primary: 'from-primary to-accent',
    success: 'from-success to-emerald-400',
    warning: 'from-warning to-amber-400',
    error: 'from-error to-rose-400',
    capture: 'from-capture to-cyan-400',
    processing: 'from-processing to-teal-400',
    vendor: 'from-vendor to-purple-400',
    reporting: 'from-reporting to-yellow-400',
  };

  return (
    <div className={cn('space-y-2', className)}>
      <span className="text-2xl font-bold tabular-nums">
        {displayValue.toFixed(0)}%
      </span>
      {showBar && (
        <div className="h-2 w-full rounded-full bg-secondary overflow-hidden">
          <div
            className={cn(
              'h-full rounded-full transition-all duration-300 bg-gradient-to-r',
              gradientClasses[colorScheme],
              barClassName
            )}
            style={{ width: `${displayValue}%` }}
          />
        </div>
      )}
    </div>
  );
}

interface CountUpCardProps {
  label: string;
  value: number;
  prefix?: string;
  suffix?: string;
  icon?: React.ReactNode;
  trend?: { value: number; isPositive: boolean };
  colorScheme?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting';
  className?: string;
}

export function CountUpCard({
  label,
  value,
  prefix = '',
  suffix = '',
  icon,
  trend,
  colorScheme = 'primary',
  className,
}: CountUpCardProps) {
  const colorStyles = {
    primary: {
      bg: 'hsl(var(--primary) / 0.1)',
      text: 'hsl(var(--primary))',
      gradient: 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--accent)))',
    },
    capture: {
      bg: 'hsl(var(--capture) / 0.1)',
      text: 'hsl(var(--capture))',
      gradient: 'linear-gradient(135deg, hsl(var(--capture)), hsl(195 100% 55%))',
    },
    processing: {
      bg: 'hsl(var(--processing) / 0.1)',
      text: 'hsl(var(--processing))',
      gradient: 'linear-gradient(135deg, hsl(var(--processing)), hsl(160 84% 55%))',
    },
    vendor: {
      bg: 'hsl(var(--vendor) / 0.1)',
      text: 'hsl(var(--vendor))',
      gradient: 'linear-gradient(135deg, hsl(var(--vendor)), hsl(280 70% 60%))',
    },
    reporting: {
      bg: 'hsl(var(--reporting) / 0.1)',
      text: 'hsl(var(--reporting))',
      gradient: 'linear-gradient(135deg, hsl(var(--reporting)), hsl(45 95% 55%))',
    },
  };

  const styles = colorStyles[colorScheme];

  return (
    <div
      className={cn(
        'relative p-5 rounded-2xl bg-card border border-border overflow-hidden',
        'transition-all duration-300 hover:shadow-card-hover hover:-translate-y-0.5',
        className
      )}
    >
      {/* Side indicator */}
      <div
        className="absolute top-0 left-0 w-1 h-full rounded-l-2xl"
        style={{ background: styles.gradient }}
      />

      <div className="flex items-start justify-between">
        {icon && (
          <div
            className="p-2.5 rounded-xl"
            style={{ background: styles.bg }}
          >
            <div style={{ color: styles.text }}>{icon}</div>
          </div>
        )}
        {trend && (
          <div
            className={cn(
              'flex items-center gap-1 text-xs font-medium px-2 py-1 rounded-full',
              trend.isPositive
                ? 'bg-success/10 text-success'
                : 'bg-error/10 text-error'
            )}
          >
            <span>{trend.isPositive ? '↑' : '↓'}</span>
            <span>{Math.abs(trend.value)}%</span>
          </div>
        )}
      </div>

      <div className="mt-4">
        <AnimatedCounter
          value={value}
          prefix={prefix}
          suffix={suffix}
          className="text-3xl font-bold text-foreground"
        />
        <p className="text-sm text-muted-foreground mt-1">{label}</p>
      </div>
    </div>
  );
}
