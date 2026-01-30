'use client';

import * as React from 'react';
import { useState, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { Button } from './button';
import {
  Bell,
  Check,
  CheckCheck,
  X,
  FileText,
  Users,
  AlertCircle,
  CheckCircle,
  Clock,
  Trash2,
  Settings,
  ExternalLink,
} from 'lucide-react';
import { formatRelativeTime } from '@/lib/utils';

export interface Notification {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message: string;
  timestamp: Date | string;
  read: boolean;
  actionUrl?: string;
  actionLabel?: string;
  module?: 'capture' | 'processing' | 'vendor' | 'reporting';
}

interface NotificationCenterProps {
  notifications?: Notification[];
  onMarkAsRead?: (id: string) => void;
  onMarkAllAsRead?: () => void;
  onDelete?: (id: string) => void;
  onClearAll?: () => void;
  onActionClick?: (notification: Notification) => void;
  maxVisible?: number;
  className?: string;
}

const typeIcons = {
  info: Bell,
  success: CheckCircle,
  warning: AlertCircle,
  error: AlertCircle,
};

const typeColors = {
  info: {
    bg: 'bg-primary/10',
    text: 'text-primary',
    border: 'border-primary/20',
  },
  success: {
    bg: 'bg-success/10',
    text: 'text-success',
    border: 'border-success/20',
  },
  warning: {
    bg: 'bg-warning/10',
    text: 'text-warning',
    border: 'border-warning/20',
  },
  error: {
    bg: 'bg-error/10',
    text: 'text-error',
    border: 'border-error/20',
  },
};

const moduleColors = {
  capture: 'text-capture',
  processing: 'text-processing',
  vendor: 'text-vendor',
  reporting: 'text-reporting',
};

export function NotificationCenter({
  notifications = [],
  onMarkAsRead,
  onMarkAllAsRead,
  onDelete,
  onClearAll,
  onActionClick,
  maxVisible = 10,
  className,
}: NotificationCenterProps) {
  const [isOpen, setIsOpen] = useState(false);
  const unreadCount = notifications.filter((n) => !n.read).length;
  const visibleNotifications = notifications.slice(0, maxVisible);

  // Close on escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setIsOpen(false);
    };
    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, []);

  return (
    <div className={cn('relative', className)}>
      {/* Trigger Button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="relative p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
      >
        <Bell className="w-5 h-5" />
        {unreadCount > 0 && (
          <span className="absolute -top-0.5 -right-0.5 w-5 h-5 bg-error text-white text-[10px] font-bold rounded-full flex items-center justify-center shadow-sm">
            {unreadCount > 9 ? '9+' : unreadCount}
          </span>
        )}
      </button>

      {/* Dropdown Panel */}
      {isOpen && (
        <>
          {/* Backdrop */}
          <div
            className="fixed inset-0 z-40"
            onClick={() => setIsOpen(false)}
          />

          {/* Panel */}
          <div className="absolute right-0 top-full mt-2 w-96 max-h-[32rem] bg-card border border-border rounded-xl shadow-soft-lg z-50 overflow-hidden animate-scale-in">
            {/* Header */}
            <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-secondary/30">
              <div className="flex items-center gap-2">
                <Bell className="w-4 h-4 text-primary" />
                <span className="font-semibold text-foreground">Notifications</span>
                {unreadCount > 0 && (
                  <span className="px-1.5 py-0.5 text-xs font-medium bg-primary text-white rounded-full">
                    {unreadCount}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-1">
                {unreadCount > 0 && onMarkAllAsRead && (
                  <button
                    onClick={onMarkAllAsRead}
                    className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                    title="Mark all as read"
                  >
                    <CheckCheck className="w-4 h-4" />
                  </button>
                )}
                {notifications.length > 0 && onClearAll && (
                  <button
                    onClick={onClearAll}
                    className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                    title="Clear all"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                )}
              </div>
            </div>

            {/* Notifications List */}
            <div className="overflow-y-auto max-h-80">
              {visibleNotifications.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
                  <Bell className="w-10 h-10 mb-3 opacity-50" />
                  <p className="text-sm font-medium">No notifications</p>
                  <p className="text-xs mt-1">You're all caught up!</p>
                </div>
              ) : (
                <div className="divide-y divide-border">
                  {visibleNotifications.map((notification) => {
                    const Icon = typeIcons[notification.type];
                    const colors = typeColors[notification.type];

                    return (
                      <div
                        key={notification.id}
                        className={cn(
                          'relative px-4 py-3 hover:bg-secondary/30 transition-colors',
                          !notification.read && 'bg-primary/5'
                        )}
                      >
                        {/* Unread indicator */}
                        {!notification.read && (
                          <div className="absolute left-1.5 top-1/2 -translate-y-1/2 w-1.5 h-1.5 rounded-full bg-primary" />
                        )}

                        <div className="flex gap-3">
                          {/* Icon */}
                          <div
                            className={cn(
                              'flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center',
                              colors.bg
                            )}
                          >
                            <Icon className={cn('w-4 h-4', colors.text)} />
                          </div>

                          {/* Content */}
                          <div className="flex-1 min-w-0">
                            <div className="flex items-start justify-between gap-2">
                              <p
                                className={cn(
                                  'text-sm font-medium text-foreground',
                                  !notification.read && 'font-semibold'
                                )}
                              >
                                {notification.title}
                              </p>
                              <div className="flex items-center gap-1">
                                {!notification.read && onMarkAsRead && (
                                  <button
                                    onClick={() => onMarkAsRead(notification.id)}
                                    className="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                                    title="Mark as read"
                                  >
                                    <Check className="w-3.5 h-3.5" />
                                  </button>
                                )}
                                {onDelete && (
                                  <button
                                    onClick={() => onDelete(notification.id)}
                                    className="p-1 rounded text-muted-foreground hover:text-error hover:bg-error/10 transition-colors"
                                    title="Delete"
                                  >
                                    <X className="w-3.5 h-3.5" />
                                  </button>
                                )}
                              </div>
                            </div>

                            <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                              {notification.message}
                            </p>

                            <div className="flex items-center justify-between mt-2">
                              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                <Clock className="w-3 h-3" />
                                <span>{formatRelativeTime(notification.timestamp)}</span>
                                {notification.module && (
                                  <span
                                    className={cn(
                                      'px-1.5 py-0.5 rounded text-[10px] font-medium uppercase',
                                      moduleColors[notification.module]
                                    )}
                                  >
                                    {notification.module}
                                  </span>
                                )}
                              </div>

                              {notification.actionUrl && (
                                <button
                                  onClick={() => {
                                    onActionClick?.(notification);
                                    if (!notification.read && onMarkAsRead) {
                                      onMarkAsRead(notification.id);
                                    }
                                    setIsOpen(false);
                                    window.location.href = notification.actionUrl!;
                                  }}
                                  className="flex items-center gap-1 text-xs font-medium text-primary hover:text-primary/80 transition-colors"
                                >
                                  {notification.actionLabel || 'View'}
                                  <ExternalLink className="w-3 h-3" />
                                </button>
                              )}
                            </div>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Footer */}
            {notifications.length > maxVisible && (
              <div className="px-4 py-3 border-t border-border bg-secondary/30">
                <button
                  onClick={() => {
                    setIsOpen(false);
                    window.location.href = '/notifications';
                  }}
                  className="w-full text-sm font-medium text-primary hover:text-primary/80 transition-colors"
                >
                  View all {notifications.length} notifications
                </button>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// Notification item for standalone use
interface NotificationItemProps {
  notification: Notification;
  onMarkAsRead?: (id: string) => void;
  onDelete?: (id: string) => void;
  onActionClick?: (notification: Notification) => void;
  compact?: boolean;
}

export function NotificationItem({
  notification,
  onMarkAsRead,
  onDelete,
  onActionClick,
  compact = false,
}: NotificationItemProps) {
  const Icon = typeIcons[notification.type];
  const colors = typeColors[notification.type];

  return (
    <div
      className={cn(
        'relative flex gap-3 p-4 rounded-xl border transition-all',
        !notification.read && 'bg-primary/5',
        colors.border
      )}
    >
      <div
        className={cn(
          'flex-shrink-0 rounded-lg flex items-center justify-center',
          colors.bg,
          compact ? 'w-8 h-8' : 'w-10 h-10'
        )}
      >
        <Icon className={cn(colors.text, compact ? 'w-4 h-4' : 'w-5 h-5')} />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2">
          <div>
            <p className={cn('font-medium text-foreground', compact ? 'text-sm' : 'text-base')}>
              {notification.title}
            </p>
            <p className="text-sm text-muted-foreground mt-0.5">
              {notification.message}
            </p>
          </div>
          <div className="flex items-center gap-1 flex-shrink-0">
            {!notification.read && onMarkAsRead && (
              <button
                onClick={() => onMarkAsRead(notification.id)}
                className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
              >
                <Check className="w-4 h-4" />
              </button>
            )}
            {onDelete && (
              <button
                onClick={() => onDelete(notification.id)}
                className="p-1.5 rounded-md text-muted-foreground hover:text-error hover:bg-error/10 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>

        <div className="flex items-center justify-between mt-3">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Clock className="w-3.5 h-3.5" />
            <span>{formatRelativeTime(notification.timestamp)}</span>
            {notification.module && (
              <span
                className={cn(
                  'px-2 py-0.5 rounded text-[10px] font-medium uppercase bg-secondary',
                  moduleColors[notification.module]
                )}
              >
                {notification.module}
              </span>
            )}
          </div>

          {notification.actionUrl && (
            <button
              onClick={() => {
                onActionClick?.(notification);
                if (!notification.read && onMarkAsRead) {
                  onMarkAsRead(notification.id);
                }
                window.location.href = notification.actionUrl!;
              }}
              className="flex items-center gap-1 text-sm font-medium text-primary hover:text-primary/80 transition-colors"
            >
              {notification.actionLabel || 'View details'}
              <ExternalLink className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// Toast-style notification for temporary display
interface ToastNotificationProps {
  notification: Notification;
  onClose?: () => void;
  autoClose?: boolean;
  autoCloseDelay?: number;
}

export function ToastNotification({
  notification,
  onClose,
  autoClose = true,
  autoCloseDelay = 5000,
}: ToastNotificationProps) {
  useEffect(() => {
    if (autoClose && onClose) {
      const timer = setTimeout(onClose, autoCloseDelay);
      return () => clearTimeout(timer);
    }
  }, [autoClose, autoCloseDelay, onClose]);

  const Icon = typeIcons[notification.type];
  const colors = typeColors[notification.type];

  return (
    <div
      className={cn(
        'flex items-start gap-3 p-4 rounded-xl border bg-card shadow-soft-lg animate-slide-in-right',
        colors.border
      )}
    >
      <div className={cn('flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center', colors.bg)}>
        <Icon className={cn('w-4 h-4', colors.text)} />
      </div>

      <div className="flex-1 min-w-0">
        <p className="font-medium text-foreground text-sm">{notification.title}</p>
        <p className="text-xs text-muted-foreground mt-0.5">{notification.message}</p>
      </div>

      {onClose && (
        <button
          onClick={onClose}
          className="p-1 rounded text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
        >
          <X className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}
