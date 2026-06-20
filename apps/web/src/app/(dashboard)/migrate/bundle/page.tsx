'use client';

import { useCallback, useEffect, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { Upload, ArrowLeft, CheckCircle2, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  apMigrationApi,
  type ApMigrationCommitResponse,
  type ApMigrationPreview,
  type ApMigrationPreviewRow,
} from '@/lib/api/apMigration';

type Section = {
  key: keyof ApMigrationPreview['entities'];
  label: string;
};

const SECTIONS: Section[] = [
  { key: 'vendors', label: 'Vendors' },
  { key: 'invoices', label: 'Open Invoices' },
  { key: 'approval_workflows', label: 'Approval Workflows' },
  { key: 'gl_mappings', label: 'GL Mappings' },
  { key: 'approvers', label: 'Approver Hierarchy' },
  { key: 'documents', label: 'Documents' },
];

export default function ApMigrationBundlePage() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const bundleId = searchParams.get('id');

  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<ApMigrationPreview | null>(null);
  const [committing, setCommitting] = useState(false);
  const [commitResult, setCommitResult] = useState<ApMigrationCommitResponse | null>(null);
  const [overrides, setOverrides] = useState<Record<string, 'apply' | 'skip'>>({});

  const loadPreview = useCallback(async (id: string) => {
    try {
      const data = await apMigrationApi.getPreview(id);
      setPreview(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load preview');
    }
  }, []);

  useEffect(() => {
    if (bundleId && !preview && !commitResult) {
      loadPreview(bundleId);
    }
  }, [bundleId, preview, commitResult, loadPreview]);

  const handleUpload = async () => {
    if (!file) {
      setError('Please select a BILL.com or Coupa export ZIP');
      return;
    }
    setUploading(true);
    setError(null);
    try {
      const res = await apMigrationApi.uploadBundle(file);
      router.push(`/migrate/bundle?id=${res.bundle_id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Upload failed');
    } finally {
      setUploading(false);
    }
  };

  const handleBulkAction = (sectionKey: Section['key'], action: 'apply' | 'skip') => {
    if (!preview) return;
    const update: Record<string, 'apply' | 'skip'> = { ...overrides };
    for (const row of preview.entities[sectionKey]) {
      update[row.id] = action;
    }
    setOverrides(update);
  };

  const handleCommit = async () => {
    if (!bundleId) return;
    setCommitting(true);
    setError(null);
    try {
      const res = await apMigrationApi.commit(bundleId);
      setCommitResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Commit failed');
    } finally {
      setCommitting(false);
    }
  };

  const handleCancel = async () => {
    if (!bundleId) return;
    try {
      await apMigrationApi.cancel(bundleId);
    } finally {
      router.push('/migrate');
    }
  };

  // -------------------------------------------------------------------------
  // Result state
  // -------------------------------------------------------------------------
  if (commitResult) {
    return (
      <div className="min-h-[80vh] p-6 max-w-4xl mx-auto">
        <div className="bg-card border border-border rounded-2xl shadow-lg p-8">
          <div className="text-center mb-6">
            <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-green-500/20 mb-3">
              <CheckCircle2 className="w-6 h-6 text-green-400" />
            </div>
            <h1 className="text-2xl font-bold text-foreground">Migration Complete</h1>
            <p className="text-muted-foreground mt-1">
              Bundle {commitResult.bundle_id.slice(0, 8)}… committed to your tenant.
            </p>
          </div>
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
            <ResultCard label="Vendors created" value={commitResult.vendors_created} />
            <ResultCard label="Vendors updated" value={commitResult.vendors_updated} />
            <ResultCard label="Invoices created" value={commitResult.invoices_created} />
            <ResultCard label="Invoices updated" value={commitResult.invoices_updated} />
            <ResultCard label="Workflows imported" value={commitResult.approval_workflows_created} />
            <ResultCard label="GL mappings imported" value={commitResult.gl_mappings_created} />
            <ResultCard label="Approvers imported" value={commitResult.approvers_created} />
            <ResultCard label="Documents imported" value={commitResult.documents_created} />
            <ResultCard label="Skipped" value={commitResult.skipped} />
          </div>
          <div className="mt-6 flex justify-end">
            <Link
              href="/migrate"
              className="px-4 py-2 rounded-lg border border-border text-sm text-foreground hover:bg-muted"
            >
              Back to Import
            </Link>
          </div>
        </div>
      </div>
    );
  }

  // -------------------------------------------------------------------------
  // Preview state
  // -------------------------------------------------------------------------
  if (bundleId && preview) {
    return (
      <div className="p-6 max-w-7xl mx-auto">
        <div className="flex items-center gap-3 mb-4">
          <Link
            href="/migrate"
            className="inline-flex items-center text-sm text-muted-foreground hover:text-foreground"
          >
            <ArrowLeft className="w-4 h-4 mr-1" /> Back
          </Link>
          <h1 className="text-2xl font-bold text-foreground">
            Migration Preview
          </h1>
          <span className="px-2 py-1 text-xs rounded-md bg-muted text-muted-foreground uppercase">
            {preview.bundle.source}
          </span>
        </div>
        {error && (
          <div className="mb-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}
        <p className="text-sm text-muted-foreground mb-6">
          Review the staged records side-by-side before any changes are written to your tenant.
          Use Apply / Skip per section, then Commit.
        </p>

        <div className="space-y-6">
          {SECTIONS.map((section) => {
            const rows = preview.entities[section.key];
            const counts = countActions(rows, overrides);
            return (
              <section
                key={section.key}
                aria-label={section.label}
                className="border border-border rounded-xl bg-card"
              >
                <header className="flex items-center justify-between px-4 py-3 border-b border-border">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">{section.label}</h2>
                    <p className="text-xs text-muted-foreground">
                      {rows.length} rows · {counts.create} create · {counts.update} update ·{' '}
                      {counts.skip} skip
                    </p>
                  </div>
                  {rows.length > 0 && (
                    <div className="flex gap-2">
                      <button
                        type="button"
                        onClick={() => handleBulkAction(section.key, 'apply')}
                        className="px-3 py-1 text-xs rounded-md border border-border hover:bg-muted"
                      >
                        Apply all
                      </button>
                      <button
                        type="button"
                        onClick={() => handleBulkAction(section.key, 'skip')}
                        className="px-3 py-1 text-xs rounded-md border border-border hover:bg-muted"
                      >
                        Skip all
                      </button>
                    </div>
                  )}
                </header>
                {rows.length === 0 ? (
                  <div className="px-4 py-6 text-sm text-muted-foreground italic">
                    No {section.label.toLowerCase()} in this bundle.
                  </div>
                ) : (
                  <div className="grid grid-cols-2 divide-x divide-border text-xs">
                    <div>
                      <div className="px-3 py-2 font-semibold text-muted-foreground border-b border-border bg-muted/30">
                        Source ({preview.bundle.source.toUpperCase()})
                      </div>
                      <ul className="divide-y divide-border">
                        {rows.map((row) => (
                          <li key={`src-${row.id}`} className="px-3 py-2 font-mono break-all">
                            {renderPayload(row.source_payload)}
                          </li>
                        ))}
                      </ul>
                    </div>
                    <div>
                      <div className="px-3 py-2 font-semibold text-muted-foreground border-b border-border bg-muted/30">
                        Target (BillForge)
                      </div>
                      <ul className="divide-y divide-border">
                        {rows.map((row) => (
                          <li key={`tgt-${row.id}`} className="px-3 py-2">
                            <TargetCell row={row} override={overrides[row.id]} />
                          </li>
                        ))}
                      </ul>
                    </div>
                  </div>
                )}
              </section>
            );
          })}
        </div>

        <div className="mt-6 flex justify-end gap-3">
          <Button variant="outline" onClick={handleCancel} disabled={committing}>
            Cancel
          </Button>
          <Button onClick={handleCommit} disabled={committing}>
            {committing ? 'Committing…' : 'Commit Migration'}
          </Button>
        </div>
      </div>
    );
  }

  // -------------------------------------------------------------------------
  // Upload state (no bundle yet)
  // -------------------------------------------------------------------------
  return (
    <div className="min-h-[80vh] flex items-center justify-center p-4">
      <div className="w-full max-w-2xl bg-card border border-border rounded-2xl shadow-lg p-8">
        <Link
          href="/migrate"
          className="inline-flex items-center text-sm text-muted-foreground hover:text-foreground mb-4"
        >
          <ArrowLeft className="w-4 h-4 mr-1" /> Back
        </Link>
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 mb-3">
            <Upload className="w-6 h-6 text-primary" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">Import from BILL.com or Coupa</h1>
          <p className="text-muted-foreground mt-1">
            Upload an export bundle (ZIP) and BillForge will stage a side-by-side preview before
            anything is written.
          </p>
        </div>
        {error && (
          <div className="mb-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}
        <div className="space-y-3">
          <input
            type="file"
            accept=".zip,application/zip"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
            className="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-semibold file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer"
          />
          {file && (
            <p className="text-xs text-muted-foreground">
              Selected: {file.name} ({(file.size / 1024).toFixed(1)} KB)
            </p>
          )}
          <div className="bg-muted/30 rounded-lg p-3 text-xs text-muted-foreground space-y-1">
            <p className="font-medium text-foreground text-sm mb-1">Expected bundle layout</p>
            <p><code>manifest.json</code> declares <code>{`{ "source": "bill" | "coupa", "version": "1" }`}</code></p>
            <p><code>vendors.csv</code>, <code>invoices.csv</code>, <code>approval_workflows.csv</code></p>
            <p><code>gl_mappings.csv</code>, <code>approvers.csv</code>, <code>documents/</code></p>
          </div>
        </div>
        <div className="mt-6 flex justify-end">
          <Button onClick={handleUpload} disabled={uploading || !file}>
            {uploading ? 'Uploading…' : 'Upload and Preview'}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ResultCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="bg-muted/30 rounded-lg p-4 text-center">
      <div className="text-2xl font-bold text-foreground">{value}</div>
      <div className="text-xs text-muted-foreground">{label}</div>
    </div>
  );
}

function TargetCell({
  row,
  override,
}: {
  row: ApMigrationPreviewRow;
  override?: 'apply' | 'skip';
}) {
  const effectiveAction =
    override === 'skip' ? 'skip' : override === 'apply' ? row.target_action : row.target_action;
  const isMatch = !!row.target_match_id;
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-2">
        <span
          className={`px-1.5 py-0.5 rounded text-[10px] uppercase font-semibold ${
            effectiveAction === 'create'
              ? 'bg-green-500/15 text-green-300'
              : effectiveAction === 'update'
              ? 'bg-blue-500/15 text-blue-300'
              : 'bg-yellow-500/15 text-yellow-300'
          }`}
        >
          {effectiveAction}
        </span>
        <span className="text-muted-foreground">{isMatch ? 'Matched' : 'New'}</span>
      </div>
      {row.conflict_reason && (
        <div className="flex items-start gap-1 text-yellow-400/90">
          <AlertCircle className="w-3 h-3 mt-0.5 flex-shrink-0" />
          <span>{row.conflict_reason}</span>
        </div>
      )}
    </div>
  );
}

function renderPayload(payload: Record<string, string>) {
  const entries = Object.entries(payload).slice(0, 6);
  if (entries.length === 0) return <span className="text-muted-foreground italic">(empty)</span>;
  return (
    <div className="space-y-0.5">
      {entries.map(([k, v]) => (
        <div key={k}>
          <span className="text-muted-foreground">{k}:</span> {String(v)}
        </div>
      ))}
    </div>
  );
}

function countActions(
  rows: ApMigrationPreviewRow[],
  overrides: Record<string, 'apply' | 'skip'>,
) {
  let create = 0;
  let update = 0;
  let skip = 0;
  for (const row of rows) {
    const override = overrides[row.id];
    const action = override === 'skip' ? 'skip' : row.target_action;
    if (action === 'create') create += 1;
    else if (action === 'update') update += 1;
    else skip += 1;
  }
  return { create, update, skip };
}
