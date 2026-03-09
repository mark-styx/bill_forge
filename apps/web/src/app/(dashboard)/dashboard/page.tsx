'use client';

import { useQuery } from '@tanstack/react-query';
import { reportsApi } from '@/lib/api';
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
  TrendingUp,
  AlertCircle,
  ArrowRight,
  ArrowUpRight,
  Plus,
  Upload,
  Eye,
  Sparkles,
  Activity,
  Zap,
  BarChart3,
  Target,
} from 'lucide-react';
import Link from 'next/link';

export default function DashboardPage() {
  const { hasModule, currentPersona, user, tenant } = useAuthStore();
  const { getCurrentColors } = useThemeStore();
  const { getBrandGradient } = useOrganizationTheme();

  const colors = getCurrentColors();
  const brandGradient = getBrandGradient();

  const { data: summary, isLoading } = useQuery({
    queryKey: ['dashboard-summary'],
    queryFn: () => reportsApi.dashboardSummary(),
  });

  // Stats based on enabled modules
  const allStats = [
    {
      name: 'Pending Review',
      value: summary?.invoices_pending_review ?? 0,
      icon: Clock,
      colorKey: 'warning',
      href: '/invoices?status=pending',
      module: 'invoice_capture',
      trend: '+12%',
      trendUp: true,
    },
    {
      name: 'Awaiting Approval',
      value: summary?.invoices_pending_approval ?? 0,
      icon: AlertCircle,
      colorKey: 'primary',
      href: '/processing/approvals',
      module: 'invoice_processing',
    },
    {
      name: 'Ready for Payment',
      value: summary?.invoices_ready_for_payment ?? 0,
      icon: CheckCircle,
      colorKey: 'success',
      href: '/processing/queues',
      module: 'invoice_processing',
    },
    {
      name: 'Active Vendors',
      value: summary?.vendors_active ?? 0,
      icon: Users,
      colorKey: 'vendor',
      href: '/vendors',
      module: 'vendor_management',
    },
    {
      name: 'Total Pending',
      value: `$${(summary?.total_amount_pending ?? 0).toLocaleString()}`,
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
          <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
            Welcome back, {user?.name?.split(' ')[0] || 'there'}
            <Sparkles className="w-5 h-5 text-warning" />
          </h1>
          <p className="text-muted-foreground mt-0.5">{welcomeMessage()}</p>
        </div>
        {currentPersona && (
          <div
            className="flex items-center gap-2 px-3 py-1.5 rounded-xl border"
            style={{
              background: `linear-gradient(135deg, hsl(${colors.primary} / 0.05), hsl(${colors.accent} / 0.05))`,
              borderColor: `hsl(${colors.primary} / 0.2)`,
            }}
          >
            <div
              className="w-2 h-2 rounded-full animate-pulse"
              style={{ background: `hsl(${colors.primary})` }}
            />
            <span className="text-sm font-medium" style={{ color: `hsl(${colors.primary})` }}>
              {currentPersona.name}
            </span>
          </div>
        )}
      </div>

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
                  ) : typeof stat.value === 'number' ? (
                    <AnimatedCounter value={stat.value} duration={800} />
                  ) : (
                    stat.value
                  )}
                </div>
                <p className="text-sm text-muted-foreground mt-0.5">{stat.name}</p>
              </div>
              {stat.trend && (
                <div
                  className="mt-2 flex items-center gap-1 text-xs font-medium"
                  style={{ color: stat.trendUp ? 'hsl(162 78% 42%)' : 'hsl(0 84% 60%)' }}
                >
                  <TrendingUp className={`w-3 h-3 ${stat.trendUp ? '' : 'rotate-180'}`} />
                  {stat.trend} from last week
                </div>
              )}
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
                  <TrendingUp className="w-5 h-5" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">View Reports</p>
                  <p className="text-sm text-muted-foreground">Analytics & insights</p>
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
                { action: 'Invoice #1247 approved', time: '2 min ago', icon: CheckCircle, colorKey: 'success' },
                { action: 'New vendor: Acme Corp', time: '15 min ago', icon: Users, colorKey: 'vendor' },
                { action: 'Invoice #1246 uploaded', time: '1 hour ago', icon: Upload, colorKey: 'capture' },
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
            <Link
              href="/activity"
              className="mt-4 flex items-center justify-center gap-1 text-sm font-medium transition-colors"
              style={{ color: `hsl(${colors.primary})` }}
            >
              View all activity
              <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
        </div>
      </div>

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
                      <AnimatedCounter value={summary?.invoices_pending_review ?? 0} duration={1000} />
                    </div>
                    <p className="text-xs text-muted-foreground">Pending</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      <AnimatedCounter value={94} suffix="%" duration={1200} />
                    </div>
                    <p className="text-xs text-muted-foreground">Accuracy</p>
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
                    <p className="text-sm text-muted-foreground">Approvals & workflows</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      <AnimatedCounter value={summary?.invoices_pending_approval ?? 0} duration={1000} />
                    </div>
                    <p className="text-xs text-muted-foreground">Awaiting</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      <AnimatedCounter value={summary?.invoices_ready_for_payment ?? 0} duration={1000} />
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
                      <AnimatedCounter value={summary?.vendors_active ?? 0} duration={1000} />
                    </div>
                    <p className="text-xs text-muted-foreground">Active</p>
                  </div>
                  <div>
                    <div className="text-2xl font-semibold text-foreground">
                      <AnimatedCounter value={85} suffix="%" duration={1200} />
                    </div>
                    <p className="text-xs text-muted-foreground">W-9 on file</p>
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
