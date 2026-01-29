'use client';

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { vendorsApi } from '@/lib/api';
import {
  Plus,
  Search,
  Filter,
  Users,
  Mail,
  Phone,
  Building2,
  ArrowRight,
} from 'lucide-react';

export default function VendorsPage() {
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState('');

  const { data, isLoading } = useQuery({
    queryKey: ['vendors', page, search],
    queryFn: () => vendorsApi.list({ page, per_page: 25, search }),
  });

  const vendors = data?.data ?? [];

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Vendors</h1>
          <p className="text-muted-foreground mt-0.5">
            Manage your vendor relationships
          </p>
        </div>
        <Link href="/vendors/new" className="btn btn-primary btn-sm">
          <Plus className="w-4 h-4 mr-1.5" />
          Add Vendor
        </Link>
      </div>

      {/* Search & Filters */}
      <div className="card p-4">
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search vendors..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="input pl-9"
            />
          </div>
          <button className="btn btn-secondary btn-sm">
            <Filter className="w-4 h-4 mr-1.5" />
            Filters
          </button>
        </div>
      </div>

      {/* Vendor Grid */}
      {isLoading ? (
        <div className="text-center py-12 text-muted-foreground">
          <div className="flex items-center justify-center gap-2">
            <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
            Loading vendors...
          </div>
        </div>
      ) : vendors.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-vendor/10 flex items-center justify-center mx-auto mb-4">
            <Users className="w-7 h-7 text-vendor" />
          </div>
          <p className="text-foreground font-medium mb-1">No vendors found</p>
          <p className="text-sm text-muted-foreground mb-4">
            Get started by adding your first vendor
          </p>
          <Link href="/vendors/new" className="btn btn-primary btn-sm inline-flex">
            <Plus className="w-4 h-4 mr-1.5" />
            Add Vendor
          </Link>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {vendors.map((vendor, index) => (
            <Link
              key={vendor.id}
              href={`/vendors/${vendor.id}`}
              className="card card-hover p-5 group animate-slide-up"
              style={{ animationDelay: `${index * 30}ms` }}
            >
              <div className="flex items-start justify-between mb-4">
                <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-vendor/80 to-vendor flex items-center justify-center text-white font-semibold">
                  {vendor.name.charAt(0).toUpperCase()}
                </div>
                <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${
                  vendor.status === 'active'
                    ? 'bg-success/10 text-success'
                    : 'bg-warning/10 text-warning'
                }`}>
                  {vendor.status}
                </span>
              </div>

              <h3 className="font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">
                {vendor.name}
              </h3>

              <div className="space-y-1.5 mb-4">
                {vendor.email && (
                  <p className="text-sm text-muted-foreground flex items-center gap-2">
                    <Mail className="w-3.5 h-3.5" />
                    {vendor.email}
                  </p>
                )}
                {vendor.phone && (
                  <p className="text-sm text-muted-foreground flex items-center gap-2">
                    <Phone className="w-3.5 h-3.5" />
                    {vendor.phone}
                  </p>
                )}
              </div>

              <div className="flex items-center justify-between pt-4 border-t border-border">
                <span className="text-xs text-muted-foreground flex items-center gap-1.5">
                  <Building2 className="w-3.5 h-3.5" />
                  {vendor.vendor_type}
                </span>
                <span className="text-sm text-primary group-hover:translate-x-1 transition-transform flex items-center gap-1">
                  View
                  <ArrowRight className="w-3.5 h-3.5" />
                </span>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
