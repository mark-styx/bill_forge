'use client';

import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import {
  ClipboardCheck,
  Clock,
  CheckCircle,
  XCircle,
  ArrowRight,
  Layers,
  AlertCircle,
  TrendingUp,
} from 'lucide-react';

export default function ProcessingPage() {
  const { data: approvals, isLoading } = useQuery({
    queryKey: ['pending-approvals'],
    queryFn: () => workflowsApi.listPendingApprovals(),
  });

  const stats = [
    { name: 'Pending Approval', count: approvals?.length ?? 0, icon: Clock, color: 'text-warning', bg: 'bg-warning/10' },
    { name: 'Approved Today', count: 12, icon: CheckCircle, color: 'text-success', bg: 'bg-success/10' },
    { name: 'Rejected Today', count: 2, icon: XCircle, color: 'text-error', bg: 'bg-error/10' },
    { name: 'In Queues', count: 25, icon: Layers, color: 'text-primary', bg: 'bg-primary/10' },
  ];

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Invoice Processing</h1>
          <p className="text-muted-foreground mt-0.5">
            Manage approvals and workflow queues
          </p>
        </div>
        <Link href="/processing/queues" className="btn btn-primary btn-sm">
          <Layers className="w-4 h-4 mr-1.5" />
          View All Queues
        </Link>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {stats.map((stat, index) => (
          <div
            key={stat.name}
            className="stat-card animate-slide-up"
            style={{ animationDelay: `${index * 50}ms` }}
          >
            <div className={`p-2 rounded-lg ${stat.bg} w-fit`}>
              <stat.icon className={`w-5 h-5 ${stat.color}`} />
            </div>
            <p className="stat-value mt-3">{stat.count}</p>
            <p className="stat-label">{stat.name}</p>
          </div>
        ))}
      </div>

      {/* Main Content */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Pending Approvals */}
        <div className="card overflow-hidden">
          <div className="h-1 bg-gradient-to-r from-warning to-warning/50" />
          <div className="p-5 border-b border-border flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-warning/10">
                <AlertCircle className="w-5 h-5 text-warning" />
              </div>
              <h2 className="font-semibold text-foreground">Pending Approvals</h2>
            </div>
            <Link
              href="/processing/approvals"
              className="text-sm text-primary hover:underline flex items-center gap-1"
            >
              View all
              <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
          <div className="p-5">
            {isLoading ? (
              <div className="text-center py-8 text-muted-foreground">
                <div className="flex items-center justify-center gap-2">
                  <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
                  Loading...
                </div>
              </div>
            ) : !approvals || approvals.length === 0 ? (
              <div className="text-center py-8">
                <div className="w-12 h-12 rounded-xl bg-success/10 flex items-center justify-center mx-auto mb-3">
                  <CheckCircle className="w-6 h-6 text-success" />
                </div>
                <p className="font-medium text-foreground mb-1">All caught up!</p>
                <p className="text-sm text-muted-foreground">No pending approvals</p>
              </div>
            ) : (
              <div className="space-y-3">
                {approvals.slice(0, 5).map((approval) => (
                  <div
                    key={approval.id}
                    className="flex items-center justify-between p-3 bg-secondary/50 rounded-lg hover:bg-secondary transition-colors"
                  >
                    <div>
                      <p className="font-medium text-foreground">
                        Invoice #{approval.invoice_id.slice(0, 8)}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Requested {new Date(approval.created_at).toLocaleDateString()}
                      </p>
                    </div>
                    <Link
                      href={`/processing/approvals/${approval.id}`}
                      className="btn btn-primary btn-sm"
                    >
                      Review
                    </Link>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Work Queues */}
        <div className="card overflow-hidden">
          <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
          <div className="p-5 border-b border-border flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-processing/10">
                <Layers className="w-5 h-5 text-processing" />
              </div>
              <h2 className="font-semibold text-foreground">Work Queues</h2>
            </div>
            <Link
              href="/processing/queues"
              className="text-sm text-primary hover:underline flex items-center gap-1"
            >
              Manage
              <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
          <div className="p-5 space-y-3">
            {[
              { name: 'Initial Review', count: 12, icon: Clock, color: 'text-warning', bg: 'bg-warning/10', href: '/processing/queues/review' },
              { name: 'Manager Approval', count: 5, icon: ClipboardCheck, color: 'text-primary', bg: 'bg-primary/10', href: '/processing/queues/approval' },
              { name: 'Ready for Payment', count: 8, icon: CheckCircle, color: 'text-success', bg: 'bg-success/10', href: '/processing/queues/payment' },
            ].map((queue) => (
              <Link
                key={queue.name}
                href={queue.href}
                className="flex items-center justify-between p-3 bg-secondary/50 rounded-lg hover:bg-secondary transition-colors group"
              >
                <div className="flex items-center gap-3">
                  <div className={`p-2 rounded-lg ${queue.bg}`}>
                    <queue.icon className={`w-4 h-4 ${queue.color}`} />
                  </div>
                  <div>
                    <p className="font-medium text-foreground group-hover:text-primary transition-colors">
                      {queue.name}
                    </p>
                    <p className="text-xs text-muted-foreground">{queue.count} items</p>
                  </div>
                </div>
                <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-primary group-hover:translate-x-1 transition-all" />
              </Link>
            ))}
          </div>
        </div>
      </div>

      {/* Quick Actions */}
      <div className="card p-5">
        <h2 className="font-semibold text-foreground mb-4">Quick Actions</h2>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          <Link href="/processing/assignment-rules" className="action-card group">
            <div className="action-card-icon bg-processing/10">
              <TrendingUp className="w-5 h-5 text-processing" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Assignment Rules</p>
              <p className="text-sm text-muted-foreground">Configure routing</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
          </Link>
          <Link href="/processing/approval-limits" className="action-card group">
            <div className="action-card-icon bg-warning/10">
              <AlertCircle className="w-5 h-5 text-warning" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Approval Limits</p>
              <p className="text-sm text-muted-foreground">Set thresholds</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
          </Link>
          <Link href="/processing/delegations" className="action-card group">
            <div className="action-card-icon bg-primary/10">
              <ClipboardCheck className="w-5 h-5 text-primary" />
            </div>
            <div className="flex-1">
              <p className="font-medium text-foreground">Delegations</p>
              <p className="text-sm text-muted-foreground">Manage approvers</p>
            </div>
            <ArrowRight className="w-4 h-4 text-muted-foreground group-hover:text-foreground group-hover:translate-x-1 transition-all" />
          </Link>
        </div>
      </div>
    </div>
  );
}
