'use client';

import { useQuery } from '@tanstack/react-query';
import { reportsApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
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
} from 'lucide-react';
import Link from 'next/link';

export default function DashboardPage() {
  const { hasModule, currentPersona, user } = useAuthStore();

  const { data: summary, isLoading } = useQuery({
    queryKey: ['dashboard-summary'],
    queryFn: () => reportsApi.dashboardSummary(),
  });

  // Build stats based on enabled modules
  const allStats = [
    {
      name: 'Pending Review',
      value: summary?.invoices_pending_review ?? 0,
      icon: Clock,
      color: 'text-warning',
      bgColor: 'bg-warning/10',
      href: '/invoices?status=pending',
      module: 'invoice_capture',
      trend: '+12%',
      trendUp: true,
    },
    {
      name: 'Awaiting Approval',
      value: summary?.invoices_pending_approval ?? 0,
      icon: AlertCircle,
      color: 'text-primary',
      bgColor: 'bg-primary/10',
      href: '/processing/approvals',
      module: 'invoice_processing',
    },
    {
      name: 'Ready for Payment',
      value: summary?.invoices_ready_for_payment ?? 0,
      icon: CheckCircle,
      color: 'text-success',
      bgColor: 'bg-success/10',
      href: '/processing/queues',
      module: 'invoice_processing',
    },
    {
      name: 'Active Vendors',
      value: summary?.vendors_active ?? 0,
      icon: Users,
      color: 'text-vendor',
      bgColor: 'bg-vendor/10',
      href: '/vendors',
      module: 'vendor_management',
    },
    {
      name: 'Total Pending',
      value: `$${(summary?.total_amount_pending ?? 0).toLocaleString()}`,
      icon: DollarSign,
      color: 'text-accent',
      bgColor: 'bg-accent/10',
      href: '/reports',
      module: 'reporting',
    },
  ];

  const stats = allStats.filter((stat) => hasModule(stat.module));

  const welcomeMessage = () => {
    const modules = [];
    if (hasModule('invoice_capture')) modules.push('invoice capture');
    if (hasModule('invoice_processing')) modules.push('processing');
    if (hasModule('vendor_management')) modules.push('vendors');
    
    if (modules.length === 0) return 'Welcome back!';
    if (modules.length === 1) return `Here's what's happening with your ${modules[0]}.`;
    return `Here's what's happening with your ${modules.slice(0, -1).join(', ')} and ${modules.slice(-1)}.`;
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
        {currentPersona && (
          <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/5 border border-primary/20 rounded-lg">
            <div className="w-2 h-2 rounded-full bg-primary animate-pulse-soft" />
            <span className="text-sm font-medium text-primary">{currentPersona.name}</span>
          </div>
        )}
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {stats.map((stat, index) => (
          <Link
            key={stat.name}
            href={stat.href}
            className="card card-hover p-4 group animate-slide-up"
            style={{ animationDelay: `${index * 50}ms` }}
          >
            <div className="flex items-start justify-between">
              <div className={`p-2 rounded-lg ${stat.bgColor}`}>
                <stat.icon className={`w-5 h-5 ${stat.color}`} />
              </div>
              <ArrowUpRight className="w-4 h-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <div className="mt-3">
              <p className="text-2xl font-semibold text-foreground">
                {isLoading ? '—' : stat.value}
              </p>
              <p className="text-sm text-muted-foreground mt-0.5">{stat.name}</p>
            </div>
            {stat.trend && (
              <div className={`mt-2 flex items-center gap-1 text-xs font-medium ${stat.trendUp ? 'text-success' : 'text-error'}`}>
                <TrendingUp className={`w-3 h-3 ${stat.trendUp ? '' : 'rotate-180'}`} />
                {stat.trend} from last week
              </div>
            )}
          </Link>
        ))}
      </div>

      {/* Quick Actions */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Main Actions */}
        <div className="lg:col-span-2 space-y-4">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            Quick Actions
          </h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {hasModule('invoice_capture') && (
              <>
                <Link href="/invoices/upload" className="action-card group">
                  <div className="action-card-icon bg-capture/10">
                    <Upload className="w-5 h-5 text-capture" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Upload Invoice</p>
                    <p className="text-sm text-muted-foreground">Scan and process new invoices</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
                </Link>
                <Link href="/invoices" className="action-card group">
                  <div className="action-card-icon bg-capture/10">
                    <Eye className="w-5 h-5 text-capture" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Review Invoices</p>
                    <p className="text-sm text-muted-foreground">View and correct OCR results</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
                </Link>
              </>
            )}
            {hasModule('invoice_processing') && (
              <>
                <Link href="/processing/approvals" className="action-card group">
                  <div className="action-card-icon bg-processing/10">
                    <CheckCircle className="w-5 h-5 text-processing" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Pending Approvals</p>
                    <p className="text-sm text-muted-foreground">Review items needing approval</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
                </Link>
                <Link href="/processing/queues" className="action-card group">
                  <div className="action-card-icon bg-processing/10">
                    <FileText className="w-5 h-5 text-processing" />
                  </div>
                  <div className="flex-1">
                    <p className="font-medium text-foreground">Work Queues</p>
                    <p className="text-sm text-muted-foreground">Manage processing workflows</p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
                </Link>
              </>
            )}
            {hasModule('vendor_management') && (
              <Link href="/vendors/new" className="action-card group">
                <div className="action-card-icon bg-vendor/10">
                  <Plus className="w-5 h-5 text-vendor" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Add Vendor</p>
                  <p className="text-sm text-muted-foreground">Create a new vendor profile</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
              </Link>
            )}
            {hasModule('reporting') && (
              <Link href="/reports" className="action-card group">
                <div className="action-card-icon bg-reporting/10">
                  <TrendingUp className="w-5 h-5 text-reporting" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">View Reports</p>
                  <p className="text-sm text-muted-foreground">Analytics and insights</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
              </Link>
            )}
          </div>
        </div>

        {/* Activity Feed */}
        <div className="space-y-4">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            Recent Activity
          </h2>
          <div className="card p-4">
            <div className="space-y-4">
              {[
                { action: 'Invoice #1247 approved', time: '2 min ago', icon: CheckCircle, color: 'text-success' },
                { action: 'New vendor added: Acme Corp', time: '15 min ago', icon: Users, color: 'text-vendor' },
                { action: 'Invoice #1246 uploaded', time: '1 hour ago', icon: Upload, color: 'text-capture' },
              ].map((activity, i) => (
                <div key={i} className="flex items-start gap-3">
                  <div className={`p-1.5 rounded-md bg-secondary ${activity.color}`}>
                    <activity.icon className="w-3.5 h-3.5" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-foreground">{activity.action}</p>
                    <p className="text-xs text-muted-foreground">{activity.time}</p>
                  </div>
                </div>
              ))}
            </div>
            <Link 
              href="/activity" 
              className="mt-4 flex items-center justify-center gap-1 text-sm text-primary hover:text-primary/80 transition-colors"
            >
              View all activity
              <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
        </div>
      </div>

      {/* Module Cards */}
      {(hasModule('invoice_capture') || hasModule('invoice_processing') || hasModule('vendor_management')) && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {hasModule('invoice_capture') && (
            <div className="card overflow-hidden">
              <div className="h-1 bg-gradient-to-r from-capture to-capture/50" />
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2 rounded-lg bg-capture/10">
                    <FileText className="w-5 h-5 text-capture" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Invoice Capture</h3>
                    <p className="text-sm text-muted-foreground">OCR processing pipeline</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <p className="text-2xl font-semibold text-foreground">{summary?.invoices_pending_review ?? 0}</p>
                    <p className="text-xs text-muted-foreground">Pending review</p>
                  </div>
                  <div>
                    <p className="text-2xl font-semibold text-foreground">94%</p>
                    <p className="text-xs text-muted-foreground">OCR accuracy</p>
                  </div>
                </div>
                <Link
                  href="/invoices"
                  className="btn btn-secondary btn-sm w-full"
                >
                  View Invoices
                </Link>
              </div>
            </div>
          )}

          {hasModule('invoice_processing') && (
            <div className="card overflow-hidden">
              <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2 rounded-lg bg-processing/10">
                    <CheckCircle className="w-5 h-5 text-processing" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Processing</h3>
                    <p className="text-sm text-muted-foreground">Approvals & workflows</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <p className="text-2xl font-semibold text-foreground">{summary?.invoices_pending_approval ?? 0}</p>
                    <p className="text-xs text-muted-foreground">Awaiting approval</p>
                  </div>
                  <div>
                    <p className="text-2xl font-semibold text-foreground">{summary?.invoices_ready_for_payment ?? 0}</p>
                    <p className="text-xs text-muted-foreground">Ready for payment</p>
                  </div>
                </div>
                <Link
                  href="/processing"
                  className="btn btn-secondary btn-sm w-full"
                >
                  View Processing
                </Link>
              </div>
            </div>
          )}

          {hasModule('vendor_management') && (
            <div className="card overflow-hidden">
              <div className="h-1 bg-gradient-to-r from-vendor to-vendor/50" />
              <div className="p-5">
                <div className="flex items-center gap-3 mb-4">
                  <div className="p-2 rounded-lg bg-vendor/10">
                    <Users className="w-5 h-5 text-vendor" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">Vendors</h3>
                    <p className="text-sm text-muted-foreground">Supplier management</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <p className="text-2xl font-semibold text-foreground">{summary?.vendors_active ?? 0}</p>
                    <p className="text-xs text-muted-foreground">Active vendors</p>
                  </div>
                  <div>
                    <p className="text-2xl font-semibold text-foreground">85%</p>
                    <p className="text-xs text-muted-foreground">W-9 on file</p>
                  </div>
                </div>
                <Link
                  href="/vendors"
                  className="btn btn-secondary btn-sm w-full"
                >
                  View Vendors
                </Link>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
