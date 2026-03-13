'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi } from '@/lib/api';
import { toast } from 'sonner';
import Link from 'next/link';
import {
  ArrowLeft,
  FolderOpen,
  Loader2,
  CheckCircle,
} from 'lucide-react';

const queueTypes = [
  { value: 'review', label: 'Review', description: 'For AP staff to review invoices' },
  { value: 'approval', label: 'Approval', description: 'For managers to approve invoices' },
  { value: 'payment', label: 'Payment', description: 'Invoices ready for payment' },
  { value: 'exception', label: 'Exception', description: 'For handling errors or issues' },
  { value: 'custom', label: 'Custom', description: 'Custom workflow queue' },
];

export default function NewQueuePage() {
  const router = useRouter();
  const queryClient = useQueryClient();

  const [formData, setFormData] = useState({
    name: '',
    description: '',
    queue_type: 'review',
  });

  const createMutation = useMutation({
    mutationFn: (data: any) => workflowsApi.createQueue(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['work-queues'] });
      toast.success('Queue created successfully');
      router.push('/processing/queues');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to create queue');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      name: formData.name,
      description: formData.description || undefined,
      queue_type: formData.queue_type,
      assigned_users: [],
      assigned_roles: [],
      settings: {
        default_sort: 'priority',
      },
    });
  };

  const isValid = formData.name.trim().length > 0;

  return (
    <div className="space-y-6 max-w-2xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/queues"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Queues
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Create New Queue</h1>
        <p className="text-muted-foreground mt-0.5">Set up a new work queue for invoice processing</p>
      </div>

      {/* Form Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Queue Name */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">
              <FolderOpen className="w-4 h-4 inline mr-1.5 text-processing" />
              Queue Name <span className="text-error">*</span>
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              className="input"
              placeholder="e.g., Senior Review Queue"
              required
            />
          </div>

          {/* Description */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">Description</label>
            <textarea
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
              className="input min-h-[80px]"
              placeholder="Describe the purpose of this queue"
            />
          </div>

          {/* Queue Type */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-3">
              Queue Type <span className="text-error">*</span>
            </label>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              {queueTypes.map((type) => {
                const isSelected = formData.queue_type === type.value;
                return (
                  <button
                    key={type.value}
                    type="button"
                    onClick={() => setFormData({ ...formData, queue_type: type.value })}
                    className={`p-3 rounded-xl border-2 text-left transition-all ${
                      isSelected
                        ? 'border-processing bg-processing/5'
                        : 'border-border hover:border-processing/30'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <p className={`font-medium ${isSelected ? 'text-processing' : 'text-foreground'}`}>
                          {type.label}
                        </p>
                        <p className="text-xs text-muted-foreground">{type.description}</p>
                      </div>
                      {isSelected && <CheckCircle className="w-5 h-5 text-processing" />}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border">
            <Link href="/processing/queues" className="btn btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              disabled={!isValid || createMutation.isPending}
              className="btn bg-processing text-processing-foreground hover:bg-processing/90 disabled:opacity-50"
            >
              {createMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <CheckCircle className="w-4 h-4 mr-2" />
                  Create Queue
                </>
              )}
            </button>
          </div>
        </form>
      </div>

      {/* Help Text */}
      <div className="p-4 bg-processing/5 border border-processing/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">Queue Types</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            <b>Review</b> queues are for AP staff to review and validate invoices
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            <b>Approval</b> queues hold invoices waiting for manager sign-off
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            <b>Exception</b> queues catch invoices with OCR errors or other issues
          </li>
        </ul>
      </div>
    </div>
  );
}
