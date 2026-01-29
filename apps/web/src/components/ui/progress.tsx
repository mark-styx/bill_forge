'use client';

import * as React from 'react';
import * as ProgressPrimitive from '@radix-ui/react-progress';
import { cn } from '@/lib/utils';

const Progress = React.forwardRef<
  React.ElementRef<typeof ProgressPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof ProgressPrimitive.Root> & {
    indicatorClassName?: string;
    variant?: 'default' | 'success' | 'warning' | 'error' | 'capture' | 'processing' | 'vendor' | 'reporting';
  }
>(({ className, value, indicatorClassName, variant = 'default', ...props }, ref) => (
  <ProgressPrimitive.Root
    ref={ref}
    className={cn(
      'relative h-2 w-full overflow-hidden rounded-full bg-secondary',
      className
    )}
    {...props}
  >
    <ProgressPrimitive.Indicator
      className={cn(
        'h-full w-full flex-1 transition-all rounded-full',
        variant === 'default' && 'bg-gradient-to-r from-primary to-accent',
        variant === 'success' && 'bg-success',
        variant === 'warning' && 'bg-warning',
        variant === 'error' && 'bg-error',
        variant === 'capture' && 'bg-gradient-to-r from-capture to-capture/70',
        variant === 'processing' && 'bg-gradient-to-r from-processing to-processing/70',
        variant === 'vendor' && 'bg-gradient-to-r from-vendor to-vendor/70',
        variant === 'reporting' && 'bg-gradient-to-r from-reporting to-reporting/70',
        indicatorClassName
      )}
      style={{ transform: `translateX(-${100 - (value || 0)}%)` }}
    />
  </ProgressPrimitive.Root>
));
Progress.displayName = ProgressPrimitive.Root.displayName;

export { Progress };
