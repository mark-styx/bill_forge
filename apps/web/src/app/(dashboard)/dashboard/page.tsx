'use client';

import { useQuery } from '@tanstack/react-query';
import { reportsApi, dashboardApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore } from '@/stores/theme';
import { useOrganizationTheme } from '@/components/organization-theme-provider';
import { AnimatedCounter } from '@/components/ui/animated-counter';
import { GlassCard, SpotlightCard } from '@/components/ui/glass-card';
import { GradientButton } from '@/components/ui/gradient-card';
import {
  FileText,
  Clock,
  CheckCircle,
  DollarSign,
  Users,
  AlertCircle,
  ArrowRight,
  ArrowUpRight,
  Plus,
  Upload,
  Eye,
  Activity,
  Zap,
  BarChart3,
  Target,
} from 'lucide-react';
import Link from 'next/link';

export default function DashboardPage() {
  const { hasModule, user } = useAuthStore();
  const { getCurrentColors } = useThemeStore();
  const { getBrandGradient } = useOrganizationTheme();

  const colors = getCurrentColors();
  const brandGradient = getBrandGradient();

  const { data: summary, isLoading, isError, error, refetch } = useQuery({
    queryKey: ['dashboard-summary'],
    queryFn: () => reportsApi.dashboardSummary(),
  });

  const { data: metrics, isLoading: metricsLoading, isError: metricsError } = useQuery({
    queryKey: ['dashboard-metrics'],
    queryFn: () => dashboardApi.metrics(),
  });

  // Stats based on enabled modules
  const allStats = [
    {
      name: 'Pending Review',
      value: isError ? '--' : (summary?.invoices_pending_review ?? 0),
      icon: Clock,
      colorKey: 'warning',
      href: '/invoices?status=pending',
      module: 'invoice_capture',
    },
    {
      name: 'Awaiting Approval',
      value: isError ? '--' : (summary?.invoices_pending_approval ?? 0),
      icon: AlertCircle,
      colorKey: 'primary',
      href: '/processing/approvals',
      module: 'invoice_processing',
    },
    {
      name: 'Ready for Payment',
      value: isError ? '--' : (summary?.invoices_ready_for_payment ?? 0),
      icon: CheckCircle,
      colorKey: 'success',
      href: '/processing/queues',
      module: 'invoice_processing',
    },
    {
      name: 'Active Vendors',
      value: isError ? '--' : (summary?.vendors_active ?? 0),
      icon: Users,
      colorKey: 'vendor',
      href: '/vendors',
      module: 'vendor_management',
    },
    {
      name: 'Total Pending',
      value: isError ? '$--' : `$${(summary?.total_amount_pending ?? 0).toLocaleString()}`,
      icon: DollarSign,
      colorKey: 'accent',
      href: '/reports',
      module: 'reporting',
    },
  ];

  const stats = allStats.filter((stat) => hasModule(stat.module));

  const getColorValue = (key: string) => {
    const colorMap: Record<string, string> = {
      primary: colors.primary,
      accent: colors.accent,
      capture: colors.capture,
      processing: colors.processing,
      vendor: colors.vendor,
      reporting: colors.reporting,
      success: '162 78% 42%',
      warning: '38 92% 50%',
      error: '0 84% 60%',
    };
    return colorMap[key] || colors.primary;
  };

  const welcomeMessage = () => {
    const modules = [];
    if (hasModule('invoice_capture')) modules.push('capture');
    if (hasModule('invoice_processing')) modules.push('processing');
    if (hasModule('vendor_management')) modules.push('vendors');

    if (modules.length === 0) return 'Welcome back!';
    if (modules.length === 1) return `Your ${modules[0]} overview is ready.`;
    return `Your ${modules.slice(0, -1).join(', ')} and ${modules.slice(-1)} overview.`;
  };

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">
            Welcome back, {user?.name?.split(' ')[0] || 'there'}
          </h1>
          <p className="text-muted-foreground mt-0.5">{welcomeMessage()}</p>
        </div>
      </div>

      {/* Error Banner */}
      {isError && (
        <div className="card border-destructive/50 bg-destructive/5 p-4 flex items-center gap-3">
          <AlertCircle className="w-5 h-5 text-destructive flex-shrink-0" />
          <div className="flex-1">
            <p className="text-sm font-medium text-destructive">Unable to load dashboard metrics</p>
            <p className="text-xs text-muted-foreground mt-0.5">{error instanceof Error ? error.message : 'Please try again'}</p>
          </div>
          <button onClick={() => refetch()} className="text-sm font-medium text-primary hover:underline">
            Retry
          </button>
        </div>
      )}

      {/* Stats Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {stats.map((stat, index) => {
          const colorValue = getColorValue(stat.colorKey);
          return (
            <Link
              key={stat.name}
              href={stat.href}
              className="bright-stat-card group animate-fade-in-up"
              style={{
                animationDelay: `${index * 50}ms`,
                '--stat-color': `hsl(${colorValue})`,
              } as React.CSSProperties}
            >
              <div
                className="absolute top-0 left-0 w-1 h-full rounded-l-2xl"
                style={{ background: `linear-gradient(180deg, hsl(${colorValue}), hsl(${colorValue} / 0.5))` }}
              />
              <div className="flex items-start justify-between">
                <div
                  className="p-2.5 rounded-xl"
                  style={{
                    background: `linear-gradient(135deg, hsl(${colorValue} / 0.1), hsl(${colorValue} / 0.05))`,
                  }}
                >
                  <stat.icon className="w-5 h-5" style={{ color: `hsl(${colorValue})` }} />
                </div>
                <ArrowUpRight className="w-4 h-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
              <div className="mt-3">
                <div className="text-2xl font-semibold text-foreground">
                  {isLoading ? (
                    <span className="inline-block w-12 h-7 bg-secondary animate-pulse rounded" />
                  ) : isError ? (
                    stat.value
                  ) : typeof stat.value === 'number' ? (
                    <AnimatedCounter value={stat.value} duration={800} />
                  ) : (
                    stat.value
                  )}
                </div>
                <p className="text-sm text-muted-foreground mt-0.5">{stat.name}</p>
              </div>
            </Link>
          );
        })}
      </div>

      {/* Quick Actions & Activity */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Main Actions */}
        <div className="lg:col-span-2 space-y-4">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
            <Zap className="w-4 h-4" />
            Quick Actions
          </h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {hasModule('invoice_capture') && (
              <>
                <Link
                  href="/invoices/upload"
                  className="card card-hover p-4 group flex items-center gap-4"
                >
                  <div className="bright-icon bright-icon-capture">
                    <Upload className="w-5 h-5" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Upload Invoice</p>
                    <p className="text-sm text-muted-foreground">Scan and process new</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-capture group-hover:translate-x-1 transition-all" />
                </Link>
                <Link
                  href="/invoices"
                  className="card card-hover p-4 group flex items-center gap-4"
                >
                  <div className="bright-icon bright-icon-capture">
                    <Eye className="w-5 h-5" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Review Invoices</p>
                    <p className="text-sm text-muted-foreground">View OCR results</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-capture group-hover:translate-x-1 transition-all" />
                </Link>
              </>
            )}
            {hasModule('invoice_processing') && (
              <>
                <Link
                  href="/processing/approvals"
                  className="card card-hover p-4 group flex items-center gap-4"
                >
                  <div className="bright-icon bright-icon-processing">
                    <CheckCircle className="w-5 h-5" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Pending Approvals</p>
                    <p className="text-sm text-muted-foreground">Review items</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
                </Link>
                <Link
                  href="/processing/queues"
                  className="card card-hover p-4 group flex items-center gap-4"
                >
                  <div className="bright-icon bright-icon-processing">
                    <FileText className="w-5 h-5" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Work Queues</p>
                    <p className="text-sm text-muted-foreground">Manage workflows</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
                </Link>
              </>
            )}
            {hasModule('vendor_management') && (
              <Link
                href="/vendors/new"
                className="card card-hover p-4 group flex items-center gap-4"
              >
                <div className="bright-icon bright-icon-vendor">
                  <Plus className="w-5 h-5" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Add Vendor</p>
                  <p className="text-sm text-muted-foreground">New vendor profile</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-vendor group-hover:translate-x-1 transition-all" />
              </Link>
            )}
            {hasModule('reporting') && (
              <Link
                href="/reports"
                className="card card-hover p-4 group flex items-center gap-4"
              >
                <div className="bright-icon bright-icon-reporting">
                  <BarChart3 className="w-5 h-5" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">View Reports</p>
                  <p className="text-sm text-muted-foreground">Analytics &amp; insights</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-reporting group-hover:translate-x-1 transition-all" />
              </Link>
            )}
          </div>
        </div>

        {/* Activity Feed */}
        <div className="space-y-4">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
            <Activity className="w-4 h-4" />
            Recent Activity
          </h2>
          <div className="card p-4">
            <div className="space-y-4">
              {[
                { action: 'AWS-2024-JAN approved', time: '2 min ago', icon: CheckCircle, colorKey: 'success' },
                { action: '6 invoices uploaded via OCR', time: '15 min ago', icon: Upload, colorKey: 'capture' },
                { action: 'MMA-Q1-2024 sent for approval', time: '1 hour ago', icon: AlertCircle, colorKey: 'warning' },
                { action: 'DevOps Solutions LLC added', time: '3 hours ago', icon: Users, colorKey: 'vendor' },
              ].map((activity, i) => {
                const colorValue = getColorValue(activity.colorKey);
                return (
                  <div key={i} className="flex items-start gap-3 animate-fade-in" style={{ animationDelay: `${i * 100}ms` }}>
                    <div
                      className="p-1.5 rounded-lg"
                      style={{ background: `hsl(${colorValue} / 0.1)` }}
                    >
                      <activity.icon className="w-3.5 h-3.5" style={{ color: `hsl(${colorValue})` }} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm text-foreground">{activity.action}</p>
                      <p className="text-xs text-muted-foreground">{activity.time}</p>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>

      {/* Insights - Rich Metrics */}
      {hasModule('reporting') && !metricsError && (
        <div className="space-y-4">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
            <BarChart3 className="w-4 h-4" />
            Insights
          </h2>
          {metricsLoading ? (
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="card p-5 space-y-3">
                  <span className="inline-block w-16 h-4 bg-secondary animate-pulse rounded" />
                  <span className="block w-12 h-7 bg-secondary animate-pulse rounded" />
                </div>
              ))}
            </div>
          ) : metrics && (
            <>
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                <div className="card p-5">
                  <p className="text-sm text-muted-foreground">Avg Processing Time</p>
                  <p className="text-2xl font-semibold text-foreground mt-1">
                    {metrics.invoices.avg_processing_time_hours.toFixed(1)}h
                  </p>
                </div>
                <div className="card p-5">
                  <p className="text-sm text-muted-foreground">Approval Rate</p>
                  <p className="text-2xl font-semibold text-foreground mt-1">
                    {metrics.approvals.approval_rate.toFixed(1)}%
                  </p>
                </div>
                <div className="card p-5">
                  <p className="text-sm text-muted-foreground">Overdue Approvals</p>
                  <p className="text-2xl font-semibold text-red-500 mt-1">
                    {metrics.approvals.overdue}
                  </p>
                </div>
                <div className="card p-5">
                  <p className="text-sm text-muted-foreground">Trend vs Last Month</p>
                  <p className="text-2xl font-semibold text-foreground mt-1">
                    {metrics.invoices.trend_vs_last_month > 0 ? '+' : ''}
                    {metrics.invoices.trend_vs_last_month.toFixed(1)}%
                  </p>
                </div>
              </div>
              {metrics.vendors.top_vendors.length > 0 && (
                <div className="card p-5">
                  <p className="text-sm font-semibold text-muted-foreground mb-3">Top Vendors</p>
                  <div className="space-y-2">
                    {metrics.vendors.top_vendors.slice(0, 5).map((vendor) => (
                      <div key={vendor.vendor_id} className="flex items-center justify-between text-sm">
                        <span className="text-foreground">{vendor.vendor_name}</span>
                        <div className="flex items-center gap-4 text-muted-foreground">
                          <span>{vendor.invoice_count} invoices</span>
                          <span>${(vendor.total_amount / 100).toLocaleString()}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      )}

      {/* Module Overview Cards */}
      {(hasModule('invoice_capture') || hasModule('invoice_processing') || hasModule('vendor_management')) && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {hasModule('invoice_capture') && (
            <GlassCard
              variant="subtle"
              hover="bright"
              padding="none"
              accentBar="top"
              accentColor="capture"
              className="group"
            >
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="bright-icon bright-icon-capture">
                    <FileText className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Invoice Capture</h3>
                    <p className="text-sm text-muted-foreground">OCR pipeline</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.invoices_pending_review ?? 0} duration={1000} />}
                    </div>
                    <p className="text-xs text-muted-foreground">Pending review</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.invoices_this_month ?? 0} duration={1200} />}
                    </div>
                    <p className="text-xs text-muted-foreground">This month</p>
                  </div>
                </div>
                <Link href="/invoices">
                  <GradientButton gradient="capture" size="sm" className="w-full">
                    View Invoices
                    <ArrowRight className="w-4 h-4 ml-1.5" />
                  </GradientButton>
                </Link>
              </div>
            </GlassCard>
          )}

          {hasModule('invoice_processing') && (
            <GlassCard
              variant="subtle"
              hover="bright"
              padding="none"
              accentBar="top"
              accentColor="processing"
              className="group"
            >
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="bright-icon bright-icon-processing">
                    <CheckCircle className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Processing</h3>
                    <p className="text-sm text-muted-foreground">Approvals &amp; workflows</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.invoices_pending_approval ?? 0} duration={1000} />}
                    </div>
                    <p className="text-xs text-muted-foreground">Awaiting</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.invoices_ready_for_payment ?? 0} duration={1000} />}
                    </div>
                    <p className="text-xs text-muted-foreground">Ready</p>
                  </div>
                </div>
                <Link href="/processing">
                  <GradientButton gradient="processing" size="sm" className="w-full">
                    View Processing
                    <ArrowRight className="w-4 h-4 ml-1.5" />
                  </GradientButton>
                </Link>
              </div>
            </GlassCard>
          )}

          {hasModule('vendor_management') && (
            <GlassCard
              variant="subtle"
              hover="bright"
              padding="none"
              accentBar="top"
              accentColor="vendor"
              className="group"
            >
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="bright-icon bright-icon-vendor">
                    <Users className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Vendors</h3>
                    <p className="text-sm text-muted-foreground">Supplier management</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.vendors_active ?? 0} duration={1000} />}
                    </div>
                    <p className="text-xs text-muted-foreground">Active</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      {isError ? '--' : <AnimatedCounter value={summary?.invoices_ready_for_payment ?? 0} duration={1200} />}
                    </div>
                    <p className="text-xs text-muted-foreground">Ready to pay</p>
                  </div>
                </div>
                <Link href="/vendors">
                  <GradientButton gradient="vendor" size="sm" className="w-full">
                    View Vendors
                    <ArrowRight className="w-4 h-4 ml-1.5" />
                  </GradientButton>
                </Link>
              </div>
            </GlassCard>
          )}
        </div>
      )}

      {/* Performance Banner */}
      {hasModule('reporting') && (
        <SpotlightCard
          variant="tinted"
          hover="glow"
          className="overflow-hidden"
        >
          <div className="flex flex-col sm:flex-row items-center gap-6">
            <div
              className="w-16 h-16 rounded-2xl flex items-center justify-center shadow-lg flex-shrink-0"
              style={{ background: brandGradient }}
            >
              <BarChart3 className="w-8 h-8 text-white" />
            </div>
            <div className="flex-1 text-center sm:text-left">
              <h3 className="text-lg font-semibold text-foreground">Performance Insights</h3>
              <p className="text-muted-foreground text-sm mt-1">
                Track your organization&apos;s invoice processing efficiency and vendor relationships
              </p>
            </div>
            <Link href="/reports">
              <GradientButton gradient="primary" size="default">
                <Target className="w-4 h-4 mr-2" />
                View Analytics
              </GradientButton>
            </Link>
          </div>
        </SpotlightCard>
      )}
    </div>
  );
}
