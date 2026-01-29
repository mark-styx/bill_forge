'use client';

import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import {
  AlertCircle,
  CheckCircle,
  Info,
  AlertTriangle,
  X,
} from 'lucide-react';
import { cn } from '@/lib/utils';

const alertVariants = cva(
  cn(
    'relative w-full rounded-xl border p-4',
    'flex gap-3 items-start',
    '[&>svg]:flex-shrink-0 [&>svg]:mt-0.5'
  ),
  {
    variants: {
      variant: {
        default: 'bg-background border-border text-foreground',
        info: cn(
          'bg-primary/5 border-primary/20 text-primary',
          '[&>svg]:text-primary'
        ),
        success: cn(
          'bg-success/5 border-success/20 text-success',
          '[&>svg]:text-success'
        ),
        warning: cn(
          'bg-warning/5 border-warning/20 text-warning',
          '[&>svg]:text-warning'
        ),
        error: cn(
          'bg-error/5 border-error/20 text-error',
          '[&>svg]:text-error'
        ),
        destructive: cn(
          'bg-destructive/5 border-destructive/20 text-destructive',
          '[&>svg]:text-destructive'
        ),
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

const alertIcons = {
  default: Info,
  info: Info,
  success: CheckCircle,
  warning: AlertTriangle,
  error: AlertCircle,
  destructive: AlertCircle,
};

export interface AlertProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof alertVariants> {
  icon?: React.ReactNode;
  onClose?: () => void;
}

const Alert = React.forwardRef<HTMLDivElement, AlertProps>(
  ({ className, variant, icon, onClose, children, ...props }, ref) => {
    const IconComponent = alertIcons[variant || 'default'];
    const displayIcon = icon !== undefined ? icon : <IconComponent className="h-5 w-5" />;

    return (
      <div
        ref={ref}
        role="alert"
        className={cn(alertVariants({ variant }), className)}
        {...props}
      >
        {displayIcon}
        <div className="flex-1 min-w-0">{children}</div>
        {onClose && (
          <button
            onClick={onClose}
            className={cn(
              'flex-shrink-0 rounded-md p-1 -m-1',
              'opacity-70 hover:opacity-100 transition-opacity',
              'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2'
            )}
          >
            <X className="h-4 w-4" />
            <span className="sr-only">Dismiss</span>
          </button>
        )}
      </div>
    );
  }
);
Alert.displayName = 'Alert';

const AlertTitle = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h5
    ref={ref}
    className={cn('font-semibold leading-none tracking-tight', className)}
    {...props}
  />
));
AlertTitle.displayName = 'AlertTitle';

const AlertDescription = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn('mt-1 text-sm opacity-90 [&_p]:leading-relaxed', className)}
    {...props}
  />
));
AlertDescription.displayName = 'AlertDescription';

// Banner component for full-width notifications
const bannerVariants = cva(
  cn(
    'relative w-full py-3 px-4',
    'flex items-center justify-center gap-3',
    'text-sm font-medium'
  ),
  {
    variants: {
      variant: {
        default: 'bg-background border-b border-border text-foreground',
        info: 'bg-primary text-primary-foreground',
        success: 'bg-success text-success-foreground',
        warning: 'bg-warning text-warning-foreground',
        error: 'bg-error text-error-foreground',
        gradient: cn(
          'bg-gradient-to-r from-primary to-accent text-white',
          'border-none'
        ),
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export interface BannerProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof bannerVariants> {
  icon?: React.ReactNode;
  action?: React.ReactNode;
  onClose?: () => void;
}

const Banner = React.forwardRef<HTMLDivElement, BannerProps>(
  ({ className, variant, icon, action, onClose, children, ...props }, ref) => {
    return (
      <div
        ref={ref}
        role="alert"
        className={cn(bannerVariants({ variant }), className)}
        {...props}
      >
        <div className="flex items-center justify-center gap-3 flex-1">
          {icon && <span className="flex-shrink-0">{icon}</span>}
          <span>{children}</span>
          {action && <span className="flex-shrink-0">{action}</span>}
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className={cn(
              'absolute right-4 top-1/2 -translate-y-1/2',
              'rounded-md p-1 opacity-70 hover:opacity-100 transition-opacity',
              'focus:outline-none focus:ring-2 focus:ring-white/50'
            )}
          >
            <X className="h-4 w-4" />
            <span className="sr-only">Dismiss</span>
          </button>
        )}
      </div>
    );
  }
);
Banner.displayName = 'Banner';

// Callout component for highlighted content
const calloutVariants = cva(
  cn(
    'relative rounded-xl p-4',
    'flex gap-3 items-start',
    '[&>svg]:flex-shrink-0 [&>svg]:mt-0.5'
  ),
  {
    variants: {
      variant: {
        default: 'bg-secondary/50 border-l-4 border-primary',
        info: 'bg-primary/5 border-l-4 border-primary',
        success: 'bg-success/5 border-l-4 border-success',
        warning: 'bg-warning/5 border-l-4 border-warning',
        error: 'bg-error/5 border-l-4 border-error',
        tip: 'bg-accent/5 border-l-4 border-accent',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export interface CalloutProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof calloutVariants> {
  icon?: React.ReactNode;
  title?: string;
}

const Callout = React.forwardRef<HTMLDivElement, CalloutProps>(
  ({ className, variant, icon, title, children, ...props }, ref) => {
    const defaultIcons = {
      default: Info,
      info: Info,
      success: CheckCircle,
      warning: AlertTriangle,
      error: AlertCircle,
      tip: Info,
    };
    const IconComponent = defaultIcons[variant || 'default'];
    const displayIcon = icon !== undefined ? icon : <IconComponent className="h-5 w-5" />;

    return (
      <div
        ref={ref}
        className={cn(calloutVariants({ variant }), className)}
        {...props}
      >
        <span
          className={cn(
            variant === 'info' && 'text-primary',
            variant === 'success' && 'text-success',
            variant === 'warning' && 'text-warning',
            variant === 'error' && 'text-error',
            variant === 'tip' && 'text-accent',
            (variant === 'default' || !variant) && 'text-primary'
          )}
        >
          {displayIcon}
        </span>
        <div className="flex-1 min-w-0">
          {title && (
            <p className="font-semibold text-foreground mb-1">{title}</p>
          )}
          <div className="text-sm text-muted-foreground">{children}</div>
        </div>
      </div>
    );
  }
);
Callout.displayName = 'Callout';

export { Alert, AlertTitle, AlertDescription, Banner, Callout };
