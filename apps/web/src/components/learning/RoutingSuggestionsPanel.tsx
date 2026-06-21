'use client';

import { useMemo, useState } from 'react';
import Link from 'next/link';
import { useQuery } from '@tanstack/react-query';
import { Sparkles, GitBranch } from 'lucide-react';
import {
  routingPatternsApi,
  type RoutingAmountBucket,
  type RoutingPatternSuggestion,
} from '@/lib/api';

const BUCKET_LABELS: Record<RoutingAmountBucket, string> = {
  under1k: 'under $1k',
  range1k_to5k: 'between $1k and $5k',
  range5k_to25k: 'over $5k',
  over25k: 'over $25k',
};

function describePattern(s: RoutingPatternSuggestion): string {
  const subject =
    s.pattern_key.department ??
    s.pattern_key.vendor_name ??
    'these';
  const amount = BUCKET_LABELS[s.pattern_key.amount_bucket] ?? '';
  const approver = s.dominant_approver_name ?? s.dominant_approver_id;
  return `${s.confidence_pct}% of ${subject} invoices ${amount} are re-routed to ${approver} - update the rule?`;
}

function buildRuleHref(s: RoutingPatternSuggestion): string {
  const params = new URLSearchParams();
  if (s.pattern_key.vendor_id) {
    params.set('vendor_id', s.pattern_key.vendor_id);
  }
  if (s.pattern_key.department) {
    params.set('department', s.pattern_key.department);
  }
  params.set('amount_bucket', s.pattern_key.amount_bucket);
  params.set('approver_id', s.dominant_approver_id);
  params.set('source', 'routing_suggestion');
  return `/processing/assignment-rules/new?${params.toString()}`;
}

export function RoutingSuggestionsPanel() {
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());
  const { data, isLoading, isError } = useQuery({
    queryKey: ['routing-pattern-suggestions'],
    queryFn: () => routingPatternsApi.getSuggestions(),
  });

  const visible = useMemo(() => {
    const all = data?.suggestions ?? [];
    return all.filter((s) => !dismissed.has(suggestionKey(s)));
  }, [data, dismissed]);

  if (isLoading) {
    return (
      <div
        className="rounded-lg border border-border bg-card p-4 text-sm text-muted-foreground"
        data-testid="routing-suggestions-loading"
      >
        Mining recent reroutes for patterns…
      </div>
    );
  }

  if (isError) {
    return (
      <div
        className="rounded-lg border border-border bg-card p-4 text-sm text-muted-foreground"
        data-testid="routing-suggestions-error"
      >
        Unable to load routing suggestions right now.
      </div>
    );
  }

  if (visible.length === 0) {
    return (
      <div
        className="rounded-lg border border-dashed border-border bg-card p-6 text-center"
        data-testid="routing-suggestions-empty"
      >
        <Sparkles className="w-5 h-5 mx-auto text-muted-foreground" />
        <p className="mt-2 text-sm text-muted-foreground">
          No re-routing patterns detected yet. Suggestions will appear here as
          approvers re-route invoices away from the static rules.
        </p>
      </div>
    );
  }

  return (
    <section
      className="rounded-lg border border-border bg-card p-4 space-y-3"
      data-testid="routing-suggestions"
    >
      <header className="flex items-center gap-2">
        <GitBranch className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">
          Learned routing suggestions
        </h3>
      </header>
      <ul className="space-y-2">
        {visible.map((s) => {
          const key = suggestionKey(s);
          return (
            <li
              key={key}
              className="flex items-start justify-between gap-3 rounded-md border border-border bg-background p-3"
              data-testid="routing-suggestion-card"
            >
              <div className="flex-1">
                <p className="text-sm text-foreground">{describePattern(s)}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {s.sample_size} reroutes in the lookback window
                  {s.suggested_action === 'update_rule'
                    ? ' · rule currently routes elsewhere'
                    : ' · no matching rule yet'}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <Link
                  href={buildRuleHref(s)}
                  className="btn btn-primary btn-sm"
                  data-testid="routing-suggestion-update"
                >
                  Update rule
                </Link>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() =>
                    setDismissed((prev) => {
                      const next = new Set(prev);
                      next.add(key);
                      return next;
                    })
                  }
                  data-testid="routing-suggestion-dismiss"
                >
                  Dismiss
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function suggestionKey(s: RoutingPatternSuggestion): string {
  return [
    s.pattern_key.vendor_id ?? 'novendor',
    s.pattern_key.department ?? 'nodept',
    s.pattern_key.amount_bucket,
    s.dominant_approver_id,
  ].join('|');
}
