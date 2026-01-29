'use client';

import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import {
  Layers,
  Plus,
  Search,
  ChevronRight,
  AlertCircle,
  CheckCircle,
  Clock,
  CreditCard,
  Filter,
  ArrowRight,
  FolderOpen,
  Loader2,
} from 'lucide-react';
import { useState } from 'react';

const queueTypeConfig: Record<string, { icon: typeof AlertCircle; color: string; bg: string; border: string }> = {
  exception: {
    icon: AlertCircle,
    color: 'text-error',
    bg: 'bg-error/10',
    border: 'border-error/20',
  },
  review: {
    icon: Clock,
    color: 'text-processing',
    bg: 'bg-processing/10',
    border: 'border-processing/20',
  },
  approval: {
    icon: CheckCircle,
    color: 'text-warning',
    bg: 'bg-warning/10',
    border: 'border-warning/20',
  },
  payment: {
    icon: CreditCard,
    color: 'text-success',
    bg: 'bg-success/10',
    border: 'border-success/20',
  },
  custom: {
    icon: Layers,
    color: 'text-vendor',
    bg: 'bg-vendor/10',
    border: 'border-vendor/20',
  },
};

const flowStages = [
  { name: 'OCR Error', color: 'bg-error/10 text-error border-error/20' },
  { name: 'AP Queue', color: 'bg-processing/10 text-processing border-processing/20' },
  { name: 'Pending Approval', color: 'bg-warning/10 text-warning border-warning/20' },
  { name: 'Ready for Payment', color: 'bg-success/10 text-success border-success/20' },
];

export default function WorkQueuesPage() {
  const [searchQuery, setSearchQuery] = useState('');

  const { data: queues, isLoading } = useQuery({
    queryKey: ['work-queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  // Sort queues by type for a logical flow: exception -> review -> approval -> payment
  const sortedQueues = queues?.sort((a: any, b: any) => {
    const order = ['exception', 'review', 'approval', 'payment', 'custom'];
    return order.indexOf(a.queue_type) - order.indexOf(b.queue_type);
  });

  const filteredQueues = sortedQueues?.filter((queue: any) => {
    if (!searchQuery) return true;
    const query = searchQuery.toLowerCase();
    return (
      queue.name.toLowerCase().includes(query) ||
      queue.description?.toLowerCase().includes(query) ||
      queue.queue_type.toLowerCase().includes(query)
    );
  });

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Work Queues</h1>
          <p className="text-muted-foreground mt-0.5">
            Manage invoice workflow and processing queues
          </p>
        </div>
        <Link href="/processing/queues/new" className="btn btn-primary btn-sm">
          <Plus className="w-4 h-4 mr-1.5" />
          Create Queue
        </Link>
      </div>

      {/* Queue Flow Diagram */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
        <div className="p-6">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-4">
            Queue Flow
          </h2>
          <div className="flex items-center justify-between overflow-x-auto pb-2 gap-2">
            {flowStages.map((stage, idx) => (
              <div key={stage.name} className="flex items-center">
                <div className={`px-4 py-2.5 rounded-xl border text-sm font-medium whitespace-nowrap ${stage.color}`}>
                  {stage.name}
                </div>
                {idx < flowStages.length - 1 && (
                  <ArrowRight className="w-5 h-5 text-muted-foreground mx-2 flex-shrink-0" />
                )}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Search & Filter */}
      <div className="card p-4">
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search queues..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="input pl-9"
            />
          </div>
          <button className="btn btn-secondary">
            <Filter className="w-4 h-4 mr-1.5" />
            Filters
          </button>
        </div>
      </div>

      {/* Queues Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
        {isLoading ? (
          // Loading skeleton
          [...Array(4)].map((_, i) => (
            <div key={i} className="card p-6 animate-pulse">
              <div className="flex items-center gap-3 mb-4">
                <div className="w-10 h-10 bg-secondary rounded-lg" />
                <div className="flex-1">
                  <div className="h-4 bg-secondary rounded w-2/3 mb-2" />
                  <div className="h-3 bg-secondary rounded w-1/2" />
                </div>
              </div>
              <div className="h-4 bg-secondary rounded w-3/4" />
            </div>
          ))
        ) : !filteredQueues || filteredQueues.length === 0 ? (
          <div className="col-span-full">
            <div className="card p-12 text-center">
              <div className="w-14 h-14 rounded-xl bg-processing/10 flex items-center justify-center mx-auto mb-4">
                <FolderOpen className="w-7 h-7 text-processing" />
              </div>
              <h3 className="text-lg font-semibold text-foreground mb-2">
                {searchQuery ? 'No Matching Queues' : 'No Work Queues'}
              </h3>
              <p className="text-muted-foreground mb-4 max-w-sm mx-auto">
                {searchQuery
                  ? 'No queues match your search criteria. Try adjusting your search.'
                  : 'Create your first work queue to start organizing invoice processing.'}
              </p>
              {!searchQuery && (
                <Link href="/processing/queues/new" className="btn btn-primary btn-sm inline-flex">
                  <Plus className="w-4 h-4 mr-1.5" />
                  Create your first queue
                </Link>
              )}
            </div>
          </div>
        ) : (
          filteredQueues.map((queue: any, index: number) => {
            const config = queueTypeConfig[queue.queue_type] || queueTypeConfig.custom;
            const Icon = config.icon;

            return (
              <Link
                key={queue.id}
                href={`/processing/queues/${queue.id}`}
                className="card card-hover overflow-hidden group animate-slide-up"
                style={{ animationDelay: `${index * 50}ms` }}
              >
                <div className={`h-1 ${config.bg.replace('/10', '')}`} />
                <div className="p-5">
                  <div className="flex items-center justify-between mb-4">
                    <div className={`p-2.5 rounded-xl ${config.bg}`}>
                      <Icon className={`w-5 h-5 ${config.color}`} />
                    </div>
                    {queue.is_default && (
                      <span className="px-2 py-0.5 bg-primary/10 text-primary text-xs font-medium rounded-full">
                        Default
                      </span>
                    )}
                  </div>

                  <h3 className="text-lg font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">
                    {queue.name}
                  </h3>
                  <p className="text-sm text-muted-foreground mb-4 line-clamp-2">
                    {queue.description || 'No description'}
                  </p>

                  <div className="flex items-center justify-between pt-3 border-t border-border">
                    <span className={`text-xs font-medium px-2 py-1 rounded-full ${config.bg} ${config.color} capitalize`}>
                      {queue.queue_type}
                    </span>
                    <ChevronRight className="w-4 h-4 text-muted-foreground group-hover:text-primary group-hover:translate-x-1 transition-all" />
                  </div>
                </div>
              </Link>
            );
          })
        )}
      </div>

      {/* Assignment Rules Link */}
      <div className="card p-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="p-3 rounded-xl bg-processing/10">
              <Layers className="w-6 h-6 text-processing" />
            </div>
            <div>
              <h2 className="font-semibold text-foreground">Assignment Rules</h2>
              <p className="text-sm text-muted-foreground">
                Configure automatic invoice routing and assignment
              </p>
            </div>
          </div>
          <Link href="/processing/assignment-rules" className="btn btn-secondary">
            Manage Rules
            <ArrowRight className="w-4 h-4 ml-1.5" />
          </Link>
        </div>
      </div>

      {/* Help Section */}
      <div className="p-4 bg-processing/5 border border-processing/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">About Work Queues</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Work queues organize invoices by processing stage for efficient workflow
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Use assignment rules to automatically route invoices to the right queues
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Team members can claim items from queues to work on them
          </li>
        </ul>
      </div>
    </div>
  );
}
