'use client';

import { useState } from 'react';
import Link from 'next/link';
import { useMutation } from '@tanstack/react-query';
import { invoicesApi } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  Download,
  FileText,
  Calendar,
  FileSpreadsheet,
  FileJson,
  Users,
  ClipboardList,
  CheckCircle,
  Loader2,
  Clock,
  BarChart3,
} from 'lucide-react';

const exportTypes = [
  {
    id: 'invoices',
    name: 'Invoices',
    description: 'All invoice data with amounts, dates, and status',
    icon: FileText,
    color: 'capture',
  },
  {
    id: 'vendors',
    name: 'Vendors',
    description: 'Vendor profiles and contact information',
    icon: Users,
    color: 'vendor',
  },
  {
    id: 'payments',
    name: 'Payments',
    description: 'Payment history and transaction records',
    icon: BarChart3,
    color: 'processing',
  },
  {
    id: 'audit_log',
    name: 'Audit Log',
    description: 'System activity and user actions',
    icon: ClipboardList,
    color: 'reporting',
  },
];

const dateRanges = [
  { id: 'last_7_days', name: 'Last 7 days' },
  { id: 'last_30_days', name: 'Last 30 days' },
  { id: 'last_90_days', name: 'Last 90 days' },
  { id: 'this_year', name: 'This year' },
  { id: 'all_time', name: 'All time' },
];

const formats = [
  { id: 'csv', name: 'CSV', icon: FileSpreadsheet, description: 'Comma-separated values' },
  { id: 'xlsx', name: 'Excel', icon: FileSpreadsheet, description: 'Microsoft Excel format' },
  { id: 'json', name: 'JSON', icon: FileJson, description: 'JavaScript Object Notation' },
];

// Simulated recent exports
const recentExports = [
  { id: '1', type: 'invoices', date: '2024-01-15', format: 'csv', size: '245 KB' },
  { id: '2', type: 'vendors', date: '2024-01-10', format: 'xlsx', size: '128 KB' },
];

export default function ExportPage() {
  const [exportType, setExportType] = useState('invoices');
  const [dateRange, setDateRange] = useState('last_30_days');
  const [format, setFormat] = useState('csv');
  const [isExporting, setIsExporting] = useState(false);

  const exportMutation = useMutation({
    mutationFn: async () => {
      setIsExporting(true);
      // Simulate export delay
      await new Promise((resolve) => setTimeout(resolve, 2000));
      // In real implementation, call the appropriate API
      // if (exportType === 'invoices') return invoicesApi.exportCsv();
      return { success: true };
    },
    onSuccess: () => {
      toast.success(`${exportType.charAt(0).toUpperCase() + exportType.slice(1)} exported successfully`);
      setIsExporting(false);
    },
    onError: (error: any) => {
      toast.error(error.message || 'Export failed');
      setIsExporting(false);
    },
  });

  const handleExport = () => {
    exportMutation.mutate();
  };

  const selectedType = exportTypes.find((t) => t.id === exportType);

  return (
    <div className="space-y-6 max-w-4xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/reports"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Reports
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Export Data</h1>
        <p className="text-muted-foreground mt-0.5">
          Download your data in various formats for analysis or backup
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Export Configuration */}
        <div className="lg:col-span-2 space-y-6">
          {/* Data Type Selection */}
          <div className="card overflow-hidden">
            <div className="h-1 bg-gradient-to-r from-reporting to-reporting/50" />
            <div className="p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">
                <FileText className="w-5 h-5 inline mr-2 text-reporting" />
                Select Data Type
              </h2>
              <div className="grid grid-cols-2 gap-3">
                {exportTypes.map((type) => {
                  const isSelected = exportType === type.id;
                  return (
                    <button
                      key={type.id}
                      onClick={() => setExportType(type.id)}
                      className={`p-4 rounded-xl border-2 text-left transition-all ${
                        isSelected
                          ? `border-${type.color} bg-${type.color}/5`
                          : 'border-border hover:border-primary/30'
                      }`}
                    >
                      <div className="flex items-start gap-3">
                        <div className={`p-2 rounded-lg bg-${type.color}/10`}>
                          <type.icon className={`w-5 h-5 text-${type.color}`} />
                        </div>
                        <div className="flex-1 min-w-0">
                          <p className={`font-medium ${isSelected ? 'text-foreground' : 'text-foreground'}`}>
                            {type.name}
                          </p>
                          <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                            {type.description}
                          </p>
                        </div>
                        {isSelected && (
                          <CheckCircle className="w-5 h-5 text-primary flex-shrink-0" />
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          </div>

          {/* Date Range */}
          <div className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">
              <Calendar className="w-5 h-5 inline mr-2 text-reporting" />
              Date Range
            </h2>
            <select
              value={dateRange}
              onChange={(e) => setDateRange(e.target.value)}
              className="input"
            >
              {dateRanges.map((range) => (
                <option key={range.id} value={range.id}>
                  {range.name}
                </option>
              ))}
            </select>
            <p className="text-sm text-muted-foreground mt-2">
              Data will be filtered based on the selected time period
            </p>
          </div>

          {/* Format Selection */}
          <div className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">
              <FileSpreadsheet className="w-5 h-5 inline mr-2 text-reporting" />
              Export Format
            </h2>
            <div className="flex flex-wrap gap-3">
              {formats.map((f) => {
                const isSelected = format === f.id;
                return (
                  <button
                    key={f.id}
                    onClick={() => setFormat(f.id)}
                    className={`flex items-center gap-3 px-4 py-3 rounded-xl border-2 transition-all ${
                      isSelected
                        ? 'border-reporting bg-reporting/5'
                        : 'border-border hover:border-primary/30'
                    }`}
                  >
                    <f.icon className={`w-5 h-5 ${isSelected ? 'text-reporting' : 'text-muted-foreground'}`} />
                    <div className="text-left">
                      <p className={`font-medium ${isSelected ? 'text-foreground' : 'text-foreground'}`}>
                        {f.name}
                      </p>
                      <p className="text-xs text-muted-foreground">{f.description}</p>
                    </div>
                    {isSelected && <CheckCircle className="w-5 h-5 text-reporting ml-2" />}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Export Button */}
          <button
            onClick={handleExport}
            disabled={isExporting}
            className="btn bg-reporting text-reporting-foreground hover:bg-reporting/90 w-full py-3"
          >
            {isExporting ? (
              <>
                <Loader2 className="w-5 h-5 mr-2 animate-spin" />
                Preparing Export...
              </>
            ) : (
              <>
                <Download className="w-5 h-5 mr-2" />
                Export {selectedType?.name} as {format.toUpperCase()}
              </>
            )}
          </button>
        </div>

        {/* Sidebar */}
        <div className="space-y-6">
          {/* Export Summary */}
          <div className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Export Summary</h2>
            <div className="space-y-3">
              <div className="flex items-center justify-between py-2 border-b border-border">
                <span className="text-sm text-muted-foreground">Data Type</span>
                <span className="text-sm font-medium text-foreground capitalize">{exportType.replace('_', ' ')}</span>
              </div>
              <div className="flex items-center justify-between py-2 border-b border-border">
                <span className="text-sm text-muted-foreground">Date Range</span>
                <span className="text-sm font-medium text-foreground">
                  {dateRanges.find((r) => r.id === dateRange)?.name}
                </span>
              </div>
              <div className="flex items-center justify-between py-2">
                <span className="text-sm text-muted-foreground">Format</span>
                <span className="text-sm font-medium text-foreground uppercase">{format}</span>
              </div>
            </div>
          </div>

          {/* Recent Exports */}
          <div className="card">
            <div className="p-4 border-b border-border">
              <h2 className="font-semibold text-foreground">Recent Exports</h2>
            </div>
            {recentExports.length > 0 ? (
              <div className="divide-y divide-border">
                {recentExports.map((exp) => (
                  <div key={exp.id} className="p-4 flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <div className="p-2 rounded-lg bg-secondary">
                        <FileSpreadsheet className="w-4 h-4 text-muted-foreground" />
                      </div>
                      <div>
                        <p className="text-sm font-medium text-foreground capitalize">
                          {exp.type.replace('_', ' ')}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {exp.date} · {exp.format.toUpperCase()} · {exp.size}
                        </p>
                      </div>
                    </div>
                    <button className="p-2 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors">
                      <Download className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="p-8 text-center">
                <Clock className="w-10 h-10 text-muted-foreground mx-auto mb-3" />
                <p className="text-sm text-muted-foreground">No recent exports</p>
              </div>
            )}
          </div>

          {/* Help */}
          <div className="p-4 bg-reporting/5 border border-reporting/20 rounded-xl">
            <h3 className="font-medium text-foreground mb-2">Need help?</h3>
            <p className="text-sm text-muted-foreground">
              Exported files can be opened in Excel, Google Sheets, or any compatible application.
              JSON format is ideal for programmatic access.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
