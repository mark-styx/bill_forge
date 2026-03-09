'use client';

import * as React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { cn } from '@/lib/utils';
import { LucideIcon } from 'lucide-react';

interface ModuleNavItem {
  id: string;
  name: string;
  description?: string;
  href: string;
  icon: LucideIcon;
  module: 'capture' | 'processing' | 'vendor' | 'reporting';
  badge?: string | number;
  children?: {
    name: string;
    href: string;
    icon?: LucideIcon;
  }[];
}

interface ModuleNavProps extends React.HTMLAttributes<HTMLElement> {
  items: ModuleNavItem[];
  variant?: 'pills' | 'tabs' | 'underline';
  size?: 'sm' | 'md' | 'lg';
  showIcons?: boolean;
  showBadges?: boolean;
}

const moduleColors = {
  capture: {
    active: 'bg-capture text-capture-foreground',
    inactive: 'text-capture hover:bg-capture/10',
    badge: 'bg-capture/20 text-capture',
    indicator: 'bg-capture',
    border: 'border-capture',
  },
  processing: {
    active: 'bg-processing text-processing-foreground',
    inactive: 'text-processing hover:bg-processing/10',
    badge: 'bg-processing/20 text-processing',
    indicator: 'bg-processing',
    border: 'border-processing',
  },
  vendor: {
    active: 'bg-vendor text-vendor-foreground',
    inactive: 'text-vendor hover:bg-vendor/10',
    badge: 'bg-vendor/20 text-vendor',
    indicator: 'bg-vendor',
    border: 'border-vendor',
  },
  reporting: {
    active: 'bg-reporting text-reporting-foreground',
    inactive: 'text-reporting hover:bg-reporting/10',
    badge: 'bg-reporting/20 text-reporting',
    indicator: 'bg-reporting',
    border: 'border-reporting',
  },
};

export function ModuleNav({
  items,
  variant = 'pills',
  size = 'md',
  showIcons = true,
  showBadges = true,
  className,
  ...props
}: ModuleNavProps) {
  const pathname = usePathname();

  const sizeClasses = {
    sm: {
      item: 'px-3 py-1.5 text-xs',
      icon: 'w-3.5 h-3.5',
      gap: 'gap-1.5',
    },
    md: {
      item: 'px-4 py-2 text-sm',
      icon: 'w-4 h-4',
      gap: 'gap-2',
    },
    lg: {
      item: 'px-5 py-2.5 text-sm',
      icon: 'w-5 h-5',
      gap: 'gap-2.5',
    },
  };

  const sizes = sizeClasses[size];

  return (
    <nav
      className={cn(
        'flex',
        variant === 'pills' && 'gap-2 flex-wrap',
        variant === 'tabs' && 'border-b border-border gap-0',
        variant === 'underline' && 'gap-4 border-b border-border',
        className
      )}
      {...props}
    >
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = pathname === item.href || pathname.startsWith(`${item.href}/`);
        const colors = moduleColors[item.module];

        if (variant === 'pills') {
          return (
            <Link
              key={item.id}
              href={item.href}
              className={cn(
                'flex items-center font-medium rounded-lg transition-all duration-200',
                sizes.item,
                sizes.gap,
                isActive ? colors.active : colors.inactive
              )}
            >
              {showIcons && <Icon className={sizes.icon} />}
              <span>{item.name}</span>
              {showBadges && item.badge !== undefined && (
                <span
                  className={cn(
                    'px-1.5 py-0.5 rounded-full text-xs font-semibold',
                    colors.badge
                  )}
                >
                  {item.badge}
                </span>
              )}
            </Link>
          );
        }

        if (variant === 'tabs') {
          return (
            <Link
              key={item.id}
              href={item.href}
              className={cn(
                'relative flex items-center font-medium transition-all duration-200 border-b-2 -mb-px',
                sizes.item,
                sizes.gap,
                isActive
                  ? `${colors.border} text-foreground`
                  : 'border-transparent text-muted-foreground hover:text-foreground'
              )}
            >
              {showIcons && (
                <Icon
                  className={cn(
                    sizes.icon,
                    isActive ? colors.inactive.replace('hover:', '') : ''
                  )}
                />
              )}
              <span>{item.name}</span>
              {showBadges && item.badge !== undefined && (
                <span
                  className={cn(
                    'px-1.5 py-0.5 rounded-full text-xs font-semibold',
                    colors.badge
                  )}
                >
                  {item.badge}
                </span>
              )}
            </Link>
          );
        }

        // underline variant
        return (
          <Link
            key={item.id}
            href={item.href}
            className={cn(
              'relative flex items-center font-medium pb-3 transition-all duration-200',
              sizes.gap,
              isActive ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'
            )}
          >
            {showIcons && (
              <Icon
                className={cn(
                  sizes.icon,
                  isActive && colors.inactive.replace('hover:', '').replace(/bg-\S+\/\d+/, '')
                )}
              />
            )}
            <span>{item.name}</span>
            {showBadges && item.badge !== undefined && (
              <span
                className={cn(
                  'px-1.5 py-0.5 rounded-full text-xs font-semibold',
                  colors.badge
                )}
              >
                {item.badge}
              </span>
            )}
            {isActive && (
              <div className={cn('absolute bottom-0 left-0 right-0 h-0.5', colors.indicator)} />
            )}
          </Link>
        );
      })}
    </nav>
  );
}

interface ModuleCardNavProps extends React.HTMLAttributes<HTMLDivElement> {
  items: ModuleNavItem[];
  columns?: 2 | 3 | 4;
}

export function ModuleCardNav({
  items,
  columns = 4,
  className,
  ...props
}: ModuleCardNavProps) {
  const pathname = usePathname();

  const columnClasses = {
    2: 'grid-cols-1 sm:grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
  };

  return (
    <div className={cn('grid gap-4', columnClasses[columns], className)} {...props}>
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = pathname === item.href || pathname.startsWith(`${item.href}/`);
        const colors = moduleColors[item.module];

        return (
          <Link
            key={item.id}
            href={item.href}
            className={cn(
              'group relative p-4 rounded-xl border transition-all duration-200',
              'hover:shadow-md card-shine',
              isActive
                ? `${colors.border} border-2 bg-card`
                : 'border-border hover:border-opacity-50'
            )}
          >
            {/* Gradient indicator */}
            <div
              className={cn(
                'absolute top-0 left-0 right-0 h-1 rounded-t-xl opacity-0 transition-opacity',
                colors.indicator,
                isActive && 'opacity-100'
              )}
            />

            <div className="flex items-start justify-between">
              <div
                className={cn(
                  'flex items-center justify-center w-10 h-10 rounded-lg',
                  `bg-${item.module}/10`
                )}
                style={{ backgroundColor: `hsl(var(--${item.module}) / 0.1)` }}
              >
                <Icon
                  className="w-5 h-5"
                  style={{ color: `hsl(var(--${item.module}))` }}
                />
              </div>
              {item.badge !== undefined && (
                <span
                  className={cn(
                    'px-2 py-0.5 rounded-full text-xs font-semibold',
                    colors.badge
                  )}
                >
                  {item.badge}
                </span>
              )}
            </div>

            <div className="mt-3">
              <h3 className="font-semibold text-foreground">{item.name}</h3>
              {item.description && (
                <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
                  {item.description}
                </p>
              )}
            </div>

            {/* Children links */}
            {item.children && item.children.length > 0 && (
              <div className="mt-3 pt-3 border-t border-border space-y-1">
                {item.children.slice(0, 3).map((child) => {
                  const ChildIcon = child.icon;
                  return (
                    <Link
                      key={child.href}
                      href={child.href}
                      className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
                      onClick={(e) => e.stopPropagation()}
                    >
                      {ChildIcon && <ChildIcon className="w-3 h-3" />}
                      <span>{child.name}</span>
                    </Link>
                  );
                })}
              </div>
            )}
          </Link>
        );
      })}
    </div>
  );
}
