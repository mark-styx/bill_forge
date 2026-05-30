'use client';

import { useState } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import { routingApi, type RoutingConfig, type SimulationSummaryResponse } from '@/lib/api';
import Link from 'next/link';
import {
  ArrowLeft,
  FlaskConical,
  Clock,
  AlertTriangle,
  Users,
  GitCompare,
  Loader2,
  Info,
  ChevronDown,
  ChevronUp,
} from 'lucide-react';

export default function SimulateRoutingPage() {
  const [showChanged, setShowChanged] = useState(false);

  // Load live config as the baseline
  const { data: liveConfig, isLoading: configLoading } = useQuery({
    queryKey: ['routing-config'],
    queryFn: () => routingApi.getConfig(),
  });

  // Candidate config starts as a deep copy of live; user can tweak individual fields.
  const [candidateConfig, setCandidateConfig] = useState<Partial<RoutingConfig> | null>(null);

  // When live config loads, seed the candidate.
  if (liveConfig && candidateConfig === null) {
    setCandidateConfig({ ...liveConfig });
  }

  // Run simulation mutation
  const simulate = useMutation({
    mutationFn: (body: { candidate_config: Partial<RoutingConfig>; sample_size?: number }) =>
      routingApi.simulate(body),
  });

  const handleRunSimulation = () => {
    if (!candidateConfig) return;
    simulate.mutate({ candidate_config: candidateConfig, sample_size: 200 });
  };

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/assignment-rules"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Assignment Rules
        </Link>
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
              <FlaskConical className="w-6 h-6 text-processing" />
              Routing Simulator
            </h1>
            <p className="text-muted-foreground mt-0.5">
              Preview how a candidate routing configuration would change approver assignments
            </p>
          </div>
          <button
            onClick={handleRunSimulation}
            disabled={!candidateConfig || simulate.isPending}
            className="btn btn-primary btn-sm"
          >
            {simulate.isPending ? (
              <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
            ) : (
              <FlaskConical className="w-4 h-4 mr-1.5" />
            )}
            Run Simulation
          </button>
        </div>
      </div>

      {/* Preview-only banner */}
      <div className="flex items-center gap-2 p-3 bg-processing/5 border border-processing/20 rounded-xl">
        <Info className="w-4 h-4 text-processing flex-shrink-0" />
        <p className="text-sm text-muted-foreground">
          <span className="font-medium text-foreground">Preview only.</span> Nothing is published. The simulation replays the last
          200 historical invoices through the candidate config and compares against the live policy.
        </p>
      </div>

      {/* Config editors: Live (read-only) vs Candidate (editable) */}
      {!configLoading && liveConfig && candidateConfig && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* Live policy */}
          <div className="card p-5">
            <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">
              Live Policy
            </h3>
            <ConfigSummary config={liveConfig} />
          </div>

          {/* Candidate policy */}
          <div className="card p-5 border-2 border-processing/30">
            <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">
              Candidate Policy
            </h3>
            <ConfigEditor config={candidateConfig} onChange={setCandidateConfig} />
          </div>
        </div>
      )}

      {/* Error state */}
      {simulate.isError && (
        <div className="card p-4 border border-destructive/30 bg-destructive/5">
          <p className="text-sm text-destructive font-medium">
            Simulation failed: {(simulate.error as any)?.message || 'Unknown error'}
          </p>
        </div>
      )}

      {/* Results */}
      {simulate.data && (
        <SimulationResults
          data={simulate.data}
          showChanged={showChanged}
          setShowChanged={setShowChanged}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Config summary (read-only)
// ---------------------------------------------------------------------------
function ConfigSummary({ config }: { config: RoutingConfig }) {
  const rows: [string, string][] = [
    ['Workload weight', config.workload_weight.toFixed(2)],
    ['Expertise weight', config.expertise_weight.toFixed(2)],
    ['Availability weight', config.availability_weight.toFixed(2)],
    ['Max workload score', config.max_workload_score.toFixed(0)],
    ['Auto delegation', config.enable_auto_delegation ? 'Yes' : 'No'],
    ['Working hours', `${config.working_hours_start} - ${config.working_hours_end}`],
    ['Timezone', config.working_timezone],
  ];
  return (
    <div className="space-y-1.5">
      {rows.map(([label, value]) => (
        <div key={label} className="flex justify-between text-sm">
          <span className="text-muted-foreground">{label}</span>
          <span className="font-medium text-foreground">{value}</span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Config editor (editable fields)
// ---------------------------------------------------------------------------
function ConfigEditor({
  config,
  onChange,
}: {
  config: Partial<RoutingConfig>;
  onChange: (c: Partial<RoutingConfig>) => void;
}) {
  const update = (key: keyof RoutingConfig, value: number | boolean | string) => {
    onChange({ ...config, [key]: value });
  };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-3">
        <LabeledInput
          label="Workload weight"
          type="number"
          step={0.05}
          value={config.workload_weight ?? ''}
          onChange={(v) => update('workload_weight', parseFloat(v) || 0)}
        />
        <LabeledInput
          label="Expertise weight"
          type="number"
          step={0.05}
          value={config.expertise_weight ?? ''}
          onChange={(v) => update('expertise_weight', parseFloat(v) || 0)}
        />
        <LabeledInput
          label="Availability weight"
          type="number"
          step={0.05}
          value={config.availability_weight ?? ''}
          onChange={(v) => update('availability_weight', parseFloat(v) || 0)}
        />
        <LabeledInput
          label="Max workload score"
          type="number"
          step={5}
          value={config.max_workload_score ?? ''}
          onChange={(v) => update('max_workload_score', parseFloat(v) || 0)}
        />
      </div>
      <div className="flex items-center gap-2">
        <input
          type="checkbox"
          id="auto-delegation"
          checked={config.enable_auto_delegation ?? false}
          onChange={(e) => update('enable_auto_delegation', e.target.checked)}
          className="rounded border-border"
        />
        <label htmlFor="auto-delegation" className="text-sm text-foreground">
          Enable auto delegation
        </label>
      </div>
    </div>
  );
}

function LabeledInput({
  label,
  type,
  step,
  value,
  onChange,
}: {
  label: string;
  type: string;
  step?: number;
  value: number | string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <label className="text-xs text-muted-foreground">{label}</label>
      <input
        type={type}
        step={step}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="input mt-0.5 text-sm"
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Delta badge helper
// ---------------------------------------------------------------------------
function DeltaBadge({ candidate, live, unit }: { candidate: number; live: number; unit?: string }) {
  const delta = candidate - live;
  const isPositive = delta > 0;
  const isNeutral = Math.abs(delta) < 0.01;
  return (
    <span
      className={`ml-2 text-xs font-medium px-1.5 py-0.5 rounded-full ${
        isNeutral
          ? 'bg-secondary text-muted-foreground'
          : isPositive
            ? 'bg-destructive/10 text-destructive'
            : 'bg-success/10 text-success'
      }`}
    >
      {isPositive ? '+' : ''}
      {delta.toFixed(1)}
      {unit ? ` ${unit}` : ''}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Simulation results
// ---------------------------------------------------------------------------
function SimulationResults({
  data,
  showChanged,
  setShowChanged,
}: {
  data: SimulationSummaryResponse;
  showChanged: boolean;
  setShowChanged: (v: boolean) => void;
}) {
  const changedOutcomes = data.outcomes.filter((o) => o.changed);

  // Top 10 approvers by candidate volume
  const topApprovers = Object.entries(data.approver_load_candidate)
    .sort(([, a], [, b]) => b - a)
    .slice(0, 10);

  return (
    <div className="space-y-4">
      {/* Summary cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Avg cycle time */}
        <div className="card p-4">
          <div className="flex items-center gap-2 text-muted-foreground text-xs font-medium mb-1">
            <Clock className="w-3.5 h-3.5" />
            Avg Cycle (hours)
          </div>
          <div className="flex items-baseline">
            <span className="text-2xl font-semibold text-foreground">
              {data.avg_cycle_hours_candidate.toFixed(1)}
            </span>
            <DeltaBadge
              candidate={data.avg_cycle_hours_candidate}
              live={data.avg_cycle_hours_live}
              unit="h"
            />
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Live: {data.avg_cycle_hours_live.toFixed(1)}h
          </p>
        </div>

        {/* Stalled invoices */}
        <div className="card p-4">
          <div className="flex items-center gap-2 text-muted-foreground text-xs font-medium mb-1">
            <AlertTriangle className="w-3.5 h-3.5" />
            Stalled Invoices
          </div>
          <div className="flex items-baseline">
            <span className="text-2xl font-semibold text-foreground">
              {data.stalled_count_candidate}
            </span>
            <DeltaBadge
              candidate={data.stalled_count_candidate}
              live={data.stalled_count_live}
            />
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Live: {data.stalled_count_live}
          </p>
        </div>

        {/* Changed invoices */}
        <div className="card p-4">
          <div className="flex items-center gap-2 text-muted-foreground text-xs font-medium mb-1">
            <GitCompare className="w-3.5 h-3.5" />
            Routing Changes
          </div>
          <div className="flex items-baseline">
            <span className="text-2xl font-semibold text-foreground">{data.changed_count}</span>
            <span className="text-xs text-muted-foreground ml-2">
              / {data.total_simulated} invoices
            </span>
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            {data.total_simulated > 0
              ? ((data.changed_count / data.total_simulated) * 100).toFixed(0)
              : 0}
            % of sample changed
          </p>
        </div>

        {/* Total simulated */}
        <div className="card p-4">
          <div className="flex items-center gap-2 text-muted-foreground text-xs font-medium mb-1">
            <Users className="w-3.5 h-3.5" />
            Total Simulated
          </div>
          <div className="flex items-baseline">
            <span className="text-2xl font-semibold text-foreground">
              {data.total_simulated}
            </span>
          </div>
          <p className="text-xs text-muted-foreground mt-1">Historical invoices replayed</p>
        </div>
      </div>

      {/* Approver load table */}
      <div className="card overflow-hidden">
        <div className="p-4 bg-secondary/50 border-b border-border">
          <h3 className="font-semibold text-foreground">Approver Load (Top 10 by Candidate Volume)</h3>
          <p className="text-sm text-muted-foreground">
            Number of invoices each approver would receive under each policy
          </p>
        </div>
        {topApprovers.length === 0 ? (
          <div className="p-6 text-center text-sm text-muted-foreground">
            No approver assignments in simulation
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left p-3 text-xs font-semibold text-muted-foreground uppercase">
                  Approver ID
                </th>
                <th className="text-right p-3 text-xs font-semibold text-muted-foreground uppercase">
                  Candidate
                </th>
                <th className="text-right p-3 text-xs font-semibold text-muted-foreground uppercase">
                  Live
                </th>
                <th className="text-right p-3 text-xs font-semibold text-muted-foreground uppercase">
                  Delta
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {topApprovers.map(([approverId, candidateCount]) => {
                const liveCount = data.approver_load_live[approverId] ?? 0;
                const delta = candidateCount - liveCount;
                return (
                  <tr key={approverId} className="hover:bg-secondary/30">
                    <td className="p-3 font-mono text-xs text-foreground">{approverId.slice(0, 8)}...</td>
                    <td className="p-3 text-right font-medium text-foreground">{candidateCount}</td>
                    <td className="p-3 text-right text-muted-foreground">{liveCount}</td>
                    <td
                      className={`p-3 text-right font-medium ${
                        delta > 0
                          ? 'text-destructive'
                          : delta < 0
                            ? 'text-success'
                            : 'text-muted-foreground'
                      }`}
                    >
                      {delta > 0 ? '+' : ''}
                      {delta}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {/* Changed invoices diff table */}
      {data.changed_count > 0 && (
        <div className="card overflow-hidden">
          <button
            className="w-full p-4 flex items-center justify-between hover:bg-secondary/30 transition-colors"
            onClick={() => setShowChanged(!showChanged)}
          >
            <div className="text-left">
              <h3 className="font-semibold text-foreground">
                Changed Routing ({data.changed_count} invoices)
              </h3>
              <p className="text-sm text-muted-foreground">
                Invoices whose approver changes under the candidate policy
              </p>
            </div>
            {showChanged ? (
              <ChevronUp className="w-5 h-5 text-muted-foreground" />
            ) : (
              <ChevronDown className="w-5 h-5 text-muted-foreground" />
            )}
          </button>
          {showChanged && (
            <div className="max-h-96 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="sticky top-0 bg-secondary/80 backdrop-blur-sm">
                  <tr className="border-b border-border">
                    <th className="text-left p-3 text-xs font-semibold text-muted-foreground uppercase">
                      Invoice
                    </th>
                    <th className="text-left p-3 text-xs font-semibold text-muted-foreground uppercase">
                      Live Approver
                    </th>
                    <th className="text-left p-3 text-xs font-semibold text-muted-foreground uppercase">
                      Candidate Approver
                    </th>
                    <th className="text-center p-3 text-xs font-semibold text-muted-foreground uppercase">
                      Stall?
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {changedOutcomes.slice(0, 100).map((outcome) => (
                    <tr key={outcome.invoice_id} className="hover:bg-secondary/30">
                      <td className="p-3 font-mono text-xs text-foreground">
                        {outcome.invoice_id.slice(0, 8)}...
                      </td>
                      <td className="p-3 text-xs text-muted-foreground">
                        {outcome.live_approver ? (
                          <span className="font-mono">{outcome.live_approver.slice(0, 8)}...</span>
                        ) : (
                          <span className="text-destructive">None</span>
                        )}
                      </td>
                      <td className="p-3 text-xs">
                        {outcome.predicted_approver ? (
                          <span className="font-mono text-foreground">
                            {outcome.predicted_approver.slice(0, 8)}...
                          </span>
                        ) : (
                          <span className="text-destructive">None</span>
                        )}
                      </td>
                      <td className="p-3 text-center">
                        {outcome.would_stall ? (
                          <span className="inline-flex items-center gap-1 text-destructive text-xs font-medium">
                            <AlertTriangle className="w-3 h-3" />
                            Stall
                          </span>
                        ) : (
                          <span className="text-muted-foreground">-</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {changedOutcomes.length > 100 && (
                <div className="p-3 text-center text-xs text-muted-foreground border-t border-border">
                  Showing first 100 of {changedOutcomes.length} changed invoices
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
