'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { vendorsApi, invoicesApi } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  Users,
  Mail,
  Phone,
  Building2,
  Edit2,
  FileText,
  DollarSign,
  Clock,
  CheckCircle,
  AlertTriangle,
  Trash2,
  Save,
  X,
  CreditCard,
  Calendar,
  Loader2,
} from 'lucide-react';

const statusColors: Record<string, { bg: string; text: string }> = {
  active: { bg: 'bg-success/10', text: 'text-success' },
  inactive: { bg: 'bg-secondary', text: 'text-muted-foreground' },
  on_hold: { bg: 'bg-warning/10', text: 'text-warning' },
  pending: { bg: 'bg-warning/10', text: 'text-warning' },
};

const vendorTypeLabels: Record<string, string> = {
  business: 'Business',
  contractor: '1099 Contractor',
  employee: 'Employee',
  government: 'Government',
  non_profit: 'Non-Profit',
};

export default function VendorDetailPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const id = params.id as string;

  const [isEditing, setIsEditing] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [formData, setFormData] = useState({
    name: '',
    email: '',
    phone: '',
    vendor_type: '',
  });

  const { data: vendor, isLoading } = useQuery({
    queryKey: ['vendor', id],
    queryFn: () => vendorsApi.get(id),
  });

  const { data: invoicesData } = useQuery({
    queryKey: ['vendor-invoices', id],
    queryFn: () => invoicesApi.list({ vendor_id: id } as any),
    enabled: !!vendor,
  });

  const updateMutation = useMutation({
    mutationFn: (data: typeof formData) => vendorsApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vendor', id] });
      queryClient.invalidateQueries({ queryKey: ['vendors'] });
      toast.success('Vendor updated successfully');
      setIsEditing(false);
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update vendor');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => vendorsApi.delete(id),
    onSuccess: () => {
      toast.success('Vendor deleted successfully');
      router.push('/vendors');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete vendor');
    },
  });

  const handleStartEdit = () => {
    if (vendor) {
      setFormData({
        name: vendor.name || '',
        email: vendor.email || '',
        phone: vendor.phone || '',
        vendor_type: vendor.vendor_type || '',
      });
      setIsEditing(true);
    }
  };

  const handleSave = () => {
    updateMutation.mutate(formData);
  };

  const handleDelete = () => {
    deleteMutation.mutate();
  };

  // Calculate invoice stats
  const invoices = invoicesData?.data || [];
  const totalAmount = invoices.reduce((sum: number, inv: any) => sum + (inv.total_amount?.amount || 0), 0);
  const pendingCount = invoices.filter((inv: any) =>
    ['pending', 'pending_approval', 'submitted'].includes(inv.processing_status)
  ).length;

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin" />
          Loading vendor details...
        </div>
      </div>
    );
  }

  if (!vendor) {
    return (
      <div className="text-center py-24">
        <div className="w-16 h-16 rounded-2xl bg-vendor/10 flex items-center justify-center mx-auto mb-4">
          <Users className="w-8 h-8 text-vendor" />
        </div>
        <h2 className="text-xl font-semibold text-foreground mb-2">Vendor not found</h2>
        <p className="text-muted-foreground mb-4">The vendor you're looking for doesn't exist</p>
        <Link href="/vendors" className="btn btn-primary btn-sm">
          Back to Vendors
        </Link>
      </div>
    );
  }

  const status = statusColors[vendor.status] || statusColors.pending;

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/vendors"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Vendors
        </Link>

        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
          <div className="flex items-start gap-4">
            <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-vendor/80 to-vendor flex items-center justify-center text-white text-xl font-bold shadow-lg">
              {vendor.name.charAt(0).toUpperCase()}
            </div>
            <div>
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-semibold text-foreground">{vendor.name}</h1>
                <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${status.bg} ${status.text}`}>
                  {vendor.status}
                </span>
              </div>
              <p className="text-muted-foreground mt-0.5">
                {vendorTypeLabels[vendor.vendor_type] || vendor.vendor_type}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {!isEditing && (
              <>
                <button
                  onClick={handleStartEdit}
                  className="btn btn-secondary btn-sm"
                >
                  <Edit2 className="w-4 h-4 mr-1.5" />
                  Edit
                </button>
                <button
                  onClick={() => setShowDeleteConfirm(true)}
                  className="btn btn-ghost btn-sm text-error hover:bg-error/10"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </>
            )}
          </div>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-vendor/10">
              <FileText className="w-4 h-4 text-vendor" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">{invoices.length}</p>
          <p className="text-sm text-muted-foreground">Total Invoices</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-warning/10">
              <Clock className="w-4 h-4 text-warning" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">{pendingCount}</p>
          <p className="text-sm text-muted-foreground">Pending</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-accent/10">
              <DollarSign className="w-4 h-4 text-accent" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">
            ${(totalAmount / 100).toLocaleString()}
          </p>
          <p className="text-sm text-muted-foreground">Total Volume</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-success/10">
              <CheckCircle className="w-4 h-4 text-success" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">
            {(vendor as any).w9_on_file ? 'Yes' : 'No'}
          </p>
          <p className="text-sm text-muted-foreground">W-9 On File</p>
        </div>
      </div>

      {/* Main Content */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Vendor Details */}
        <div className="lg:col-span-2 space-y-6">
          <div className="card overflow-hidden">
            <div className="h-1 bg-gradient-to-r from-vendor to-vendor/50" />
            <div className="p-6">
              <div className="flex items-center justify-between mb-6">
                <h2 className="text-lg font-semibold text-foreground">Vendor Information</h2>
                {isEditing && (
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setIsEditing(false)}
                      className="btn btn-ghost btn-sm"
                    >
                      <X className="w-4 h-4 mr-1" />
                      Cancel
                    </button>
                    <button
                      onClick={handleSave}
                      disabled={updateMutation.isPending}
                      className="btn bg-vendor text-vendor-foreground hover:bg-vendor/90 btn-sm"
                    >
                      {updateMutation.isPending ? (
                        <Loader2 className="w-4 h-4 mr-1 animate-spin" />
                      ) : (
                        <Save className="w-4 h-4 mr-1" />
                      )}
                      Save
                    </button>
                  </div>
                )}
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-4">
                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <Building2 className="w-3.5 h-3.5" />
                      Company Name
                    </label>
                    {isEditing ? (
                      <input
                        type="text"
                        value={formData.name}
                        onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                        className="input"
                      />
                    ) : (
                      <p className="text-sm font-medium text-foreground">{vendor.name}</p>
                    )}
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <Mail className="w-3.5 h-3.5" />
                      Email
                    </label>
                    {isEditing ? (
                      <input
                        type="email"
                        value={formData.email}
                        onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                        className="input"
                      />
                    ) : (
                      <p className="text-sm font-medium text-foreground">{vendor.email || '—'}</p>
                    )}
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <Phone className="w-3.5 h-3.5" />
                      Phone
                    </label>
                    {isEditing ? (
                      <input
                        type="tel"
                        value={formData.phone}
                        onChange={(e) => setFormData({ ...formData, phone: e.target.value })}
                        className="input"
                      />
                    ) : (
                      <p className="text-sm font-medium text-foreground">{vendor.phone || '—'}</p>
                    )}
                  </div>
                </div>

                <div className="space-y-4">
                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <Users className="w-3.5 h-3.5" />
                      Vendor Type
                    </label>
                    {isEditing ? (
                      <select
                        value={formData.vendor_type}
                        onChange={(e) => setFormData({ ...formData, vendor_type: e.target.value })}
                        className="input"
                      >
                        <option value="business">Business</option>
                        <option value="contractor">1099 Contractor</option>
                        <option value="employee">Employee</option>
                        <option value="government">Government</option>
                        <option value="non_profit">Non-Profit</option>
                      </select>
                    ) : (
                      <p className="text-sm font-medium text-foreground">
                        {vendorTypeLabels[vendor.vendor_type] || vendor.vendor_type}
                      </p>
                    )}
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <CreditCard className="w-3.5 h-3.5" />
                      Payment Terms
                    </label>
                    <p className="text-sm font-medium text-foreground">
                      {(vendor as any).payment_terms || 'Net 30'}
                    </p>
                  </div>

                  <div>
                    <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                      <Calendar className="w-3.5 h-3.5" />
                      W-9 Received
                    </label>
                    <p className="text-sm font-medium text-foreground">
                      {(vendor as any).w9_received_date || '—'}
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Recent Invoices */}
          <div className="card">
            <div className="p-6 border-b border-border">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold text-foreground">Recent Invoices</h2>
                <Link
                  href={`/invoices?vendor_id=${id}`}
                  className="text-sm text-primary hover:underline"
                >
                  View all
                </Link>
              </div>
            </div>
            {invoices.length > 0 ? (
              <div className="divide-y divide-border">
                {invoices.slice(0, 5).map((invoice: any) => (
                  <Link
                    key={invoice.id}
                    href={`/invoices/${invoice.id}`}
                    className="flex items-center justify-between p-4 hover:bg-secondary/50 transition-colors"
                  >
                    <div className="flex items-center gap-3">
                      <div className="p-2 rounded-lg bg-secondary">
                        <FileText className="w-4 h-4 text-muted-foreground" />
                      </div>
                      <div>
                        <p className="font-medium text-foreground">{invoice.invoice_number}</p>
                        <p className="text-sm text-muted-foreground">{invoice.invoice_date || 'No date'}</p>
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="font-medium text-foreground">
                        ${(invoice.total_amount.amount / 100).toLocaleString()}
                      </p>
                      <span className={`text-xs px-2 py-0.5 rounded-full ${
                        invoice.processing_status === 'approved' || invoice.processing_status === 'paid'
                          ? 'bg-success/10 text-success'
                          : invoice.processing_status === 'rejected'
                          ? 'bg-error/10 text-error'
                          : 'bg-warning/10 text-warning'
                      }`}>
                        {invoice.processing_status.replace(/_/g, ' ')}
                      </span>
                    </div>
                  </Link>
                ))}
              </div>
            ) : (
              <div className="p-8 text-center">
                <div className="w-12 h-12 rounded-xl bg-secondary flex items-center justify-center mx-auto mb-3">
                  <FileText className="w-6 h-6 text-muted-foreground" />
                </div>
                <p className="text-sm text-muted-foreground">No invoices yet</p>
              </div>
            )}
          </div>
        </div>

        {/* Sidebar */}
        <div className="space-y-6">
          {/* Quick Actions */}
          <div className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Quick Actions</h2>
            <div className="space-y-2">
              <Link
                href="/invoices/upload"
                className="flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors"
              >
                <div className="p-2 rounded-lg bg-capture/10">
                  <FileText className="w-4 h-4 text-capture" />
                </div>
                <span className="text-sm font-medium text-foreground">Upload Invoice</span>
              </Link>
              <button className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors">
                <div className="p-2 rounded-lg bg-vendor/10">
                  <Mail className="w-4 h-4 text-vendor" />
                </div>
                <span className="text-sm font-medium text-foreground">Send Message</span>
              </button>
              <button className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors">
                <div className="p-2 rounded-lg bg-processing/10">
                  <FileText className="w-4 h-4 text-processing" />
                </div>
                <span className="text-sm font-medium text-foreground">Request W-9</span>
              </button>
            </div>
          </div>

          {/* Notes */}
          <div className="card p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Notes</h2>
            <textarea
              placeholder="Add notes about this vendor..."
              className="input min-h-[100px] resize-none"
              defaultValue={(vendor as any).notes || ''}
            />
            <button className="btn btn-secondary btn-sm w-full mt-3">
              Save Notes
            </button>
          </div>
        </div>
      </div>

      {/* Delete Confirmation Modal */}
      {showDeleteConfirm && (
        <>
          <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50" onClick={() => setShowDeleteConfirm(false)} />
          <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
            <div className="bg-card border border-border rounded-xl shadow-xl max-w-md w-full p-6 animate-scale-in">
              <div className="flex items-center gap-4 mb-4">
                <div className="p-3 rounded-full bg-error/10">
                  <AlertTriangle className="w-6 h-6 text-error" />
                </div>
                <div>
                  <h3 className="text-lg font-semibold text-foreground">Delete Vendor</h3>
                  <p className="text-sm text-muted-foreground">This action cannot be undone</p>
                </div>
              </div>
              <p className="text-sm text-muted-foreground mb-6">
                Are you sure you want to delete <strong className="text-foreground">{vendor.name}</strong>?
                All associated data will be permanently removed.
              </p>
              <div className="flex gap-3 justify-end">
                <button
                  onClick={() => setShowDeleteConfirm(false)}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  onClick={handleDelete}
                  disabled={deleteMutation.isPending}
                  className="btn bg-error text-white hover:bg-error/90"
                >
                  {deleteMutation.isPending ? (
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  ) : (
                    <Trash2 className="w-4 h-4 mr-2" />
                  )}
                  Delete
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
