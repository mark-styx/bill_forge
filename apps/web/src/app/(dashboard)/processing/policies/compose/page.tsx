'use client';

import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { policiesApi } from '@/lib/api';
import type { ComposeResponse, ProposedRule, InvoiceSummary } from '@/lib/api';
import { ApiClientError } from '@/lib/api';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import {
  Sparkles,
  Play,
  Save,
  AlertTriangle,
  CheckCircle,
  Loader2,
  ArrowLeft,
} from 'lucide-react';
import Link from 'next/link';
import { toast } from 'sonner';

const EXAMPLE_POLICIES = [
  'over $5000 require approval from manager',
  'block invoices over $10000 without PO',
  'invoices from vendor Acme Corp need review',
  'route travel to finance',
  'cap monthly spend on software at $5000',
];

export default function PolicyComposerPage() {
  const [text, setText] = useState('');
  const [result, setResult] = useState<ComposeResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const composeMutation = useMutation({
    mutationFn: (policyText: string) => policiesApi.compose(policyText),
    onSuccess: (data) => {
      setResult(data);
      setError(null);
    },
    onError: (err) => {
      if (err instanceof ApiClientError) {
        setError(err.message);
      } else {
        setError('An unexpected error occurred');
      }
      setResult(null);
    },
  });

  const commitMutation = useMutation({
    mutationFn: ({ rule, originalText }: { rule: ProposedRule; originalText: string }) =>
      policiesApi.commit(rule, originalText),
    onSuccess: () => {
      toast.success('Policy saved successfully');
    },
    onError: (err) => {
      if (err instanceof ApiClientError) {
        toast.error(err.message);
      } else {
        toast.error('Failed to save policy');
      }
    },
  });

  const handlePreview = () => {
    if (!text.trim()) return;
    composeMutation.mutate(text.trim());
  };

  const handleSave = () => {
    if (!result) return;
    commitMutation.mutate({
      rule: result.proposed_rule,
      originalText: text.trim(),
    });
  };

  const formatAmount = (cents?: number | null) => {
    if (cents == null) return '-';
    return `$${(cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2 })}`;
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Link href="/processing/workflows" className="text-muted-foreground hover:text-foreground">
          <ArrowLeft className="h-5 w-5" />
        </Link>
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            <Sparkles className="h-6 w-6" />
            Policy Composer
          </h1>
          <p className="text-muted-foreground">
            Describe your AP policy in plain English and preview its impact.
          </p>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Describe Your Policy</CardTitle>
          <CardDescription>
            Type a policy in plain English. Try phrases like:
            &ldquo;over $5000 require approval from manager&rdquo;,
            &ldquo;block invoices over $10000 without PO&rdquo;, or
            &ldquo;cap monthly spend on software at $5000&rdquo;.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Textarea
            placeholder="e.g., over $5000 require approval from manager"
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={3}
            className="resize-none"
            data-testid="policy-text-input"
          />

          <div className="flex flex-wrap gap-2">
            {EXAMPLE_POLICIES.map((ex) => (
              <button
                key={ex}
                type="button"
                className="text-xs px-2 py-1 rounded bg-secondary hover:bg-secondary/80 text-secondary-foreground"
                onClick={() => setText(ex)}
              >
                {ex}
              </button>
            ))}
          </div>

          <Button
            onClick={handlePreview}
            disabled={!text.trim() || composeMutation.isPending}
            data-testid="preview-btn"
          >
            {composeMutation.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
            ) : (
              <Play className="h-4 w-4 mr-2" />
            )}
            Preview
          </Button>

          {error && (
            <div className="flex items-start gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm" data-testid="error-message">
              <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}
        </CardContent>
      </Card>

      {result && (
        <>
          {/* Parsed Rule Card */}
          <Card data-testid="parsed-rule-card">
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <CheckCircle className="h-5 w-5 text-green-500" />
                Parsed Rule
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div>
                <span className="font-medium">{result.proposed_rule.name}</span>
                <p className="text-sm text-muted-foreground">{result.proposed_rule.summary}</p>
              </div>
              <div className="flex items-center gap-2">
                <Badge variant="outline">{result.proposed_rule.guardrail_kind.replace('_', ' ')}</Badge>
                <Badge variant="secondary">Priority: {result.proposed_rule.priority}</Badge>
              </div>
              {result.warnings.length > 0 && (
                <div className="flex items-start gap-2 p-2 rounded bg-yellow-50 dark:bg-yellow-900/20 text-yellow-800 dark:text-yellow-200 text-sm">
                  <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
                  <span>{result.warnings[0]}</span>
                </div>
              )}
            </CardContent>
          </Card>

          {/* 90-Day Preview */}
          <Card data-testid="preview-card">
            <CardHeader>
              <CardTitle>90-Day Impact Preview</CardTitle>
              <CardDescription>
                How this rule would have applied to invoices from the last 90 days.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-3 gap-4">
                <div className="text-center p-3 rounded bg-muted">
                  <div className="text-2xl font-bold" data-testid="matched-count">
                    {result.preview.matched_count}
                  </div>
                  <div className="text-xs text-muted-foreground">Matched</div>
                </div>
                <div className="text-center p-3 rounded bg-muted">
                  <div className="text-2xl font-bold">{result.preview.total_invoices}</div>
                  <div className="text-xs text-muted-foreground">Total Invoices</div>
                </div>
                <div className="text-center p-3 rounded bg-muted">
                  <div className="text-2xl font-bold">
                    {result.preview.total_invoices > 0
                      ? Math.round(
                          (result.preview.matched_count / result.preview.total_invoices) * 100
                        )
                      : 0}
                    %
                  </div>
                  <div className="text-xs text-muted-foreground">Match Rate</div>
                </div>
              </div>

              {result.preview.sample_invoices.length > 0 && (
                <div className="overflow-x-auto">
                  <table className="w-full text-sm" data-testid="sample-table">
                    <thead>
                      <tr className="border-b">
                        <th className="text-left p-2 font-medium">Invoice #</th>
                        <th className="text-left p-2 font-medium">Vendor</th>
                        <th className="text-right p-2 font-medium">Amount</th>
                        <th className="text-left p-2 font-medium">Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.preview.sample_invoices.map((inv: InvoiceSummary) => (
                        <tr key={inv.id} className="border-b last:border-0">
                          <td className="p-2">{inv.invoice_number ?? '-'}</td>
                          <td className="p-2">{inv.vendor_name ?? '-'}</td>
                          <td className="p-2 text-right">{formatAmount(inv.total_amount_cents)}</td>
                          <td className="p-2">
                            <Badge variant="outline">{inv.processing_status ?? '-'}</Badge>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              <Button
                onClick={handleSave}
                disabled={commitMutation.isPending}
                data-testid="save-btn"
              >
                {commitMutation.isPending ? (
                  <Loader2 className="h-4 w-4 animate-spin mr-2" />
                ) : (
                  <Save className="h-4 w-4 mr-2" />
                )}
                Save Policy
              </Button>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
