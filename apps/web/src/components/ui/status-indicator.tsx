'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { Check, X, Clock, AlertTriangle, Loader2, Pause, Circle } from 'lucide-react';

type StatusType =
  | 'success'
  | 'error'
  | 'warning'
  | 'pending'
  | 'processing'
  | 'paused'
  | 'draft'
  | 'approved'
  | 'rejected'
  | 'review';

interface StatusIndicatorProps extends React.HTMLAttributes<HTMLDivElement> {
  status: StatusType;
  label?: string;
  showIcon?: boolean;
  showDot?: boolean;
  size?: 'sm' | 'md' | 'lg';
  variant?: 'default' | 'outline' | 'subtle';
  pulse?: boolean;
}

const statusConfig: Record<StatusType, {
  label: string;
  icon: typeof Check;
  color: string;
  bgColor: string;
  borderColor: string;
  dotColor: string;
}> = {
  success: {
    label: 'Success',
    icon: Check,
    color: 'text-success',
    bgColor: 'bg-success/10',
    borderColor: 'border-success/30',
    dotColor: 'bg-success',
  },
  approved: {
    label: 'Approved',
    icon: Check,
    color: 'text-success',
    bgColor: 'bg-success/10',
    borderColor: 'border-success/30',
    dotColor: 'bg-success',
  },
  error: {
    label: 'Error',
    icon: X,
    color: 'text-error',
    bgColor: 'bg-error/10',
    borderColor: 'border-error/30',
    dotColor: 'bg-error',
  },
  rejected: {
    label: 'Rejected',
    icon: X,
    color: 'text-error',
    bgColor: 'bg-error/10',
    borderColor: 'border-error/30',
    dotColor: 'bg-error',
  },
  warning: {
    label: 'Warning',
    icon: AlertTriangle,
    color: 'text-warning',
    bgColor: 'bg-warning/10',
    borderColor: 'border-warning/30',
    dotColor: 'bg-warning',
  },
  pending: {
    label: 'Pending',
    icon: Clock,
    color: 'text-warning',
    bgColor: 'bg-warning/10',
    borderColor: 'border-warning/30',
    dotColor: 'bg-warning',
  },
  processing: {
    label: 'Processing',
    icon: Loader2,
    color: 'text-primary',
    bgColor: 'bg-primary/10',
    borderColor: 'border-primary/30',
    dotColor: 'bg-primary',
  },
  paused: {
    label: 'Paused',
    icon: Pause,
    color: 'text-muted-foreground',
    bgColor: 'bg-secondary',
    borderColor: 'border-border',
    dotColor: 'bg-muted-foreground',
  },
  draft: {
    label: 'Draft',
    icon: Circle,
    color: 'text-muted-foreground',
    bgColor: 'bg-secondary',
    borderColor: 'border-border',
    dotColor: 'bg-muted-foreground',
  },
  review: {
    label: 'In Review',
    icon: Clock,
    color: 'text-primary',
    bgColor: 'bg-primary/10',
    borderColor: 'border-primary/30',
    dotColor: 'bg-primary',
  },
};

const sizeStyles = {
  sm: {
    badge: 'px-2 py-0.5 text-xs gap-1',
    icon: 'w-3 h-3',
    dot: 'w-1.5 h-1.5',
  },
  md: {
    badge: 'px-2.5 py-1 text-xs gap-1.5',
    icon: 'w-3.5 h-3.5',
    dot: 'w-2 h-2',
  },
  lg: {
    badge: 'px-3 py-1.5 text-sm gap-2',
    icon: 'w-4 h-4',
    dot: 'w-2.5 h-2.5',
  },
};

export function StatusIndicator({
  status,
  label,
  showIcon = true,
  showDot = false,
  size = 'md',
  variant = 'default',
  pulse = false,
  className,
  ...props
}: StatusIndicatorProps) {
  const config = statusConfig[status];
  const sizes = sizeStyles[size];
  const Icon = config.icon;
  const displayLabel = label || config.label;

  const isSpinning = status === 'processing';

  return (
    <div
      className={cn(
        'inline-flex items-center font-medium rounded-full',
        sizes.badge,
        variant === 'default' && config.bgColor,
        variant === 'outline' && `border ${config.borderColor} bg-transparent`,
        variant === 'subtle' && 'bg-transparent',
        config.color,
        className
      )}
      {...props}
    >
      {showDot && (
        <span
          className={cn(
            'rounded-full flex-shrink-0',
            sizes.dot,
            config.dotColor,
            (pulse || status === 'processing') && 'animate-pulse'
          )}
        />
      )}
      {showIcon && !showDot && (
        <Icon
          className={cn(
            sizes.icon,
            'flex-shrink-0',
            isSpinning && 'animate-spin'
          )}
        />
      )}
      <span>{displayLabel}</span>
    </div>
  );
}

interface StatusDotProps extends React.HTMLAttributes<HTMLSpanElement> {
  status: StatusType;
  size?: 'sm' | 'md' | 'lg';
  pulse?: boolean;
}

export function StatusDot({
  status,
  size = 'md',
  pulse = false,
  className,
  ...props
}: StatusDotProps) {
  const config = statusConfig[status];
  const dotSizes = {
    sm: 'w-1.5 h-1.5',
    md: 'w-2 h-2',
    lg: 'w-3 h-3',
  };

  return (
    <span
      className={cn(
        'rounded-full flex-shrink-0',
        dotSizes[size],
        config.dotColor,
        (pulse || status === 'processing') && 'animate-pulse',
        className
      )}
      {...props}
    />
  );
}

interface StatusStepsProps extends React.HTMLAttributes<HTMLDivElement> {
  steps: {
    id: string;
    label: string;
    status: 'complete' | 'current' | 'upcoming';
    description?: string;
  }[];
  variant?: 'default' | 'compact';
}

export function StatusSteps({
  steps,
  variant = 'default',
  className,
  ...props
}: StatusStepsProps) {
  if (variant === 'compact') {
    return (
      <div className={cn('flex items-center gap-1', className)} {...props}>
        {steps.map((step, index) => (
          <React.Fragment key={step.id}>
            <div
              className={cn(
                'w-2 h-2 rounded-full transition-colors',
                step.status === 'complete' && 'bg-success',
                step.status === 'current' && 'bg-primary animate-pulse',
                step.status === 'upcoming' && 'bg-border'
              )}
              title={step.label}
            />
            {index < steps.length - 1 && (
              <div
                className={cn(
                  'w-4 h-0.5',
                  step.status === 'complete' ? 'bg-success' : 'bg-border'
                )}
              />
            )}
          </React.Fragment>
        ))}
      </div>
    );
  }

  return (
    <div className={cn('flex items-start', className)} {...props}>
      {steps.map((step, index) => (
        <React.Fragment key={step.id}>
          <div className="flex flex-col items-center">
            <div
              className={cn(
                'w-8 h-8 rounded-full flex items-center justify-center border-2 transition-all',
                step.status === 'complete' && 'bg-success border-success text-white',
                step.status === 'current' && 'bg-primary/10 border-primary text-primary',
                step.status === 'upcoming' && 'bg-secondary border-border text-muted-foreground'
              )}
            >
              {step.status === 'complete' ? (
                <Check className="w-4 h-4" />
              ) : (
                <span className="text-xs font-semibold">{index + 1}</span>
              )}
            </div>
            <div className="mt-2 text-center">
              <p
                className={cn(
                  'text-sm font-medium',
                  step.status === 'upcoming' ? 'text-muted-foreground' : 'text-foreground'
                )}
              >
                {step.label}
              </p>
              {step.description && (
                <p className="text-xs text-muted-foreground mt-0.5">
                  {step.description}
                </p>
              )}
            </div>
          </div>
          {index < steps.length - 1 && (
            <div
              className={cn(
                'flex-1 h-0.5 mt-4 mx-2',
                step.status === 'complete' ? 'bg-success' : 'bg-border'
              )}
            />
          )}
        </React.Fragment>
      ))}
    </div>
  );
}
