'use client';

import * as React from 'react';
import Link from 'next/link';
import { cn } from '@/lib/utils';
import { LucideIcon, ArrowRight, Clock } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';

interface ActivityItem {
  id: string;
  type: 'success' | 'warning' | 'error' | 'info' | 'capture' | 'processing' | 'vendor' | 'reporting';
  title: string;
  description?: string;
  timestamp: Date | string;
  icon: LucideIcon;
  href?: string;
  user?: {
    name: string;
    avatar?: string;
  };
}

interface ActivityFeedProps extends React.HTMLAttributes<HTMLDivElement> {
  activities: ActivityItem[];
  title?: string;
  showViewAll?: boolean;
  viewAllHref?: string;
  maxItems?: number;
  emptyMessage?: string;
  loading?: boolean;
}

const typeStyles = {
  success: {
    iconBg: 'bg-success/10',
    iconColor: 'text-success',
    dotColor: 'bg-success',
  },
  warning: {
    iconBg: 'bg-warning/10',
    iconColor: 'text-warning',
    dotColor: 'bg-warning',
  },
  error: {
    iconBg: 'bg-error/10',
    iconColor: 'text-error',
    dotColor: 'bg-error',
  },
  info: {
    iconBg: 'bg-primary/10',
    iconColor: 'text-primary',
    dotColor: 'bg-primary',
  },
  capture: {
    iconBg: 'bg-capture/10',
    iconColor: 'text-capture',
    dotColor: 'bg-capture',
  },
  processing: {
    iconBg: 'bg-processing/10',
    iconColor: 'text-processing',
    dotColor: 'bg-processing',
  },
  vendor: {
    iconBg: 'bg-vendor/10',
    iconColor: 'text-vendor',
    dotColor: 'bg-vendor',
  },
  reporting: {
    iconBg: 'bg-reporting/10',
    iconColor: 'text-reporting',
    dotColor: 'bg-reporting',
  },
};

export function ActivityFeed({
  activities,
  title = 'Recent Activity',
  showViewAll = true,
  viewAllHref = '/activity',
  maxItems = 5,
  emptyMessage = 'No recent activity',
  loading = false,
  className,
  ...props
}: ActivityFeedProps) {
  const displayedActivities = activities.slice(0, maxItems);

  if (loading) {
    return (
      <div className={cn('space-y-4', className)} {...props}>
        {title && (
          <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            {title}
          </h3>
        )}
        <div className="rounded-xl bg-card border border-border p-4">
          <div className="space-y-4">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="flex items-start gap-3 animate-pulse">
                <div className="w-8 h-8 rounded-lg bg-secondary" />
                <div className="flex-1 space-y-2">
                  <div className="h-4 w-3/4 bg-secondary rounded" />
                  <div className="h-3 w-1/4 bg-secondary rounded" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={cn('space-y-4', className)} {...props}>
      {title && (
        <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          {title}
        </h3>
      )}
      <div className="rounded-xl bg-card border border-border p-4">
        {displayedActivities.length === 0 ? (
          <div className="text-center py-8">
            <div className="w-12 h-12 mx-auto rounded-full bg-secondary flex items-center justify-center mb-3">
              <Clock className="w-5 h-5 text-muted-foreground" />
            </div>
            <p className="text-sm text-muted-foreground">{emptyMessage}</p>
          </div>
        ) : (
          <div className="space-y-4">
            {displayedActivities.map((activity, index) => {
              const styles = typeStyles[activity.type];
              const Icon = activity.icon;
              const formattedTime = typeof activity.timestamp === 'string'
                ? activity.timestamp
                : formatDistanceToNow(activity.timestamp, { addSuffix: true });

              const content = (
                <div
                  className={cn(
                    'flex items-start gap-3 group',
                    activity.href && 'cursor-pointer'
                  )}
                >
                  <div
                    className={cn(
                      'flex items-center justify-center w-8 h-8 rounded-lg flex-shrink-0',
                      styles.iconBg
                    )}
                  >
                    <Icon className={cn('w-4 h-4', styles.iconColor)} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className={cn(
                      'text-sm text-foreground',
                      activity.href && 'group-hover:text-primary transition-colors'
                    )}>
                      {activity.title}
                    </p>
                    {activity.description && (
                      <p className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                        {activity.description}
                      </p>
                    )}
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-xs text-muted-foreground">
                        {formattedTime}
                      </span>
                      {activity.user && (
                        <>
                          <span className="text-xs text-muted-foreground">•</span>
                          <span className="text-xs text-muted-foreground">
                            {activity.user.name}
                          </span>
                        </>
                      )}
                    </div>
                  </div>
                  {/* Timeline connector */}
                  {index < displayedActivities.length - 1 && (
                    <div className="absolute left-[19px] top-10 bottom-0 w-px bg-border" />
                  )}
                </div>
              );

              return (
                <div key={activity.id} className="relative">
                  {activity.href ? (
                    <Link href={activity.href}>{content}</Link>
                  ) : (
                    content
                  )}
                </div>
              );
            })}
          </div>
        )}

        {showViewAll && activities.length > maxItems && (
          <Link
            href={viewAllHref}
            className="flex items-center justify-center gap-1 text-sm text-primary hover:text-primary/80 transition-colors mt-4 pt-4 border-t border-border"
          >
            View all activity
            <ArrowRight className="w-4 h-4" />
          </Link>
        )}
      </div>
    </div>
  );
}

interface ActivityTimelineProps extends React.HTMLAttributes<HTMLDivElement> {
  activities: ActivityItem[];
  groupByDate?: boolean;
}

export function ActivityTimeline({
  activities,
  groupByDate = false,
  className,
  ...props
}: ActivityTimelineProps) {
  return (
    <div className={cn('relative', className)} {...props}>
      {/* Timeline line */}
      <div className="absolute left-4 top-2 bottom-2 w-px bg-border" />

      <div className="space-y-6">
        {activities.map((activity) => {
          const styles = typeStyles[activity.type];
          const Icon = activity.icon;
          const formattedTime = typeof activity.timestamp === 'string'
            ? activity.timestamp
            : formatDistanceToNow(activity.timestamp, { addSuffix: true });

          const content = (
            <div className="relative flex items-start gap-4 pl-10">
              {/* Timeline dot */}
              <div
                className={cn(
                  'absolute left-2 w-4 h-4 rounded-full border-2 border-background',
                  styles.dotColor
                )}
              />

              <div
                className={cn(
                  'flex items-center justify-center w-10 h-10 rounded-xl flex-shrink-0',
                  styles.iconBg
                )}
              >
                <Icon className={cn('w-5 h-5', styles.iconColor)} />
              </div>

              <div className="flex-1 min-w-0 pb-6">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="text-sm font-medium text-foreground">
                      {activity.title}
                    </p>
                    {activity.description && (
                      <p className="text-sm text-muted-foreground mt-1">
                        {activity.description}
                      </p>
                    )}
                  </div>
                  <span className="text-xs text-muted-foreground whitespace-nowrap">
                    {formattedTime}
                  </span>
                </div>
                {activity.user && (
                  <div className="flex items-center gap-2 mt-2">
                    {activity.user.avatar ? (
                      <img
                        src={activity.user.avatar}
                        alt={activity.user.name}
                        className="w-5 h-5 rounded-full"
                      />
                    ) : (
                      <div className="w-5 h-5 rounded-full bg-secondary flex items-center justify-center text-xs font-medium">
                        {activity.user.name[0]}
                      </div>
                    )}
                    <span className="text-xs text-muted-foreground">
                      {activity.user.name}
                    </span>
                  </div>
                )}
              </div>
            </div>
          );

          return activity.href ? (
            <Link key={activity.id} href={activity.href} className="block hover:bg-secondary/30 rounded-lg transition-colors -mx-2 px-2">
              {content}
            </Link>
          ) : (
            <div key={activity.id}>{content}</div>
          );
        })}
      </div>
    </div>
  );
}
