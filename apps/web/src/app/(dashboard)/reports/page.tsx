'use client';

import { useQuery } from '@tanstack/react-query';
import { reportsApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import {
  BarChart3,
  TrendingUp,
  TrendingDown,
  DollarSign,
  Clock,
  Download,
  Calendar,
  Users,
  FileText,
  ScanLine,
  CheckCircle,
  ArrowRight,
  Filter,
} from 'lucide-react';

export default function ReportsPage() {
  const { hasModule, currentPersona } = useAuthStore();
  
  const { data: metrics } = useQuery({
    queryKey: ['workflow-metrics'],
    queryFn: () => reportsApi.workflowMetrics(),
    enabled: hasModule('invoice_processing'),
  });

  const { data: aging } = useQuery({
    queryKey: ['invoice-aging'],
    queryFn: () => reportsApi.invoiceAging(),
    enabled: hasModule('invoice_processing'),
  });

  const { data: summary } = useQuery({
    queryKey: ['dashboard-summary'],
    queryFn: () => reportsApi.dashboardSummary(),
  });

  const showOcrMetrics = hasModule('invoice_capture');
  const showProcessingMetrics = hasModule('invoice_processing');
  const showVendorMetrics = hasModule('vendor_management');

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Reports & Analytics</h1>
          <p className="text-muted-foreground mt-0.5">
            Insights and performance metrics
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button className="btn btn-secondary btn-sm">
            <Filter className="w-4 h-4 mr-1.5" />
            Filter
          </button>
          <button className="btn btn-primary btn-sm">
            <Download className="w-4 h-4 mr-1.5" />
            Export
          </button>
        </div>
      </div>

      {/* Key Metrics */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {showVendorMetrics && (
          <div className="stat-card">
            <div className="flex items-center justify-between">
              <div className="p-2 rounded-lg bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
            </div>
            <p className="stat-value mt-3">{summary?.vendors_active ?? '—'}</p>
            <p className="stat-label">Active Vendors</p>
          </div>
        )}

        {(showOcrMetrics || showProcessingMetrics) && (
          <>
            <div className="stat-card">
              <div className="flex items-center justify-between">
                <div className="p-2 rounded-lg bg-capture/10">
                  <FileText className="w-5 h-5 text-capture" />
                </div>
                <span className="text-xs text-success flex items-center gap-0.5">
                  <TrendingUp className="w-3 h-3" /> 12%
                </span>
              </div>
              <p className="stat-value mt-3">{summary?.invoices_this_month ?? '—'}</p>
              <p className="stat-label">This Month</p>
            </div>

            <div className="stat-card">
              <div className="flex items-center justify-between">
                <div className="p-2 rounded-lg bg-accent/10">
                  <DollarSign className="w-5 h-5 text-accent" />
                </div>
              </div>
              <p className="stat-value mt-3">${(summary?.total_amount_pending ?? 0).toLocaleString()}</p>
              <p className="stat-label">Total Pending</p>
            </div>
          </>
        )}

        {showProcessingMetrics && (
          <div className="stat-card">
            <div className="flex items-center justify-between">
              <div className="p-2 rounded-lg bg-processing/10">
                <Clock className="w-5 h-5 text-processing" />
              </div>
            </div>
            <p className="stat-value mt-3">{metrics?.avg_processing_time_hours?.toFixed(1) ?? '—'}h</p>
            <p className="stat-label">Avg Processing</p>
          </div>
        )}
      </div>

      {/* OCR Metrics */}
      {showOcrMetrics && (
        <div className="card overflow-hidden">
          <div className="h-1 bg-gradient-to-r from-capture to-capture/50" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-6">
              <div className="p-2 rounded-lg bg-capture/10">
                <ScanLine className="w-5 h-5 text-capture" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">OCR & Capture Performance</h2>
                <p className="text-sm text-muted-foreground">Invoice processing pipeline metrics</p>
              </div>
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              {[
                { label: 'OCR Accuracy', value: '94%', trend: '+2%', up: true },
                { label: 'Avg Process Time', value: '2.3s', trend: '-0.5s', up: true },
                { label: 'Pending Review', value: summary?.invoices_pending_review ?? 0, trend: null },
                { label: 'Error Rate', value: '3%', trend: '-1%', up: true },
              ].map((metric) => (
                <div key={metric.label} className="p-4 bg-secondary/50 rounded-xl">
                  <p className="text-2xl font-semibold text-foreground">{metric.value}</p>
                  <div className="flex items-center justify-between mt-1">
                    <p className="text-sm text-muted-foreground">{metric.label}</p>
                    {metric.trend && (
                      <span className={`text-xs flex items-center gap-0.5 ${metric.up ? 'text-success' : 'text-error'}`}>
                        {metric.up ? <TrendingUp className="w-3 h-3" /> : <TrendingDown className="w-3 h-3" />}
                        {metric.trend}
                      </span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Processing Metrics */}
      {showProcessingMetrics && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Invoice Aging */}
          <div className="card p-6">
            <h2 className="font-semibold text-foreground mb-4">Invoice Aging</h2>
            {aging && aging.length > 0 ? (
              <div className="space-y-3">
                {aging.map((bucket) => {
                  const total = aging.reduce((sum, b) => sum + b.count, 0) || 1;
                  const percentage = (bucket.count / total) * 100;
                  return (
                    <div key={bucket.bucket}>
                      <div className="flex items-center justify-between mb-1.5">
                        <span className="text-sm text-foreground">{bucket.bucket}</span>
                        <span className="text-sm font-medium text-foreground">
                          {bucket.count} · ${bucket.total_amount.toLocaleString()}
                        </span>
                      </div>
                      <div className="h-2 bg-secondary rounded-full overflow-hidden">
                        <div
                          className="h-full bg-gradient-to-r from-primary to-accent rounded-full transition-all duration-500"
                          style={{ width: `${Math.min(percentage, 100)}%` }}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                No aging data available
              </div>
            )}
          </div>

          {/* Queue Performance */}
          <div className="card p-6">
            <h2 className="font-semibold text-foreground mb-4">Queue Performance</h2>
            <div className="space-y-3">
              {[
                { name: 'Accounts Payable', count: summary?.invoices_pending_review ?? 0, color: 'bg-blue-500' },
                { name: 'Pending Approval', count: summary?.invoices_pending_approval ?? 0, color: 'bg-warning' },
                { name: 'Ready for Payment', count: summary?.invoices_ready_for_payment ?? 0, color: 'bg-success' },
              ].map((queue) => (
                <div key={queue.name} className="flex items-center justify-between p-3 bg-secondary/50 rounded-lg">
                  <div className="flex items-center gap-3">
                    <div className={`w-2.5 h-2.5 rounded-full ${queue.color}`} />
                    <span className="text-sm font-medium text-foreground">{queue.name}</span>
                  </div>
                  <span className="text-sm text-muted-foreground">{queue.count} items</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Vendor Analytics */}
      {showVendorMetrics && (
        <div className="card overflow-hidden">
          <div className="h-1 bg-gradient-to-r from-vendor to-vendor/50" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-6">
              <div className="p-2 rounded-lg bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">Vendor Analytics</h2>
                <p className="text-sm text-muted-foreground">Supplier performance and compliance</p>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {[
                { label: 'Active Vendors', value: summary?.vendors_active ?? 0 },
                { label: 'W-9 On File', value: '85%' },
                { label: 'New This Month', value: 12 },
              ].map((metric) => (
                <div key={metric.label} className="p-4 bg-secondary/50 rounded-xl text-center">
                  <p className="text-2xl font-semibold text-foreground">{metric.value}</p>
                  <p className="text-sm text-muted-foreground mt-1">{metric.label}</p>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Quick Reports */}
      <div className="card p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="font-semibold text-foreground">Available Reports</h2>
          <button className="text-sm text-primary hover:underline">View all</button>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {showVendorMetrics && (
            <button className="action-card group text-left">
              <div className="action-card-icon bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <div className="flex-1">
                <p className="font-medium text-foreground">Vendor Spend</p>
                <p className="text-sm text-muted-foreground">Analysis by vendor</p>
              </div>
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
            </button>
          )}
          
          {showProcessingMetrics && (
            <>
              <button className="action-card group text-left">
                <div className="action-card-icon bg-processing/10">
                  <BarChart3 className="w-5 h-5 text-processing" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Invoice Volume</p>
                  <p className="text-sm text-muted-foreground">Monthly trends</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
              </button>
              
              <button className="action-card group text-left">
                <div className="action-card-icon bg-processing/10">
                  <Calendar className="w-5 h-5 text-processing" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Payment Schedule</p>
                  <p className="text-sm text-muted-foreground">Upcoming payments</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
              </button>
            </>
          )}
          
          {showOcrMetrics && (
            <button className="action-card group text-left">
              <div className="action-card-icon bg-capture/10">
                <ScanLine className="w-5 h-5 text-capture" />
              </div>
              <div className="flex-1">
                <p className="font-medium text-foreground">OCR Performance</p>
                <p className="text-sm text-muted-foreground">Accuracy metrics</p>
              </div>
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
            </button>
          )}
          
          <button className="action-card group text-left">
            <div className="action-card-icon bg-secondary">
              <DollarSign className="w-5 h-5 text-muted-foreground" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Monthly Summary</p>
              <p className="text-sm text-muted-foreground">Financial overview</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
          </button>
        </div>
      </div>
    </div>
  );
}
