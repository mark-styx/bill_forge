'use client';

import * as React from 'react';
import Link from 'next/link';
import { cn } from '@/lib/utils';
import { ArrowRight, LucideIcon } from 'lucide-react';

interface QuickAction {
  id: string;
  title: string;
  description: string;
  href: string;
  icon: LucideIcon;
  variant?: 'default' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'primary';
  badge?: string;
  disabled?: boolean;
}

interface QuickActionsGridProps extends React.HTMLAttributes<HTMLDivElement> {
  actions: QuickAction[];
  columns?: 1 | 2 | 3 | 4;
  size?: 'sm' | 'md' | 'lg';
}

const variantStyles = {
  default: {
    iconBg: 'bg-secondary',
    iconColor: 'text-foreground',
    hoverBorder: 'hover:border-primary/30',
  },
  primary: {
    iconBg: 'bg-primary/10',
    iconColor: 'text-primary',
    hoverBorder: 'hover:border-primary/30',
  },
  capture: {
    iconBg: 'bg-capture/10',
    iconColor: 'text-capture',
    hoverBorder: 'hover:border-capture/30',
  },
  processing: {
    iconBg: 'bg-processing/10',
    iconColor: 'text-processing',
    hoverBorder: 'hover:border-processing/30',
  },
  vendor: {
    iconBg: 'bg-vendor/10',
    iconColor: 'text-vendor',
    hoverBorder: 'hover:border-vendor/30',
  },
  reporting: {
    iconBg: 'bg-reporting/10',
    iconColor: 'text-reporting',
    hoverBorder: 'hover:border-reporting/30',
  },
};

export function QuickActionsGrid({
  actions,
  columns = 2,
  size = 'md',
  className,
  ...props
}: QuickActionsGridProps) {
  const columnClasses = {
    1: 'grid-cols-1',
    2: 'grid-cols-1 sm:grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
  };

  const sizeClasses = {
    sm: {
      padding: 'p-3',
      icon: 'w-8 h-8',
      iconInner: 'w-4 h-4',
      title: 'text-sm',
      description: 'text-xs',
    },
    md: {
      padding: 'p-4',
      icon: 'w-10 h-10',
      iconInner: 'w-5 h-5',
      title: 'text-sm',
      description: 'text-xs',
    },
    lg: {
      padding: 'p-5',
      icon: 'w-12 h-12',
      iconInner: 'w-6 h-6',
      title: 'text-base',
      description: 'text-sm',
    },
  };

  const sizes = sizeClasses[size];

  return (
    <div
      className={cn('grid gap-3', columnClasses[columns], className)}
      {...props}
    >
      {actions.map((action) => {
        const styles = variantStyles[action.variant || 'default'];
        const Icon = action.icon;

        const content = (
          <div
            className={cn(
              'group relative flex items-center gap-4 rounded-xl bg-card border border-border',
              'transition-all duration-200 hover:shadow-md card-shine',
              styles.hoverBorder,
              sizes.padding,
              action.disabled && 'opacity-50 cursor-not-allowed pointer-events-none'
            )}
          >
            <div
              className={cn(
                'flex items-center justify-center rounded-lg flex-shrink-0',
                styles.iconBg,
                sizes.icon
              )}
            >
              <Icon className={cn(styles.iconColor, sizes.iconInner)} />
            </div>

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <p className={cn('font-medium text-foreground', sizes.title)}>
                  {action.title}
                </p>
                {action.badge && (
                  <span className="px-1.5 py-0.5 text-xs font-medium rounded-full bg-primary/10 text-primary">
                    {action.badge}
                  </span>
                )}
              </div>
              <p className={cn('text-muted-foreground', sizes.description)}>
                {action.description}
              </p>
            </div>

            <ArrowRight
              className={cn(
                'w-4 h-4 text-muted-foreground flex-shrink-0',
                'transition-all duration-200',
                'group-hover:text-foreground group-hover:translate-x-1'
              )}
            />
          </div>
        );

        if (action.disabled) {
          return <div key={action.id}>{content}</div>;
        }

        return (
          <Link key={action.id} href={action.href}>
            {content}
          </Link>
        );
      })}
    </div>
  );
}

interface QuickActionButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  icon: LucideIcon;
  label: string;
  variant?: 'default' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'primary';
  size?: 'sm' | 'md' | 'lg';
}

export function QuickActionButton({
  icon: Icon,
  label,
  variant = 'default',
  size = 'md',
  className,
  ...props
}: QuickActionButtonProps) {
  const styles = variantStyles[variant];

  const sizeClasses = {
    sm: {
      padding: 'p-2',
      icon: 'w-6 h-6',
      iconInner: 'w-3 h-3',
      label: 'text-xs',
    },
    md: {
      padding: 'p-3',
      icon: 'w-8 h-8',
      iconInner: 'w-4 h-4',
      label: 'text-xs',
    },
    lg: {
      padding: 'p-4',
      icon: 'w-10 h-10',
      iconInner: 'w-5 h-5',
      label: 'text-sm',
    },
  };

  const sizes = sizeClasses[size];

  return (
    <button
      className={cn(
        'flex flex-col items-center gap-2 rounded-xl transition-all duration-200',
        'hover:bg-secondary/50 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary',
        sizes.padding,
        className
      )}
      {...props}
    >
      <div
        className={cn(
          'flex items-center justify-center rounded-lg',
          styles.iconBg,
          sizes.icon
        )}
      >
        <Icon className={cn(styles.iconColor, sizes.iconInner)} />
      </div>
      <span className={cn('text-muted-foreground font-medium', sizes.label)}>
        {label}
      </span>
    </button>
  );
}
