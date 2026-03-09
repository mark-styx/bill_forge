'use client';

import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { reportsApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore } from '@/stores/theme';
import {
  ChartContainer,
  BillForgeAreaChart,
  BillForgeBarChart,
  BillForgeLineChart,
  BillForgeDonutChart,
  BillForgeProgressChart,
  BillForgeSparkline,
} from '@/components/ui/charts';
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
  Activity,
  Target,
  Zap,
  RefreshCw,
} from 'lucide-react';

const monthlyData = [
  { name: 'Jan', invoices: 145, amount: 42500 },
  { name: 'Feb', invoices: 162, amount: 48200 },
  { name: 'Mar', invoices: 189, amount: 52800 },
  { name: 'Apr', invoices: 175, amount: 49100 },
  { name: 'May', invoices: 198, amount: 58300 },
  { name: 'Jun', invoices: 215, amount: 64200 },
  { name: 'Jul', invoices: 228, amount: 68500 },
  { name: 'Aug', invoices: 242, amount: 72100 },
  { name: 'Sep', invoices: 235, amount: 69800 },
  { name: 'Oct', invoices: 258, amount: 76400 },
  { name: 'Nov', invoices: 272, amount: 81200 },
  { name: 'Dec', invoices: 285, amount: 85600 },
];

const processingTimeData = [
  { name: 'Mon', time: 2.4 },
  { name: 'Tue', time: 2.1 },
  { name: 'Wed', time: 2.8 },
  { name: 'Thu', time: 1.9 },
  { name: 'Fri', time: 2.2 },
  { name: 'Sat', time: 1.5 },
  { name: 'Sun', time: 1.2 },
];

const statusDistribution = [
  { name: 'Approved', value: 45 },
  { name: 'Pending Review', value: 25 },
  { name: 'Processing', value: 18 },
  { name: 'Rejected', value: 7 },
  { name: 'On Hold', value: 5 },
];

const vendorSpendData = [
  { name: 'Acme Corp', spend: 125000 },
  { name: 'TechSupply Inc', spend: 98500 },
  { name: 'Global Parts', spend: 87200 },
  { name: 'Office Depot', spend: 65400 },
  { name: 'CloudServe', spend: 52800 },
];

const weeklyTrend = [12, 15, 18, 14, 22, 19, 25];

export default function ReportsPage() {
  const { hasModule } = useAuthStore();
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

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

  const formatCurrency = (value: number) => `$${(value / 1000).toFixed(0)}k`;

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
            <BarChart3 className="w-6 h-6 text-reporting" />
            Reports & Analytics
          </h1>
          <p className="text-muted-foreground mt-0.5">
            Real-time insights and performance metrics
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button className="btn btn-secondary btn-sm">
            <RefreshCw className="w-4 h-4 mr-1.5" />
            Refresh
          </button>
          <button className="btn btn-secondary btn-sm">
            <Filter className="w-4 h-4 mr-1.5" />
            Filter
          </button>
          <Link href="/reports/export" className="btn btn-primary btn-sm">
            <Download className="w-4 h-4 mr-1.5" />
            Export
          </Link>
        </div>
      </div>

      {/* Key Metrics - Modern Cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {showVendorMetrics && (
          <div className="bright-stat-card group animate-fade-in-up">
            <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-vendor to-vendor/50" />
            <div className="flex items-center justify-between mb-3">
              <div className="p-2.5 rounded-xl bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <BillForgeSparkline data={weeklyTrend} color="vendor" showArea />
            </div>
            <p className="text-2xl font-bold text-foreground">{summary?.vendors_active ?? '—'}</p>
            <p className="text-sm text-muted-foreground">Active Vendors</p>
          </div>
        )}

        {(showOcrMetrics || showProcessingMetrics) && (
          <>
            <div className="bright-stat-card group animate-fade-in-up" style={{ animationDelay: '50ms' }}>
              <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-capture to-capture/50" />
              <div className="flex items-center justify-between mb-3">
                <div className="p-2.5 rounded-xl bg-capture/10">
                  <FileText className="w-5 h-5 text-capture" />
                </div>
                <div className="flex items-center gap-1 text-xs font-medium text-success">
                  <TrendingUp className="w-3 h-3" />
                  12%
                </div>
              </div>
              <p className="text-2xl font-bold text-foreground">{summary?.invoices_this_month ?? '—'}</p>
              <p className="text-sm text-muted-foreground">This Month</p>
            </div>

            <div className="bright-stat-card group animate-fade-in-up" style={{ animationDelay: '100ms' }}>
              <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-accent to-accent/50" />
              <div className="flex items-center justify-between mb-3">
                <div className="p-2.5 rounded-xl bg-accent/10">
                  <DollarSign className="w-5 h-5 text-accent" />
                </div>
                <BillForgeSparkline data={[28, 32, 45, 38, 52, 48, 65]} color="accent" showArea />
              </div>
              <p className="text-2xl font-bold text-foreground">${(summary?.total_amount_pending ?? 0).toLocaleString()}</p>
              <p className="text-sm text-muted-foreground">Total Pending</p>
            </div>
          </>
        )}

        {showProcessingMetrics && (
          <div className="bright-stat-card group animate-fade-in-up" style={{ animationDelay: '150ms' }}>
            <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-processing to-processing/50" />
            <div className="flex items-center justify-between mb-3">
              <div className="p-2.5 rounded-xl bg-processing/10">
                <Clock className="w-5 h-5 text-processing" />
              </div>
              <div className="flex items-center gap-1 text-xs font-medium text-success">
                <TrendingDown className="w-3 h-3" />
                -15%
              </div>
            </div>
            <p className="text-2xl font-bold text-foreground">{metrics?.avg_processing_time_hours?.toFixed(1) ?? '—'}h</p>
            <p className="text-sm text-muted-foreground">Avg Processing</p>
          </div>
        )}
      </div>

      {/* Main Charts Row */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Invoice Volume Chart */}
        <div className="lg:col-span-2">
          <ChartContainer
            title="Invoice Volume Trend"
            description="Monthly invoice count over the past year"
            action={
              <select className="text-sm bg-secondary border-0 rounded-lg px-2 py-1 text-muted-foreground focus:ring-2 focus:ring-primary">
                <option>Last 12 months</option>
                <option>Last 6 months</option>
                <option>Last 30 days</option>
              </select>
            }
          >
            <BillForgeAreaChart
              data={monthlyData}
              dataKey="invoices"
              color="primary"
              formatter={(v) => v.toString()}
            />
          </ChartContainer>
        </div>

        {/* Status Distribution */}
        <ChartContainer
          title="Invoice Status"
          description="Current status distribution"
        >
          <BillForgeDonutChart
            data={statusDistribution}
            centerValue={statusDistribution.reduce((a, b) => a + b.value, 0)}
            centerText="Total"
            innerRadius={55}
            outerRadius={85}
            height={250}
          />
          <div className="mt-4 space-y-2">
            {statusDistribution.slice(0, 3).map((item, index) => {
              const colorClasses = ['bg-primary', 'bg-capture', 'bg-processing', 'bg-vendor', 'bg-reporting'];
              return (
                <div key={item.name} className="flex items-center justify-between text-sm">
                  <div className="flex items-center gap-2">
                    <div className={`w-2.5 h-2.5 rounded-full ${colorClasses[index]}`} />
                    <span className="text-muted-foreground">{item.name}</span>
                  </div>
                  <span className="font-medium text-foreground">{item.value}%</span>
                </div>
              );
            })}
          </div>
        </ChartContainer>
      </div>

      {/* OCR Metrics - Enhanced */}
      {showOcrMetrics && (
        <div className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-capture via-capture/70 to-transparent" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-6">
              <div className="p-2.5 rounded-xl bg-capture/10">
                <ScanLine className="w-5 h-5 text-capture" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">OCR & Capture Performance</h2>
                <p className="text-sm text-muted-foreground">Invoice processing pipeline metrics</p>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              <div className="text-center">
                <BillForgeProgressChart
                  value={94}
                  color="capture"
                  label="OCR Accuracy"
                  height={140}
                />
              </div>
              {[
                { label: 'Avg Process Time', value: '2.3s', trend: '-0.5s', up: true, icon: Zap },
                { label: 'Pending Review', value: summary?.invoices_pending_review ?? 0, trend: null, icon: Clock },
                { label: 'Error Rate', value: '3%', trend: '-1%', up: true, icon: Target },
              ].map((metric) => (
                <div key={metric.label} className="p-4 bg-secondary/50 rounded-xl flex flex-col justify-center">
                  <div className="flex items-center gap-2 mb-2">
                    <metric.icon className="w-4 h-4 text-capture" />
                    <span className="text-sm text-muted-foreground">{metric.label}</span>
                  </div>
                  <p className="text-2xl font-bold text-foreground">{metric.value}</p>
                  {metric.trend && (
                    <span className={`text-xs flex items-center gap-0.5 mt-1 ${metric.up ? 'text-success' : 'text-error'}`}>
                      {metric.up ? <TrendingUp className="w-3 h-3" /> : <TrendingDown className="w-3 h-3" />}
                      {metric.trend}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Processing Metrics Row */}
      {showProcessingMetrics && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Invoice Aging with Bars */}
          <ChartContainer
            title="Invoice Aging"
            description="Distribution by days outstanding"
          >
            <BillForgeBarChart
              data={aging?.map((bucket: any) => ({
                name: bucket.bucket,
                invoices: bucket.count,
                amount: bucket.total_amount,
              })) || [
                { name: '0-30 days', invoices: 45, amount: 125000 },
                { name: '31-60 days', invoices: 28, amount: 78500 },
                { name: '61-90 days', invoices: 15, amount: 42000 },
                { name: '90+ days', invoices: 8, amount: 22500 },
              ]}
              dataKey="invoices"
              horizontal
              height={220}
            />
          </ChartContainer>

          {/* Processing Time Trend */}
          <ChartContainer
            title="Processing Time Trend"
            description="Average hours to process invoices"
          >
            <BillForgeLineChart
              data={processingTimeData}
              dataKey="time"
              showDots
              height={220}
              formatter={(v) => `${v}h`}
            />
          </ChartContainer>
        </div>
      )}

      {/* Vendor Analytics - Enhanced */}
      {showVendorMetrics && (
        <div className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-vendor via-vendor/70 to-transparent" />
          <div className="p-6">
            <div className="flex items-center justify-between mb-6">
              <div className="flex items-center gap-3">
                <div className="p-2.5 rounded-xl bg-vendor/10">
                  <Users className="w-5 h-5 text-vendor" />
                </div>
                <div>
                  <h2 className="font-semibold text-foreground">Vendor Analytics</h2>
                  <p className="text-sm text-muted-foreground">Supplier performance and spend</p>
                </div>
              </div>
              <Link href="/vendors" className="text-sm text-primary hover:underline flex items-center gap-1">
                View all vendors
                <ArrowRight className="w-3.5 h-3.5" />
              </Link>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              {/* Vendor Stats */}
              <div className="space-y-3">
                {[
                  { label: 'Active Vendors', value: summary?.vendors_active ?? 0, color: 'bg-vendor' },
                  { label: 'W-9 On File', value: '85%', color: 'bg-success' },
                  { label: 'New This Month', value: 12, color: 'bg-capture' },
                ].map((metric) => (
                  <div key={metric.label} className="flex items-center justify-between p-3 bg-secondary/50 rounded-xl">
                    <div className="flex items-center gap-3">
                      <div className={`w-2 h-8 rounded-full ${metric.color}`} />
                      <span className="text-sm text-muted-foreground">{metric.label}</span>
                    </div>
                    <span className="text-lg font-semibold text-foreground">{metric.value}</span>
                  </div>
                ))}
              </div>

              {/* Top Vendors by Spend */}
              <div className="lg:col-span-2">
                <h3 className="text-sm font-medium text-foreground mb-3">Top Vendors by Spend</h3>
                <BillForgeBarChart
                  data={vendorSpendData}
                  dataKey="spend"
                  horizontal
                  height={180}
                  formatter={formatCurrency}
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Quick Reports Grid */}
      <div className="card p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="font-semibold text-foreground">Available Reports</h2>
            <p className="text-sm text-muted-foreground">Generate detailed reports</p>
          </div>
          <Link href="/reports/export" className="text-sm text-primary hover:underline flex items-center gap-1">
            View all
            <ArrowRight className="w-3.5 h-3.5" />
          </Link>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {showVendorMetrics && (
            <button className="action-card group text-left">
              <div className="action-card-icon bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <div className="flex-1">
                <p className="font-medium text-foreground">Vendor Spend</p>
                <p className="text-sm text-muted-foreground">Analysis by vendor</p>
              </div>
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-vendor group-hover:translate-x-1 transition-all" />
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
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
              </button>

              <button className="action-card group text-left">
                <div className="action-card-icon bg-processing/10">
                  <Calendar className="w-5 h-5 text-processing" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Payment Schedule</p>
                  <p className="text-sm text-muted-foreground">Upcoming payments</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
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
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-capture group-hover:translate-x-1 transition-all" />
            </button>
          )}

          <button className="action-card group text-left">
            <div className="action-card-icon bg-reporting/10">
              <Activity className="w-5 h-5 text-reporting" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Monthly Summary</p>
              <p className="text-sm text-muted-foreground">Financial overview</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-reporting group-hover:translate-x-1 transition-all" />
          </button>
        </div>
      </div>

      {/* Export Info */}
      <div className="p-4 bg-reporting/5 border border-reporting/20 rounded-xl">
        <div className="flex items-start gap-3">
          <div className="p-2 rounded-lg bg-reporting/10">
            <Download className="w-4 h-4 text-reporting" />
          </div>
          <div>
            <h3 className="font-medium text-foreground">Export Options</h3>
            <p className="text-sm text-muted-foreground mt-1">
              All reports can be exported to PDF, Excel, or CSV formats. Schedule recurring reports to be delivered to your email automatically.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
