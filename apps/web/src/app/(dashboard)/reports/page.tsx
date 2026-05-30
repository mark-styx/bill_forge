'use client';

import { useMemo, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { reportsApi, predictiveApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import {
  ChartContainer,
  BillForgeAreaChart,
  BillForgeBarChart,
  BillForgeLineChart,
  BillForgeDonutChart,
  BillForgeSparkline,
} from '@/components/ui/charts';
import {
  BarChart3,
  DollarSign,
  Clock,
  Download,
  Calendar,
  Users,
  FileText,
  ScanLine,
  ArrowRight,
  Filter,
  Activity,
  RefreshCw,
  AlertCircle,
  ShieldAlert,
  Eye,
  EyeOff,
  Sparkles,
  Shield,
  Bell,
} from 'lucide-react';

const emptySparkline = [0, 0, 0, 0, 0, 0, 0];

type DatePreset = 'last_30_days' | 'last_90_days' | 'last_12_months';
type ReportSectionFilter = 'all' | 'cash_flow' | 'vendors' | 'processing' | 'ocr';
type StatusFilter = 'all' | 'submitted' | 'pending_approval' | 'approved' | 'ready_for_payment';

const reportDatePresets: Record<DatePreset, { label: string; days: number; groupBy: 'day' | 'week' | 'month' }> = {
  last_30_days: { label: 'Last 30 days', days: 30, groupBy: 'day' },
  last_90_days: { label: 'Last 90 days', days: 90, groupBy: 'week' },
  last_12_months: { label: 'Last 12 months', days: 365, groupBy: 'month' },
};

const reportSections = [
  { id: 'all', label: 'All reports' },
  { id: 'cash_flow', label: 'Cash flow' },
  { id: 'vendors', label: 'Vendors' },
  { id: 'processing', label: 'Processing' },
  { id: 'ocr', label: 'OCR' },
] satisfies Array<{ id: ReportSectionFilter; label: string }>;

const statusFilters = [
  { id: 'all', label: 'All statuses' },
  { id: 'submitted', label: 'Submitted' },
  { id: 'pending_approval', label: 'Pending approval' },
  { id: 'approved', label: 'Approved' },
  { id: 'ready_for_payment', label: 'Ready for payment' },
] satisfies Array<{ id: StatusFilter; label: string }>;

function ReportNotice({ title, description, tone = 'muted' }: {
  title: string;
  description: string;
  tone?: 'muted' | 'error';
}) {
  const iconClass = tone === 'error' ? 'text-error' : 'text-muted-foreground';
  const borderClass = tone === 'error' ? 'border-error/30 bg-error/5' : 'border-border bg-secondary/30';

  return (
    <div className={`flex items-start gap-3 rounded-lg border p-4 ${borderClass}`}>
      <AlertCircle className={`mt-0.5 h-4 w-4 ${iconClass}`} />
      <div>
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      </div>
    </div>
  );
}

export default function ReportsPage() {
  const { hasModule } = useAuthStore();
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [datePreset, setDatePreset] = useState<DatePreset>('last_12_months');
  const [sectionFilter, setSectionFilter] = useState<ReportSectionFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [lateRiskOnly, setLateRiskOnly] = useState(false);
  const [cashThresholdInput, setCashThresholdInput] = useState('100000');
  const [scenarioVendor, setScenarioVendor] = useState('all');
  const [scenarioDelayInput, setScenarioDelayInput] = useState('0');
  const [scenarioDiscountPercent, setScenarioDiscountPercent] = useState('0');

  const showOcrMetrics = hasModule('invoice_capture');
  const showProcessingMetrics = hasModule('invoice_processing');
  const showVendorMetrics = hasModule('vendor_management');
  const selectedPreset = reportDatePresets[datePreset];
  const reportDateRange = useMemo(() => {
    const end = new Date();
    const start = new Date(end);
    start.setDate(end.getDate() - selectedPreset.days);

    return {
      start_date: start.toISOString().slice(0, 10),
      end_date: end.toISOString().slice(0, 10),
      group_by: selectedPreset.groupBy,
    };
  }, [selectedPreset.days, selectedPreset.groupBy]);

  const metricsQuery = useQuery({
    queryKey: ['workflow-metrics'],
    queryFn: () => reportsApi.workflowMetrics(),
    enabled: showProcessingMetrics,
  });

  const agingQuery = useQuery({
    queryKey: ['invoice-aging'],
    queryFn: () => reportsApi.invoiceAging(),
    enabled: showProcessingMetrics,
  });

  const statusQuery = useQuery({
    queryKey: ['invoice-status-distribution'],
    queryFn: () => reportsApi.invoicesByStatus(),
    enabled: showProcessingMetrics,
  });

  const spendTrendQuery = useQuery({
    queryKey: ['spend-trends', reportDateRange],
    queryFn: () => reportsApi.spendTrends(reportDateRange),
    enabled: showProcessingMetrics,
  });

  const vendorSpendQuery = useQuery({
    queryKey: ['vendor-spend', reportDateRange],
    queryFn: () => reportsApi.invoicesByVendor(reportDateRange),
    enabled: showVendorMetrics,
  });

  const summaryQuery = useQuery({
    queryKey: ['dashboard-summary'],
    queryFn: () => reportsApi.dashboardSummary(),
  });

  const approvalSlaQuery = useQuery({
    queryKey: ['approval-sla'],
    queryFn: () => reportsApi.approvalSla(),
    enabled: showProcessingMetrics,
  });

  const cashFlowQuery = useQuery({
    queryKey: ['cash-flow-obligations'],
    queryFn: () => reportsApi.cashFlowObligations(),
    enabled: showProcessingMetrics,
  });

  // --- Predictive Analytics Queries ---
  const queryClient = useQueryClient();

  const anomaliesQuery = useQuery({
    queryKey: ['predictive-anomalies'],
    queryFn: () => predictiveApi.getAnomalies(),
  });

  const budgetAlertsQuery = useQuery({
    queryKey: ['predictive-budget-alerts'],
    queryFn: () => predictiveApi.getBudgetAlerts(),
  });

  const anomalyRulesQuery = useQuery({
    queryKey: ['predictive-anomaly-rules'],
    queryFn: () => predictiveApi.getAnomalyRules(),
  });

  const detectAnomaliesMutation = useMutation({
    mutationFn: () => predictiveApi.detectAnomalies(),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['predictive-anomalies'] }),
  });

  const dismissAlertMutation = useMutation({
    mutationFn: (id: string) => predictiveApi.dismissAlert(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['predictive-budget-alerts'] }),
  });

  const acknowledgeAnomalyMutation = useMutation({
    mutationFn: (id: string) => predictiveApi.acknowledgeAnomaly(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['predictive-anomalies'] }),
  });

  const anomalies = anomaliesQuery.data ?? [];
  const budgetAlerts = budgetAlertsQuery.data ?? [];
  const anomalyRules = anomalyRulesQuery.data ?? [];

  const metrics = metricsQuery.data;
  const summary = summaryQuery.data;
  const reportError = [
    summaryQuery,
    metricsQuery,
    agingQuery,
    statusQuery,
    spendTrendQuery,
    vendorSpendQuery,
    approvalSlaQuery,
    cashFlowQuery,
  ].some((query) => query.isError);
  const anyLoading = [
    summaryQuery,
    metricsQuery,
    agingQuery,
    statusQuery,
    spendTrendQuery,
    vendorSpendQuery,
    approvalSlaQuery,
    cashFlowQuery,
  ].some((query) => query.isLoading || query.isFetching);

  const monthlyData = (spendTrendQuery.data ?? []).map((point) => ({
    name: point.period,
    invoices: point.invoice_count,
    amount: point.total_spend,
  }));

  const processingTimeData = metrics
    ? [{ name: 'Average', time: metrics.avg_processing_time_hours }]
    : [];

  const statusDistribution = (statusQuery.data ?? []).map((item) => ({
    name: item.status.replace(/_/g, ' '),
    value: item.count,
    amount: item.total_amount,
  }));

  const vendorSpendData = (vendorSpendQuery.data ?? []).map((vendor) => ({
    name: vendor.vendor_name,
    spend: vendor.total_amount,
  }));

  const cashFlowObligations = cashFlowQuery.data ?? [];
  const now = new Date();
  const weekEnd = new Date(now);
  weekEnd.setDate(now.getDate() + 7);
  const monthEnd = new Date(now);
  monthEnd.setDate(now.getDate() + 30);
  const cashDateRangeStart = new Date(now);
  cashDateRangeStart.setHours(0, 0, 0, 0);
  const cashDateRangeEnd = new Date(now);
  cashDateRangeEnd.setDate(now.getDate() + selectedPreset.days);
  cashDateRangeEnd.setHours(23, 59, 59, 999);
  const obligationDate = (item: { projected_payment_date?: string | null; due_date?: string | null }) =>
    item.projected_payment_date ?? item.due_date ?? null;
  const cashFlowInDateRange = cashFlowObligations.filter((item) => {
    const due = obligationDate(item);
    if (!due) return false;
    const date = new Date(due);
    return date >= cashDateRangeStart && date <= cashDateRangeEnd;
  });
  const filteredCashFlowObligations = cashFlowInDateRange.filter((item) => {
    if (statusFilter !== 'all' && item.processing_status !== statusFilter) return false;
    if (lateRiskOnly && !item.late_risk) return false;
    return true;
  });
  const cashDueThisWeek = filteredCashFlowObligations
    .filter((item) => {
      const due = obligationDate(item);
      if (!due) return false;
      const date = new Date(due);
      return date >= now && date <= weekEnd;
    })
    .reduce((sum, item) => sum + item.amount_cents, 0);
  const cashDueThisMonth = filteredCashFlowObligations
    .filter((item) => {
      const due = obligationDate(item);
      if (!due) return false;
      const date = new Date(due);
      return date >= now && date <= monthEnd;
    })
    .reduce((sum, item) => sum + item.amount_cents, 0);
  const lateRiskCount = filteredCashFlowObligations.filter((item) => item.late_risk).length;
  const upcomingObligations = filteredCashFlowObligations.slice(0, 6);
  const vendorOptions = Array.from(new Set(cashFlowInDateRange.map((item) => item.vendor_name))).sort();
  const cashThresholdCents = Math.max(0, Math.round((Number(cashThresholdInput) || 0) * 100));
  const scenarioDelayDays = Math.max(0, Math.min(90, Number(scenarioDelayInput) || 0));
  const scenarioDiscountRate = Math.max(0, Math.min(25, Number(scenarioDiscountPercent) || 0)) / 100;
  const scenarioObligations = filteredCashFlowObligations.map((item) => {
    if (scenarioVendor !== 'all' && item.vendor_name !== scenarioVendor) return item;
    const due = obligationDate(item);
    const adjustedDate = due ? new Date(due) : null;
    adjustedDate?.setDate(adjustedDate.getDate() + scenarioDelayDays);
    return {
      ...item,
      amount_cents: Math.round(item.amount_cents * (1 - scenarioDiscountRate)),
      projected_payment_date: adjustedDate ? adjustedDate.toISOString().slice(0, 10) : item.projected_payment_date,
    };
  });
  const modeledCashDueThisMonth = scenarioObligations
    .filter((item) => {
      const due = obligationDate(item);
      if (!due) return false;
      const date = new Date(due);
      return date >= now && date <= monthEnd;
    })
    .reduce((sum, item) => sum + item.amount_cents, 0);
  const modeledSavings = filteredCashFlowObligations.reduce((sum, item, index) => {
    return sum + Math.max(0, item.amount_cents - scenarioObligations[index].amount_cents);
  }, 0);
  const buildBucketTotals = (items: typeof filteredCashFlowObligations, bucket: 'week' | 'month') => {
    const totals = items.reduce<Record<string, number>>((acc, item) => {
      const due = obligationDate(item);
      if (!due) return acc;
      const date = new Date(due);
      const label = bucket === 'week'
        ? `Week of ${new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' }).format(date)}`
        : new Intl.DateTimeFormat('en-US', { month: 'short', year: 'numeric' }).format(date);
      acc[label] = (acc[label] ?? 0) + item.amount_cents;
      return acc;
    }, {});

    return Object.entries(totals)
      .map(([name, amount]) => ({ name, amount }))
      .slice(0, 6);
  };
  const weeklyCashFlow = buildBucketTotals(scenarioObligations, 'week');
  const monthlyCashFlow = buildBucketTotals(scenarioObligations, 'month');
  const thresholdExceeded = cashThresholdCents > 0 && modeledCashDueThisMonth > cashThresholdCents;
  const approvalSla = approvalSlaQuery.data;
  const bottleneckApprovers = (approvalSla?.items ?? [])
    .reduce<Record<string, number>>((acc, item) => {
      acc[item.approver_label] = (acc[item.approver_label] ?? 0) + 1;
      return acc;
    }, {});
  const topBottlenecks = Object.entries(bottleneckApprovers)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3);

  const formatCurrency = (value: number) => `$${(value / 1000).toFixed(0)}k`;
  const formatCents = (value: number, currency = 'USD') =>
    new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency,
      maximumFractionDigits: 0,
    }).format(value / 100);
  const formatDate = (value?: string | null) => {
    if (!value) return 'Unscheduled';
    return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' }).format(new Date(value));
  };
  const refreshReports = () => {
    void summaryQuery.refetch();
    void metricsQuery.refetch();
    void agingQuery.refetch();
    void statusQuery.refetch();
    void spendTrendQuery.refetch();
    void vendorSpendQuery.refetch();
    void approvalSlaQuery.refetch();
    void cashFlowQuery.refetch();
    void anomaliesQuery.refetch();
    void budgetAlertsQuery.refetch();
    void anomalyRulesQuery.refetch();
  };

  const severityBg = (s: string) => {
    switch (s) {
      case 'critical': return 'bg-error/10 text-error';
      case 'high': return 'bg-error/10 text-error';
      case 'medium': return 'bg-warning/10 text-warning';
      case 'low': return 'bg-success/10 text-success';
      default: return 'bg-secondary text-muted-foreground';
    }
  };

  const anomalyTypeLabel = (t: string) =>
    t.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  const showSection = (section: ReportSectionFilter) => sectionFilter === 'all' || sectionFilter === section;

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
          <button className="btn btn-secondary btn-sm" onClick={refreshReports} disabled={anyLoading}>
            <RefreshCw className={`w-4 h-4 mr-1.5 ${anyLoading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
          <button className="btn btn-secondary btn-sm" onClick={() => setFiltersOpen((open) => !open)}>
            <Filter className="w-4 h-4 mr-1.5" />
            Filter
          </button>
          <Link href="/reports/export" className="btn btn-primary btn-sm">
            <Download className="w-4 h-4 mr-1.5" />
            Export
          </Link>
        </div>
      </div>

      {filtersOpen && (
        <div className="card p-4" aria-label="Report filters">
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <label className="space-y-2">
              <span className="text-sm font-medium text-foreground">Date range</span>
              <select
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                value={datePreset}
                onChange={(event) => setDatePreset(event.target.value as DatePreset)}
              >
                {Object.entries(reportDatePresets).map(([value, preset]) => (
                  <option key={value} value={value}>{preset.label}</option>
                ))}
              </select>
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium text-foreground">Report section</span>
              <select
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                value={sectionFilter}
                onChange={(event) => setSectionFilter(event.target.value as ReportSectionFilter)}
              >
                {reportSections.map((section) => (
                  <option key={section.id} value={section.id}>{section.label}</option>
                ))}
              </select>
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium text-foreground">Payment status</span>
              <select
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                value={statusFilter}
                onChange={(event) => setStatusFilter(event.target.value as StatusFilter)}
              >
                {statusFilters.map((status) => (
                  <option key={status.id} value={status.id}>{status.label}</option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-3 rounded-lg border border-border px-3 py-2 text-sm">
              <input
                type="checkbox"
                checked={lateRiskOnly}
                onChange={(event) => setLateRiskOnly(event.target.checked)}
              />
              <span>Late-risk obligations only</span>
            </label>
          </div>
        </div>
      )}

      {reportError && (
        <ReportNotice
          tone="error"
          title="Some report data could not be loaded"
          description="The affected reports are marked below. Retry refresh after the API is available."
        />
      )}

      {/* Key Metrics - Modern Cards */}
      <div id="monthly-summary" className="grid grid-cols-2 lg:grid-cols-4 gap-4 scroll-mt-24">
        {showVendorMetrics && (
          <div className="bright-stat-card group animate-fade-in-up">
            <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-vendor to-vendor/50" />
            <div className="flex items-center justify-between mb-3">
              <div className="p-2.5 rounded-xl bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <BillForgeSparkline
                data={vendorSpendData.length > 0 ? vendorSpendData.map((vendor) => vendor.spend).slice(0, 7) : emptySparkline}
                color="vendor"
                showArea
              />
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
              </div>
              <p className="text-2xl font-bold text-foreground">{summary?.invoices_processed_today ?? '—'}</p>
              <p className="text-sm text-muted-foreground">Processed Today</p>
            </div>

            <div className="bright-stat-card group animate-fade-in-up" style={{ animationDelay: '100ms' }}>
              <div className="absolute top-0 left-0 w-1 h-full rounded-l-2xl bg-gradient-to-b from-accent to-accent/50" />
              <div className="flex items-center justify-between mb-3">
                <div className="p-2.5 rounded-xl bg-accent/10">
                  <DollarSign className="w-5 h-5 text-accent" />
                </div>
                <BillForgeSparkline
                  data={monthlyData.length > 0 ? monthlyData.map((point) => point.amount).slice(-7) : emptySparkline}
                  color="accent"
                  showArea
                />
              </div>
              <p className="text-2xl font-bold text-foreground">${(summary?.total_pending_amount ?? 0).toLocaleString()}</p>
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
            </div>
            <p className="text-2xl font-bold text-foreground">{metrics?.avg_processing_time_hours?.toFixed(1) ?? '—'}h</p>
            <p className="text-sm text-muted-foreground">Avg Processing</p>
          </div>
        )}
      </div>

      {/* Main Charts Row */}
      {showProcessingMetrics && showSection('processing') && (
      <div id="invoice-volume" className="grid grid-cols-1 lg:grid-cols-3 gap-6 scroll-mt-24">
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
            {spendTrendQuery.isError ? (
              <ReportNotice
                tone="error"
                title="Invoice trend unavailable"
                description="The spend trend endpoint failed, so this chart is not using fallback sample data."
              />
            ) : monthlyData.length === 0 && !spendTrendQuery.isLoading ? (
              <ReportNotice
                title="No invoice trend data"
                description="No tenant invoice volume was returned for the selected date range."
              />
            ) : (
              <BillForgeAreaChart
                data={monthlyData}
                dataKey="invoices"
                color="primary"
                formatter={(v) => v.toString()}
              />
            )}
          </ChartContainer>
        </div>

        {/* Status Distribution */}
        <ChartContainer
          title="Invoice Status"
          description="Current status distribution"
        >
          {statusQuery.isError ? (
            <ReportNotice
              tone="error"
              title="Invoice status unavailable"
              description="The status distribution endpoint failed."
            />
          ) : statusDistribution.length === 0 && !statusQuery.isLoading ? (
            <ReportNotice
              title="No status data"
              description="No invoice statuses were returned for this tenant."
            />
          ) : (
            <>
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
                        <span className="text-muted-foreground capitalize">{item.name}</span>
                      </div>
                      <span className="font-medium text-foreground">{item.value}</span>
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </ChartContainer>
      </div>
      )}

      {/* OCR Metrics - Enhanced */}
      {showOcrMetrics && showSection('ocr') && (
        <div id="ocr-performance" className="card overflow-hidden scroll-mt-24">
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

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <ReportNotice
                title="OCR accuracy unavailable"
                description="Connect capture quality telemetry before showing accuracy, process-time, or error-rate metrics."
              />
              {[
                { label: 'Pending Review', value: summary?.invoices_pending_review ?? 0, icon: Clock },
                { label: 'Processed Today', value: summary?.invoices_processed_today ?? 0, icon: FileText },
              ].map((metric) => (
                <div key={metric.label} className="p-4 bg-secondary/50 rounded-xl flex flex-col justify-center">
                  <div className="flex items-center gap-2 mb-2">
                    <metric.icon className="w-4 h-4 text-capture" />
                    <span className="text-sm text-muted-foreground">{metric.label}</span>
                  </div>
                  <p className="text-2xl font-bold text-foreground">{metric.value}</p>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Processing Metrics Row */}
      {showProcessingMetrics && showSection('processing') && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Invoice Aging with Bars */}
          <ChartContainer
            title="Invoice Aging"
            description="Distribution by days outstanding"
          >
            {agingQuery.isError ? (
              <ReportNotice
                tone="error"
                title="Invoice aging unavailable"
                description="The invoice aging endpoint failed."
              />
            ) : (agingQuery.data ?? []).length === 0 && !agingQuery.isLoading ? (
              <ReportNotice
                title="No aging data"
                description="No outstanding invoice aging buckets were returned."
              />
            ) : (
              <BillForgeBarChart
                data={(agingQuery.data ?? []).map((bucket) => ({
                  name: bucket.bucket,
                  invoices: bucket.count,
                  amount: bucket.total_amount,
                }))}
                dataKey="invoices"
                horizontal
                height={220}
              />
            )}
          </ChartContainer>

          {/* Processing Time Trend */}
          <ChartContainer
            title="Processing Time Trend"
            description="Average hours to process invoices"
          >
            {metricsQuery.isError ? (
              <ReportNotice
                tone="error"
                title="Processing metrics unavailable"
                description="The workflow metrics endpoint failed."
              />
            ) : processingTimeData.length === 0 && !metricsQuery.isLoading ? (
              <ReportNotice
                title="No processing metrics"
                description="No workflow metrics were returned for this tenant."
              />
            ) : (
              <BillForgeLineChart
                data={processingTimeData}
                dataKey="time"
                showDots
                height={220}
                formatter={(v) => `${v}h`}
              />
            )}
          </ChartContainer>
        </div>
      )}

      {showProcessingMetrics && (showSection('processing') || showSection('cash_flow')) && (
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-6">
          {showSection('processing') && (
          <div className="card overflow-hidden">
            <div className="h-1.5 bg-gradient-to-r from-processing via-processing/70 to-transparent" />
            <div className="p-6">
              <div className="flex items-center justify-between gap-3 mb-5">
                <div className="flex items-center gap-3">
                  <div className="p-2.5 rounded-xl bg-processing/10">
                    <Clock className="w-5 h-5 text-processing" />
                  </div>
                  <div>
                    <h2 className="font-semibold text-foreground">Approval SLA</h2>
                    <p className="text-sm text-muted-foreground">Pending approval bottlenecks and deadlines</p>
                  </div>
                </div>
                <span className="text-sm font-medium text-muted-foreground">
                  {approvalSla?.pending_count ?? 0} pending
                </span>
              </div>

              {approvalSlaQuery.isError ? (
                <ReportNotice
                  tone="error"
                  title="Approval SLA unavailable"
                  description="The SLA tracking endpoint failed."
                />
              ) : (approvalSla?.items ?? []).length === 0 && !approvalSlaQuery.isLoading ? (
                <ReportNotice
                  title="No pending approvals"
                  description="There are no approval SLA items waiting on action."
                />
              ) : (
                <div className="space-y-5">
                  <div className="grid grid-cols-3 gap-3">
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">Near breach</p>
                      <p className="mt-1 text-2xl font-semibold text-warning">{approvalSla?.near_breach_count ?? 0}</p>
                    </div>
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">Breached</p>
                      <p className="mt-1 text-2xl font-semibold text-error">{approvalSla?.breached_count ?? 0}</p>
                    </div>
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">Bottlenecks</p>
                      <p className="mt-1 text-2xl font-semibold text-foreground">{topBottlenecks.length}</p>
                    </div>
                  </div>

                  <div className="space-y-3">
                    {(approvalSla?.items ?? []).slice(0, 5).map((item) => (
                      <Link
                        key={item.approval_id}
                        href={`/invoices/${item.invoice_id}`}
                        className="block rounded-xl border border-border p-3 hover:bg-secondary/40 transition-colors"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <p className="font-medium text-foreground truncate">{item.invoice_number}</p>
                            <p className="text-sm text-muted-foreground truncate">{item.vendor_name} · {item.approver_label}</p>
                          </div>
                          <span className={`text-xs font-medium rounded-full px-2 py-1 ${
                            item.sla_state === 'breached'
                              ? 'bg-error/10 text-error'
                              : item.sla_state === 'near_breach'
                                ? 'bg-warning/10 text-warning'
                                : 'bg-success/10 text-success'
                          }`}>
                            {Math.round(item.percent_elapsed)}%
                          </span>
                        </div>
                        <div className="mt-3 h-2 rounded-full bg-secondary overflow-hidden">
                          <div
                            className={`h-full rounded-full ${
                              item.sla_state === 'breached'
                                ? 'bg-error'
                                : item.sla_state === 'near_breach'
                                  ? 'bg-warning'
                                  : 'bg-success'
                            }`}
                            style={{ width: `${Math.min(item.percent_elapsed, 100)}%` }}
                          />
                        </div>
                        <p className="mt-2 text-xs text-muted-foreground">
                          Due {formatDate(item.deadline_at)} · {item.hours_waiting.toFixed(1)}h of {item.sla_hours}h elapsed
                        </p>
                      </Link>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
          )}

          {showSection('cash_flow') && (
          <div id="cash-flow" className="card overflow-hidden scroll-mt-24">
            <div className="h-1.5 bg-gradient-to-r from-accent via-accent/70 to-transparent" />
            <div className="p-6">
              <div className="flex items-center gap-3 mb-5">
                <div className="p-2.5 rounded-xl bg-accent/10">
                  <Calendar className="w-5 h-5 text-accent" />
                </div>
                <div>
                  <h2 className="font-semibold text-foreground">Cash Flow Obligations</h2>
                  <p className="text-sm text-muted-foreground">Upcoming projected payment commitments</p>
                </div>
              </div>

              {cashFlowQuery.isError ? (
                <ReportNotice
                  tone="error"
                  title="Cash-flow forecast unavailable"
                  description="The obligations endpoint failed."
                />
              ) : filteredCashFlowObligations.length === 0 && !cashFlowQuery.isLoading ? (
                <ReportNotice
                  title="No upcoming obligations"
                  description="No approved or in-flight invoices match the active report filters."
                />
              ) : (
                <div className="space-y-5">
                  <div className="grid grid-cols-3 gap-3">
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">7 days</p>
                      <p className="mt-1 text-xl font-semibold text-foreground">{formatCents(cashDueThisWeek)}</p>
                    </div>
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">30 days</p>
                      <p className="mt-1 text-xl font-semibold text-foreground">{formatCents(cashDueThisMonth)}</p>
                    </div>
                    <div className="p-3 rounded-xl bg-secondary/50">
                      <p className="text-xs text-muted-foreground">Late risk</p>
                      <p className="mt-1 text-xl font-semibold text-error">{lateRiskCount}</p>
                    </div>
                  </div>

                  <div className="rounded-xl border border-border p-4">
                    <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
                      <label className="space-y-1">
                        <span className="text-xs font-medium text-muted-foreground">Runway threshold</span>
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          inputMode="numeric"
                          value={cashThresholdInput}
                          onChange={(event) => setCashThresholdInput(event.target.value)}
                        />
                      </label>
                      <label className="space-y-1">
                        <span className="text-xs font-medium text-muted-foreground">Scenario vendor</span>
                        <select
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          value={scenarioVendor}
                          onChange={(event) => setScenarioVendor(event.target.value)}
                        >
                          <option value="all">All vendors</option>
                          {vendorOptions.map((vendor) => (
                            <option key={vendor} value={vendor}>{vendor}</option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-1">
                        <span className="text-xs font-medium text-muted-foreground">Delay days</span>
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          inputMode="numeric"
                          value={scenarioDelayInput}
                          onChange={(event) => setScenarioDelayInput(event.target.value)}
                        />
                      </label>
                      <label className="space-y-1">
                        <span className="text-xs font-medium text-muted-foreground">Discount %</span>
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          inputMode="decimal"
                          value={scenarioDiscountPercent}
                          onChange={(event) => setScenarioDiscountPercent(event.target.value)}
                        />
                      </label>
                    </div>
                    <div className="mt-4 grid grid-cols-1 sm:grid-cols-3 gap-3">
                      <div>
                        <p className="text-xs text-muted-foreground">Modeled 30 days</p>
                        <p className="font-semibold text-foreground">{formatCents(modeledCashDueThisMonth)}</p>
                      </div>
                      <div>
                        <p className="text-xs text-muted-foreground">Modeled savings</p>
                        <p className="font-semibold text-success">{formatCents(modeledSavings)}</p>
                      </div>
                      <div>
                        <p className="text-xs text-muted-foreground">Scenario</p>
                        <p className="font-semibold text-foreground">{scenarioDelayDays} day delay</p>
                      </div>
                    </div>
                    {thresholdExceeded && (
                      <div className="mt-4">
                        <ReportNotice
                          tone="error"
                          title="Runway threshold exceeded"
                          description={`${formatCents(modeledCashDueThisMonth)} is modeled inside 30 days against a ${formatCents(cashThresholdCents)} threshold.`}
                        />
                      </div>
                    )}
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="rounded-xl border border-border p-4">
                      <h3 className="text-sm font-medium text-foreground mb-3">Weekly forecast</h3>
                      <div className="space-y-2">
                        {weeklyCashFlow.map((bucket) => (
                          <div key={bucket.name} className="flex items-center justify-between text-sm">
                            <span className="text-muted-foreground">{bucket.name}</span>
                            <span className="font-medium text-foreground">{formatCents(bucket.amount)}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                    <div className="rounded-xl border border-border p-4">
                      <h3 className="text-sm font-medium text-foreground mb-3">Monthly forecast</h3>
                      <div className="space-y-2">
                        {monthlyCashFlow.map((bucket) => (
                          <div key={bucket.name} className="flex items-center justify-between text-sm">
                            <span className="text-muted-foreground">{bucket.name}</span>
                            <span className="font-medium text-foreground">{formatCents(bucket.amount)}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>

                  <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
                    {upcomingObligations.map((item) => (
                      <Link
                        key={item.invoice_id}
                        href={`/invoices/${item.invoice_id}`}
                        className="flex items-center justify-between gap-3 p-3 hover:bg-secondary/40 transition-colors"
                      >
                        <div className="min-w-0">
                          <p className="font-medium text-foreground truncate">{item.vendor_name}</p>
                          <p className="text-sm text-muted-foreground truncate">
                            {item.invoice_number} · {formatDate(item.projected_payment_date ?? item.due_date)}
                          </p>
                        </div>
                        <div className="text-right flex-shrink-0">
                          <p className="font-semibold text-foreground">{formatCents(item.amount_cents, item.currency)}</p>
                          <p className={`text-xs ${item.late_risk ? 'text-error' : 'text-muted-foreground'}`}>
                            {item.late_risk ? 'Late risk' : item.processing_status.replace(/_/g, ' ')}
                          </p>
                        </div>
                      </Link>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
          )}
        </div>
      )}

      {/* Vendor Analytics - Enhanced */}
      {showVendorMetrics && showSection('vendors') && (
        <div id="vendor-spend" className="card overflow-hidden scroll-mt-24">
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
                  { label: 'Vendor Spend Rows', value: vendorSpendData.length, color: 'bg-success' },
                ].map((metric) => (
                  <div key={metric.label} className="flex items-center justify-between p-3 bg-secondary/50 rounded-xl">
                    <div className="flex items-center gap-3">
                      <div className={`w-2 h-8 rounded-full ${metric.color}`} />
                      <span className="text-sm text-muted-foreground">{metric.label}</span>
                    </div>
                    <span className="text-lg font-semibold text-foreground">{metric.value}</span>
                  </div>
                ))}
                <ReportNotice
                  title="Compliance metrics unavailable"
                  description="W-9 coverage and new-vendor counts need a live vendor compliance feed before they can be reported."
                />
              </div>

              {/* Top Vendors by Spend */}
              <div className="lg:col-span-2">
                <h3 className="text-sm font-medium text-foreground mb-3">Top Vendors by Spend</h3>
                {vendorSpendQuery.isError ? (
                  <ReportNotice
                    tone="error"
                    title="Vendor spend unavailable"
                    description="The vendor spend endpoint failed."
                  />
                ) : vendorSpendData.length === 0 && !vendorSpendQuery.isLoading ? (
                  <ReportNotice
                    title="No vendor spend"
                    description="No vendor spend records were returned for this tenant."
                  />
                ) : (
                  <BillForgeBarChart
                    data={vendorSpendData}
                    dataKey="spend"
                    horizontal
                    height={180}
                    formatter={formatCurrency}
                  />
                )}
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
            <a href="#vendor-spend" className="action-card group text-left">
              <div className="action-card-icon bg-vendor/10">
                <Users className="w-5 h-5 text-vendor" />
              </div>
              <div className="flex-1">
                <p className="font-medium text-foreground">Vendor Spend</p>
                <p className="text-sm text-muted-foreground">Analysis by vendor</p>
              </div>
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-vendor group-hover:translate-x-1 transition-all" />
            </a>
          )}

          {showProcessingMetrics && (
            <>
              <a href="#invoice-volume" className="action-card group text-left">
                <div className="action-card-icon bg-processing/10">
                  <BarChart3 className="w-5 h-5 text-processing" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Invoice Volume</p>
                  <p className="text-sm text-muted-foreground">Monthly trends</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
              </a>

              <a href="#cash-flow" className="action-card group text-left">
                <div className="action-card-icon bg-processing/10">
                  <Calendar className="w-5 h-5 text-processing" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">Payment Schedule</p>
                  <p className="text-sm text-muted-foreground">Upcoming payments</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-processing group-hover:translate-x-1 transition-all" />
              </a>

              <Link href="/reports/cash-flow-forecast" className="action-card group text-left">
                <div className="action-card-icon bg-accent/10">
                  <DollarSign className="w-5 h-5 text-accent" />
                </div>
                <div className="flex-1">
                  <p className="font-medium text-foreground">13-Week Cash Forecast</p>
                  <p className="text-sm text-muted-foreground">AP-driven funding view</p>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-accent group-hover:translate-x-1 transition-all" />
              </Link>
            </>
          )}

          {showOcrMetrics && (
            <a href="#ocr-performance" className="action-card group text-left">
              <div className="action-card-icon bg-capture/10">
                <ScanLine className="w-5 h-5 text-capture" />
              </div>
              <div className="flex-1">
                <p className="font-medium text-foreground">OCR Performance</p>
                <p className="text-sm text-muted-foreground">Accuracy metrics</p>
              </div>
              <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-capture group-hover:translate-x-1 transition-all" />
            </a>
          )}

          <a href="#monthly-summary" className="action-card group text-left">
            <div className="action-card-icon bg-reporting/10">
              <Activity className="w-5 h-5 text-reporting" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Monthly Summary</p>
              <p className="text-sm text-muted-foreground">Financial overview</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-reporting group-hover:translate-x-1 transition-all" />
          </a>
        </div>
      </div>

      {/* Predictive Insights */}
      <div className="space-y-6">
        <div className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-reporting via-accent to-transparent" />
          <div className="p-6">
            <div className="flex items-center justify-between gap-3 mb-6">
              <div className="flex items-center gap-3">
                <div className="p-2.5 rounded-xl bg-reporting/10">
                  <Sparkles className="w-5 h-5 text-reporting" />
                </div>
                <div>
                  <h2 className="font-semibold text-foreground">Predictive Insights</h2>
                  <p className="text-sm text-muted-foreground">Anomaly detection, budget alerts, and proactive recommendations</p>
                </div>
              </div>
              <button
                className="btn btn-secondary btn-sm"
                onClick={() => detectAnomaliesMutation.mutate()}
                disabled={detectAnomaliesMutation.isPending}
              >
                <ShieldAlert className={`w-4 h-4 mr-1.5 ${detectAnomaliesMutation.isPending ? 'animate-pulse' : ''}`} />
                Run Detection
              </button>
            </div>

            {/* Summary cards */}
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
              <div className="p-4 bg-secondary/50 rounded-xl">
                <div className="flex items-center gap-2 mb-2">
                  <ShieldAlert className="w-4 h-4 text-error" />
                  <span className="text-sm text-muted-foreground">Anomalies</span>
                </div>
                <p className="text-2xl font-bold text-foreground">{anomalies.length}</p>
                <p className="text-xs text-muted-foreground mt-1">
                  {anomalies.filter((a) => !a.acknowledged).length} unacknowledged
                </p>
              </div>
              <div className="p-4 bg-secondary/50 rounded-xl">
                <div className="flex items-center gap-2 mb-2">
                  <Bell className="w-4 h-4 text-warning" />
                  <span className="text-sm text-muted-foreground">Budget Alerts</span>
                </div>
                <p className="text-2xl font-bold text-foreground">{budgetAlerts.length}</p>
                <p className="text-xs text-muted-foreground mt-1">
                  {budgetAlerts.filter((a) => a.severity === 'high' || a.severity === 'critical').length} high severity
                </p>
              </div>
              <div className="p-4 bg-secondary/50 rounded-xl">
                <div className="flex items-center gap-2 mb-2">
                  <Shield className="w-4 h-4 text-reporting" />
                  <span className="text-sm text-muted-foreground">Active Rules</span>
                </div>
                <p className="text-2xl font-bold text-foreground">{anomalyRules.filter((r) => r.enabled).length}</p>
                <p className="text-xs text-muted-foreground mt-1">
                  of {anomalyRules.length} configured
                </p>
              </div>
              <div className="p-4 bg-secondary/50 rounded-xl">
                <div className="flex items-center gap-2 mb-2">
                  <Eye className="w-4 h-4 text-success" />
                  <span className="text-sm text-muted-foreground">Acknowledged</span>
                </div>
                <p className="text-2xl font-bold text-foreground">{anomalies.filter((a) => a.acknowledged).length}</p>
                <p className="text-xs text-muted-foreground mt-1">
                  reviewed by team
                </p>
              </div>
            </div>

            {/* Anomalies Table */}
            <div className="mb-6">
              <h3 className="text-sm font-medium text-foreground mb-3">Detected Anomalies</h3>
              {anomaliesQuery.isError ? (
                <ReportNotice
                  tone="error"
                  title="Anomaly data unavailable"
                  description="The predictive anomalies endpoint failed."
                />
              ) : anomalies.length === 0 && !anomaliesQuery.isLoading ? (
                <ReportNotice
                  title="No anomalies detected"
                  description="Run anomaly detection to scan recent invoices for duplicates, outliers, and volume spikes."
                />
              ) : (
                <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
                  {anomalies.slice(0, 8).map((anomaly) => (
                    <div
                      key={anomaly.id}
                      className="flex items-center justify-between gap-3 p-3 hover:bg-secondary/40 transition-colors"
                    >
                      <div className="min-w-0 flex items-center gap-3">
                        <span className={`text-xs font-medium rounded-full px-2 py-1 ${severityBg(anomaly.severity)}`}>
                          {anomaly.severity}
                        </span>
                        <div>
                          <p className="font-medium text-foreground truncate">
                            {anomalyTypeLabel(anomaly.anomaly_type)}
                          </p>
                          <p className="text-sm text-muted-foreground truncate">
                            {anomaly.entity_id} · {formatDate(anomaly.detected_at)}
                          </p>
                        </div>
                      </div>
                      <div className="flex items-center gap-2 flex-shrink-0">
                        {anomaly.acknowledged ? (
                          <span className="text-xs text-muted-foreground flex items-center gap-1">
                            <EyeOff className="w-3 h-3" /> Acknowledged
                          </span>
                        ) : (
                          <button
                            className="btn btn-secondary btn-sm text-xs"
                            onClick={() => acknowledgeAnomalyMutation.mutate(anomaly.id)}
                            disabled={acknowledgeAnomalyMutation.isPending}
                          >
                            <Eye className="w-3 h-3 mr-1" />
                            Acknowledge
                          </button>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Budget Alerts */}
            <div>
              <h3 className="text-sm font-medium text-foreground mb-3">Budget Alerts</h3>
              {budgetAlertsQuery.isError ? (
                <ReportNotice
                  tone="error"
                  title="Budget alerts unavailable"
                  description="The budget alerts endpoint failed."
                />
              ) : budgetAlerts.length === 0 && !budgetAlertsQuery.isLoading ? (
                <ReportNotice
                  title="No active budget alerts"
                  description="Budget thresholds and concentration risks will appear here when detected."
                />
              ) : (
                <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
                  {budgetAlerts.slice(0, 6).map((alert) => (
                    <div
                      key={alert.id}
                      className="flex items-center justify-between gap-3 p-3 hover:bg-secondary/40 transition-colors"
                    >
                      <div className="min-w-0 flex items-center gap-3">
                        <span className={`text-xs font-medium rounded-full px-2 py-1 ${severityBg(alert.severity)}`}>
                          {alert.severity}
                        </span>
                        <div>
                          <p className="font-medium text-foreground truncate">{alert.title}</p>
                          <p className="text-sm text-muted-foreground truncate">{alert.message}</p>
                        </div>
                      </div>
                      <div className="flex items-center gap-2 flex-shrink-0">
                        {alert.threshold_percentage != null && (
                          <span className="text-sm font-semibold text-foreground">
                            {alert.threshold_percentage.toFixed(1)}%
                          </span>
                        )}
                        <button
                          className="btn btn-secondary btn-sm text-xs"
                          onClick={() => dismissAlertMutation.mutate(alert.id)}
                          disabled={dismissAlertMutation.isPending}
                        >
                          Dismiss
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
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
