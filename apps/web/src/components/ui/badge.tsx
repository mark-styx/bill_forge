import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeVariants = cva(
  'inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2',
  {
    variants: {
      variant: {
        default:
          'border-transparent bg-primary text-primary-foreground shadow',
        secondary:
          'border-transparent bg-secondary text-secondary-foreground',
        destructive:
          'border-transparent bg-destructive text-destructive-foreground shadow',
        outline: 'text-foreground',
        // Module badges
        capture:
          'border-transparent bg-capture/15 text-capture',
        processing:
          'border-transparent bg-processing/15 text-processing',
        vendor:
          'border-transparent bg-vendor/15 text-vendor',
        reporting:
          'border-transparent bg-reporting/15 text-reporting',
        // Status badges
        success:
          'border-transparent bg-success/15 text-success',
        warning:
          'border-transparent bg-warning/15 text-warning',
        error:
          'border-transparent bg-error/15 text-error',
        info:
          'border-transparent bg-primary/15 text-primary',
        // Processing status badges
        pending:
          'border-transparent bg-warning/15 text-warning',
        approved:
          'border-transparent bg-success/15 text-success',
        rejected:
          'border-transparent bg-error/15 text-error',
        draft:
          'border-transparent bg-secondary text-muted-foreground',
      },
      size: {
        default: 'px-2.5 py-0.5 text-xs',
        sm: 'px-2 py-0.5 text-[10px]',
        lg: 'px-3 py-1 text-sm',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {
  icon?: React.ReactNode;
}

function Badge({ className, variant, size, icon, children, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant, size }), className)} {...props}>
      {icon && <span className="mr-1 -ml-0.5">{icon}</span>}
      {children}
    </div>
  );
}

export { Badge, badgeVariants };
