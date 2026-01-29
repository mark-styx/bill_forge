'use client';

import { useState } from 'react';
import Link from 'next/link';
import { ArrowLeft, Download, FileText, Calendar, Filter } from 'lucide-react';

export default function ExportPage() {
  const [exportType, setExportType] = useState('invoices');
  const [dateRange, setDateRange] = useState('last_30_days');
  const [format, setFormat] = useState('csv');

  const handleExport = () => {
    // In a real app, this would trigger a download
    alert('Export functionality would download a ' + format.toUpperCase() + ' file');
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center space-x-4">
        <Link
          href="/reports"
          className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </Link>
        <div>
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
            Export Data
          </h1>
          <p className="text-slate-500 dark:text-slate-400">
            Download your data in various formats
          </p>
        </div>
      </div>

      {/* Export Options */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6">
        <div className="max-w-xl space-y-6">
          {/* Data Type */}
          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-3">
              <FileText className="w-4 h-4 inline mr-2" />
              Data Type
            </label>
            <div className="grid grid-cols-2 gap-3">
              {[
                { value: 'invoices', label: 'Invoices' },
                { value: 'vendors', label: 'Vendors' },
                { value: 'payments', label: 'Payments' },
                { value: 'audit_log', label: 'Audit Log' },
              ].map((option) => (
                <button
                  key={option.value}
                  onClick={() => setExportType(option.value)}
                  className={`p-3 rounded-lg border text-left transition-colors ${
                    exportType === option.value
                      ? 'border-reporting bg-reporting/10 text-reporting'
                      : 'border-slate-200 dark:border-slate-700 hover:border-slate-300 dark:hover:border-slate-600'
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          {/* Date Range */}
          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-3">
              <Calendar className="w-4 h-4 inline mr-2" />
              Date Range
            </label>
            <select
              value={dateRange}
              onChange={(e) => setDateRange(e.target.value)}
              className="w-full px-4 py-2 border border-slate-200 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-700 text-slate-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-reporting/50"
            >
              <option value="last_7_days">Last 7 days</option>
              <option value="last_30_days">Last 30 days</option>
              <option value="last_90_days">Last 90 days</option>
              <option value="this_year">This year</option>
              <option value="all_time">All time</option>
            </select>
          </div>

          {/* Format */}
          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-3">
              <Filter className="w-4 h-4 inline mr-2" />
              Export Format
            </label>
            <div className="flex space-x-3">
              {[
                { value: 'csv', label: 'CSV' },
                { value: 'xlsx', label: 'Excel' },
                { value: 'json', label: 'JSON' },
              ].map((option) => (
                <button
                  key={option.value}
                  onClick={() => setFormat(option.value)}
                  className={`px-6 py-2 rounded-lg border transition-colors ${
                    format === option.value
                      ? 'border-reporting bg-reporting text-white'
                      : 'border-slate-200 dark:border-slate-700 hover:border-slate-300 dark:hover:border-slate-600'
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          {/* Export Button */}
          <div className="pt-4">
            <button
              onClick={handleExport}
              className="px-6 py-3 bg-reporting text-white rounded-lg hover:bg-reporting/90 transition-colors flex items-center space-x-2"
            >
              <Download className="w-5 h-5" />
              <span>Export {exportType.replace('_', ' ')}</span>
            </button>
          </div>
        </div>
      </div>

      {/* Recent Exports */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
        <div className="p-6 border-b border-slate-200 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            Recent Exports
          </h2>
        </div>
        <div className="p-6">
          <p className="text-slate-500 dark:text-slate-400 text-center py-8">
            Your recent exports will appear here
          </p>
        </div>
      </div>
    </div>
  );
}
