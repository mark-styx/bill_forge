'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { vendorsApi } from '@/lib/api';
import { toast } from 'sonner';
import Link from 'next/link';
import {
  ArrowLeft,
  Building2,
  Mail,
  Phone,
  Users,
  Loader2,
  CheckCircle,
} from 'lucide-react';

const vendorTypes = [
  { value: 'business', label: 'Business', description: 'Standard business vendor' },
  { value: 'contractor', label: '1099 Contractor', description: 'Independent contractor' },
  { value: 'employee', label: 'Employee', description: 'For employee reimbursements' },
  { value: 'government', label: 'Government', description: 'Government entity' },
  { value: 'non_profit', label: 'Non-Profit', description: 'Non-profit organization' },
];

export default function NewVendorPage() {
  const router = useRouter();
  const queryClient = useQueryClient();

  const [formData, setFormData] = useState({
    name: '',
    vendor_type: 'business',
    email: '',
    phone: '',
  });

  const createMutation = useMutation({
    mutationFn: (data: typeof formData) => vendorsApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vendors'] });
      toast.success('Vendor created successfully');
      router.push('/vendors');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to create vendor');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate(formData);
  };

  const isValid = formData.name.trim().length > 0;

  return (
    <div className="space-y-6 max-w-2xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/vendors"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Vendors
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Add New Vendor</h1>
        <p className="text-muted-foreground mt-0.5">Create a new vendor record</p>
      </div>

      {/* Form Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-vendor to-vendor/50" />
        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Vendor Name */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">
              <Building2 className="w-4 h-4 inline mr-1.5 text-vendor" />
              Vendor Name <span className="text-error">*</span>
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              className="input"
              placeholder="Enter vendor or company name"
              required
            />
          </div>

          {/* Vendor Type */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-3">
              <Users className="w-4 h-4 inline mr-1.5 text-vendor" />
              Vendor Type <span className="text-error">*</span>
            </label>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              {vendorTypes.map((type) => {
                const isSelected = formData.vendor_type === type.value;
                return (
                  <button
                    key={type.value}
                    type="button"
                    onClick={() => setFormData({ ...formData, vendor_type: type.value })}
                    className={`p-3 rounded-xl border-2 text-left transition-all ${
                      isSelected
                        ? 'border-vendor bg-vendor/5'
                        : 'border-border hover:border-vendor/30'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <p className={`font-medium ${isSelected ? 'text-vendor' : 'text-foreground'}`}>
                          {type.label}
                        </p>
                        <p className="text-xs text-muted-foreground">{type.description}</p>
                      </div>
                      {isSelected && <CheckCircle className="w-5 h-5 text-vendor" />}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Contact Information */}
          <div className="pt-4 border-t border-border">
            <h3 className="text-sm font-medium text-muted-foreground mb-4">Contact Information (Optional)</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  <Mail className="w-4 h-4 inline mr-1.5 text-muted-foreground" />
                  Email
                </label>
                <input
                  type="email"
                  value={formData.email}
                  onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                  className="input"
                  placeholder="vendor@example.com"
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  <Phone className="w-4 h-4 inline mr-1.5 text-muted-foreground" />
                  Phone
                </label>
                <input
                  type="tel"
                  value={formData.phone}
                  onChange={(e) => setFormData({ ...formData, phone: e.target.value })}
                  className="input"
                  placeholder="+1 (555) 000-0000"
                />
              </div>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border">
            <Link href="/vendors" className="btn btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              disabled={!isValid || createMutation.isPending}
              className="btn bg-vendor text-vendor-foreground hover:bg-vendor/90 disabled:opacity-50"
            >
              {createMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <CheckCircle className="w-4 h-4 mr-2" />
                  Create Vendor
                </>
              )}
            </button>
          </div>
        </form>
      </div>

      {/* Help Text */}
      <div className="p-4 bg-vendor/5 border border-vendor/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">What's next?</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-vendor mt-0.5 flex-shrink-0" />
            After creating the vendor, you can add additional details like addresses and banking information
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-vendor mt-0.5 flex-shrink-0" />
            Upload W-9 or other tax documents to keep records complete
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-vendor mt-0.5 flex-shrink-0" />
            Start uploading invoices from this vendor for processing
          </li>
        </ul>
      </div>
    </div>
  );
}
