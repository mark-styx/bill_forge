'use client';

import { useQuery } from '@tanstack/react-query';
import { useParams } from 'next/navigation';
import Link from 'next/link';
import { vendorsApi } from '@/lib/api';
import { ArrowLeft, Users, Mail, Phone, Building, Edit } from 'lucide-react';

export default function VendorDetailPage() {
  const params = useParams();
  const id = params.id as string;

  const { data: vendor, isLoading } = useQuery({
    queryKey: ['vendor', id],
    queryFn: () => vendorsApi.get(id),
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-slate-500">Loading vendor details...</p>
      </div>
    );
  }

  if (!vendor) {
    return (
      <div className="text-center py-12">
        <p className="text-slate-500">Vendor not found</p>
        <Link href="/vendors" className="text-vendor hover:underline mt-4 inline-block">
          Back to vendors
        </Link>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          <Link
            href="/vendors"
            className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </Link>
          <div>
            <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
              {vendor.name}
            </h1>
            <p className="text-slate-500 dark:text-slate-400">
              Vendor Details
            </p>
          </div>
        </div>
        <button className="px-4 py-2 bg-vendor text-white rounded-lg hover:bg-vendor/90 transition-colors flex items-center space-x-2">
          <Edit className="w-4 h-4" />
          <span>Edit Vendor</span>
        </button>
      </div>

      {/* Info Cards */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Main Info */}
        <div className="lg:col-span-2 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
          <div className="p-6 border-b border-slate-200 dark:border-slate-700">
            <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
              Vendor Information
            </h2>
          </div>
          <div className="p-6 space-y-4">
            <div className="flex items-center space-x-3">
              <Building className="w-5 h-5 text-slate-400" />
              <div>
                <p className="text-sm text-slate-500 dark:text-slate-400">Type</p>
                <p className="font-medium text-slate-900 dark:text-white capitalize">
                  {vendor.vendor_type}
                </p>
              </div>
            </div>
            {vendor.email && (
              <div className="flex items-center space-x-3">
                <Mail className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500 dark:text-slate-400">Email</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    {vendor.email}
                  </p>
                </div>
              </div>
            )}
            {vendor.phone && (
              <div className="flex items-center space-x-3">
                <Phone className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500 dark:text-slate-400">Phone</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    {vendor.phone}
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Status Card */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
          <div className="p-6 border-b border-slate-200 dark:border-slate-700">
            <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
              Status
            </h2>
          </div>
          <div className="p-6">
            <div className="flex items-center space-x-3 mb-4">
              <div className="p-3 bg-vendor/10 rounded-lg">
                <Users className="w-6 h-6 text-vendor" />
              </div>
              <div>
                <p className="text-sm text-slate-500 dark:text-slate-400">Current Status</p>
                <span className={`status-badge ${vendor.status === 'active' ? 'status-badge-approved' : 'status-badge-pending'}`}>
                  {vendor.status}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Recent Activity Placeholder */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
        <div className="p-6 border-b border-slate-200 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            Recent Invoices
          </h2>
        </div>
        <div className="p-6">
          <p className="text-slate-500 dark:text-slate-400 text-center py-8">
            Invoice history will appear here
          </p>
        </div>
      </div>
    </div>
  );
}
