'use client';

import { useQuery } from '@tanstack/react-query';
import {
  learningApi,
  type LearningCorrectionType,
  type LearningWeeklyInsightsResponse,
} from '@/lib/api';
import { BookOpen, GitBranch, Layers, Sparkles, TrendingUp } from 'lucide-react';

const CORRECTION_LABELS: Record<LearningCorrectionType, string> = {
  gl_recode: 'GL recodes',
  approver_reroute: 'Approver reroutes',
  autopilot_override: 'Autopilot overrides',
  duplicate_dismissal: 'Duplicate dismissals',
};

interface Props {
  weekStart?: string;
}

export function WeeklyLearningPanel({ weekStart }: Props) {
  const { data, isLoading, isError } = useQuery<LearningWeeklyInsightsResponse>({
    queryKey: ['learning', 'weekly-insights', weekStart ?? 'current'],
    queryFn: () => learningApi.getWeeklyInsights(weekStart),
  });

  if (isLoading) {
    return (
      <div className="rounded-lg border border-border bg-card p-6 text-sm text-muted-foreground">
        Loading what I learned this week…
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="rounded-lg border border-border bg-card p-6 text-sm text-muted-foreground">
        Unable to load the weekly learning summary right now.
      </div>
    );
  }

  const insights = data.insights;
  const totalCorrections =
    insights.corrections_ingested.gl_recode +
    insights.corrections_ingested.approver_reroute +
    insights.corrections_ingested.autopilot_override +
    insights.corrections_ingested.duplicate_dismissal;

  return (
    <div className="space-y-6">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-xl font-semibold text-foreground flex items-center gap-2">
            <BookOpen className="w-5 h-5" />
            What I learned this week
          </h2>
          <p className="text-sm text-muted-foreground mt-0.5">
            Continuous learning summary for week of {data.week_start}.
          </p>
        </div>
        <div className="rounded-md bg-muted px-3 py-1 text-xs uppercase tracking-wide text-muted-foreground">
          {totalCorrections} corrections ingested
        </div>
      </header>

      {totalCorrections === 0 ? (
        <div
          className="rounded-lg border border-dashed border-border bg-card p-8 text-center"
          data-testid="weekly-learning-empty"
        >
          <Sparkles className="w-6 h-6 mx-auto text-muted-foreground" />
          <p className="mt-3 text-sm text-muted-foreground">
            No corrections this week — the model has nothing new to learn yet.
            As you re-code GL lines, reroute approvers, or override autopilot
            decisions, this panel will fill in.
          </p>
        </div>
      ) : (
        <>
          <section
            className="grid grid-cols-2 md:grid-cols-4 gap-3"
            data-testid="corrections-by-kind"
          >
            {(Object.keys(CORRECTION_LABELS) as LearningCorrectionType[]).map(
              (kind) => (
                <div
                  key={kind}
                  className="rounded-lg border border-border bg-card p-4"
                >
                  <div className="text-xs uppercase tracking-wide text-muted-foreground">
                    {CORRECTION_LABELS[kind]}
                  </div>
                  <div className="mt-1 text-2xl font-semibold text-foreground">
                    {insights.corrections_ingested[kind]}
                  </div>
                </div>
              ),
            )}
          </section>

          <section data-testid="model-changes">
            <h3 className="text-sm font-semibold text-foreground flex items-center gap-1.5 mb-2">
              <Layers className="w-4 h-4" />
              Model updates
            </h3>
            <div className="rounded-lg border border-border bg-card divide-y divide-border">
              {insights.model_changes.map((change) => (
                <div
                  key={`${change.model_kind}-${change.version}`}
                  className="flex items-center justify-between p-3"
                >
                  <div>
                    <div className="text-sm font-medium text-foreground">
                      {change.model_kind}{' '}
                      <span className="text-muted-foreground">
                        v{change.version}
                      </span>
                    </div>
                    {change.note ? (
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {change.note}
                      </div>
                    ) : null}
                  </div>
                  <div className="text-right">
                    <div className="text-xs text-muted-foreground">
                      {change.corrections_applied} corrections
                    </div>
                    {change.new_metric != null ? (
                      <div className="text-xs text-foreground">
                        metric: {(change.new_metric * 100).toFixed(1)}%
                      </div>
                    ) : null}
                  </div>
                </div>
              ))}
            </div>
          </section>

          {insights.top_recategorizations.length > 0 ? (
            <section data-testid="top-recategorizations">
              <h3 className="text-sm font-semibold text-foreground flex items-center gap-1.5 mb-2">
                <TrendingUp className="w-4 h-4" />
                Top recategorizations
              </h3>
              <div className="rounded-lg border border-border bg-card overflow-hidden">
                <table className="w-full text-sm">
                  <thead className="bg-muted text-xs uppercase text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 text-left">From</th>
                      <th className="px-3 py-2 text-left">To</th>
                      <th className="px-3 py-2 text-right">Count</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    {insights.top_recategorizations.map((row, idx) => (
                      <tr key={`${row.from_value}-${row.to_value}-${idx}`}>
                        <td className="px-3 py-2 text-foreground">
                          {row.from_value}
                        </td>
                        <td className="px-3 py-2 text-foreground">
                          {row.to_value}
                        </td>
                        <td className="px-3 py-2 text-right text-muted-foreground">
                          {row.count}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          ) : null}

          {insights.routing_shifts.length > 0 ? (
            <section data-testid="routing-shifts">
              <h3 className="text-sm font-semibold text-foreground flex items-center gap-1.5 mb-2">
                <GitBranch className="w-4 h-4" />
                Routing shifts
              </h3>
              <div className="rounded-lg border border-border bg-card overflow-hidden">
                <table className="w-full text-sm">
                  <thead className="bg-muted text-xs uppercase text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 text-left">From approver</th>
                      <th className="px-3 py-2 text-left">To approver</th>
                      <th className="px-3 py-2 text-right">Count</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    {insights.routing_shifts.map((row, idx) => (
                      <tr
                        key={`${row.from_approver ?? 'unknown'}-${row.to_approver ?? 'unknown'}-${idx}`}
                      >
                        <td className="px-3 py-2 text-foreground">
                          {row.from_approver ?? 'unassigned'}
                        </td>
                        <td className="px-3 py-2 text-foreground">
                          {row.to_approver ?? 'unassigned'}
                        </td>
                        <td className="px-3 py-2 text-right text-muted-foreground">
                          {row.count}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          ) : null}
        </>
      )}
    </div>
  );
}
