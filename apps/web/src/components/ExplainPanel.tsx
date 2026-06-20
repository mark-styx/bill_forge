'use client';

import { useState } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import {
  getCategorizationExplanation,
  submitOverride,
  type ExplanationResponse,
} from '@/lib/api/explain';

export type ExplainDecisionKind = 'categorization';

export interface ExplainPanelProps {
  decisionKind: ExplainDecisionKind;
  invoiceId: string;
  onOverride?: () => void;
}

/**
 * "Show Your Work" panel for an AI decision. Renders the inputs the model
 * saw, top contributing signals with weights, citations to source spans /
 * policy clauses / prior codings, and a deterministic counterfactual.
 *
 * Falls back silently (renders nothing) when the backend returns 404 so
 * surfaces that don't yet have an explanation route degrade gracefully.
 */
export function ExplainPanel({ decisionKind, invoiceId, onOverride }: ExplainPanelProps) {
  const { data, isLoading, isError, error } = useQuery<ExplanationResponse>({
    queryKey: ['explain', decisionKind, invoiceId],
    queryFn: () => getCategorizationExplanation(invoiceId),
    retry: false,
  });

  const [showOverride, setShowOverride] = useState(false);
  const [correctedGl, setCorrectedGl] = useState('');
  const [reason, setReason] = useState('');

  const overrideMutation = useMutation({
    mutationFn: () =>
      submitOverride(invoiceId, {
        corrected_gl_code: correctedGl,
        reason: reason || undefined,
      }),
    onSuccess: () => {
      setShowOverride(false);
      setCorrectedGl('');
      setReason('');
      onOverride?.();
    },
  });

  if (isLoading) {
    return (
      <div className="text-xs text-muted-foreground" data-testid="explain-panel-loading">
        Loading explanation...
      </div>
    );
  }

  // 404 (or any error) — fall back silently so non-categorization surfaces
  // that don't have an explanation endpoint don't show an error.
  if (isError || !data) {
    return (
      <div className="hidden" data-testid="explain-panel-empty" aria-hidden>
        {error instanceof Error ? error.message : null}
      </div>
    );
  }

  return (
    <div className="mt-2 space-y-3 rounded border border-border bg-secondary/40 p-3" data-testid="explain-panel">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-foreground">
          Show your work · current outcome: {data.current_outcome}
        </span>
        <button
          onClick={() => setShowOverride((s) => !s)}
          className="btn btn-sm"
          data-testid="explain-panel-override-toggle"
        >
          Override
        </button>
      </div>

      <section data-testid="explain-section-inputs">
        <h4 className="text-xs font-semibold text-foreground mb-1">Inputs the model saw</h4>
        <dl className="grid grid-cols-2 gap-x-2 gap-y-0.5 text-xs">
          {Object.entries(data.inputs).map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted-foreground truncate">{k}</dt>
              <dd className="text-foreground truncate">
                {typeof v === 'object' ? JSON.stringify(v) : String(v)}
              </dd>
            </div>
          ))}
        </dl>
      </section>

      <section data-testid="explain-section-signals">
        <h4 className="text-xs font-semibold text-foreground mb-1">Top contributing signals</h4>
        <ul className="space-y-1">
          {data.top_signals.map((s) => (
            <li key={s.name} className="text-xs">
              <div className="flex items-center justify-between">
                <span className="text-foreground">
                  <span className="font-mono mr-1">{s.direction}</span>
                  {s.name}
                </span>
                <span className="tabular-nums text-muted-foreground">
                  {Math.round(s.weight * 100)}%
                </span>
              </div>
              <div className="h-1 bg-background rounded mt-0.5">
                <div
                  className="h-1 bg-primary rounded"
                  style={{ width: `${Math.max(2, Math.round(s.weight * 100))}%` }}
                />
              </div>
              <p className="text-muted-foreground mt-0.5">{s.value}</p>
            </li>
          ))}
        </ul>
      </section>

      <section data-testid="explain-section-citations">
        <h4 className="text-xs font-semibold text-foreground mb-1">Citations</h4>
        <ul className="space-y-0.5 text-xs">
          {data.citations.length === 0 ? (
            <li className="text-muted-foreground">No citations available.</li>
          ) : (
            data.citations.map((c, i) => (
              <li key={`${c.kind}-${c.ref}-${i}`} className="text-foreground">
                <span className="text-muted-foreground mr-1">[{c.kind}]</span>
                {c.span}
              </li>
            ))
          )}
        </ul>
      </section>

      <section data-testid="explain-section-counterfactual">
        <h4 className="text-xs font-semibold text-foreground mb-1">Counterfactual</h4>
        <p className="text-xs text-foreground">
          If <span className="font-medium">{data.counterfactual.variable}</span> were{' '}
          <span className="font-medium">{data.counterfactual.alternative}</span>, this would be{' '}
          <span className="font-medium">{data.counterfactual.predicted_outcome}</span>.
        </p>
      </section>

      {showOverride && (
        <form
          data-testid="explain-panel-override-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!correctedGl.trim()) return;
            overrideMutation.mutate();
          }}
          className="space-y-2 border-t border-border pt-2"
        >
          <label className="block text-xs text-muted-foreground">
            Corrected GL code
            <input
              type="text"
              value={correctedGl}
              onChange={(e) => setCorrectedGl(e.target.value)}
              className="input input-sm w-full mt-0.5"
              data-testid="explain-panel-override-gl"
              required
            />
          </label>
          <label className="block text-xs text-muted-foreground">
            Reason (optional)
            <input
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="input input-sm w-full mt-0.5"
              data-testid="explain-panel-override-reason"
            />
          </label>
          <button
            type="submit"
            disabled={overrideMutation.isPending || !correctedGl.trim()}
            className="btn btn-sm bg-primary text-primary-foreground"
            data-testid="explain-panel-override-submit"
          >
            Submit override
          </button>
        </form>
      )}
    </div>
  );
}
