import * as React from 'react';
import { cn } from '@/lib/utils';
import { LucideIcon, TrendingUp, TrendingDown, ArrowUpRight } from 'lucide-react';
import Link from 'next/link';

interface StatCardProps {
  title: string;
  value: string | number;
  icon?: LucideIcon;
  iconColor?: string;
  iconBgColor?: string;
  trend?: {
    value: string;
    isPositive: boolean;
    label?: string;
  };
  href?: string;
  className?: string;
}

export function StatCard({
  title,
  value,
  icon: Icon,
  iconColor = 'text-primary',
  iconBgColor = 'bg-primary/10',
  trend,
  href,
  className,
}: StatCardProps) {
  const content = (
    <>
      <div className="flex items-start justify-between">
        {Icon && (
          <div className={cn('p-2 rounded-lg', iconBgColor)}>
            <Icon className={cn('w-5 h-5', iconColor)} />
          </div>
        )}
        {href && (
          <ArrowUpRight className="w-4 h-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
        )}
      </div>
      <div className="mt-3">
        <p className="text-2xl font-semibold text-foreground">{value}</p>
        <p className="text-sm text-muted-foreground mt-0.5">{title}</p>
      </div>
      {trend && (
        <div
          className={cn(
            'mt-2 flex items-center gap-1 text-xs font-medium',
            trend.isPositive ? 'text-success' : 'text-error'
          )}
        >
          {trend.isPositive ? (
            <TrendingUp className="w-3 h-3" />
          ) : (
            <TrendingDown className="w-3 h-3" />
          )}
          {trend.value}
          {trend.label && (
            <span className="text-muted-foreground font-normal">
              {' '}
              {trend.label}
            </span>
          )}
        </div>
      )}
    </>
  );

  const cardClasses = cn(
    'card p-4 group',
    href && 'card-hover cursor-pointer',
    className
  );

  if (href) {
    return (
      <Link href={href} className={cardClasses}>
        {content}
      </Link>
    );
  }

  return <div className={cardClasses}>{content}</div>;
}
