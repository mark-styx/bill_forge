'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Upload, Link2, CheckCircle2, FileSpreadsheet } from 'lucide-react';
import { vendorsApi, quickbooksApi } from '@/lib/api';
import type { ImportVendorsResult } from '@/lib/api';
import { StepperWithContent, StepContent, Step } from '@/components/ui/stepper';
import { Button } from '@/components/ui/button';

const STEPS: Step[] = [
  { id: 'source', title: 'Source', icon: <Link2 className="w-4 h-4" /> },
  { id: 'import', title: 'Import', icon: <Upload className="w-4 h-4" /> },
  { id: 'review', title: 'Review', icon: <CheckCircle2 className="w-4 h-4" /> },
];

const SOURCE_OPTIONS = [
  { id: 'quickbooks', name: 'QuickBooks Online', description: 'Pull vendors from your connected QuickBooks account.', icon: <Link2 className="w-5 h-5" /> },
  { id: 'spreadsheet', name: 'Spreadsheet (CSV)', description: 'Upload a CSV file with your vendor data.', icon: <FileSpreadsheet className="w-5 h-5" /> },
];

export default function MigratePage() {
  const router = useRouter();
  const [selectedSource, setSelectedSource] = useState<'quickbooks' | 'spreadsheet' | ''>('');
  const [file, setFile] = useState<File | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportVendorsResult | null>(null);

  const handleStepComplete = async (stepIndex: number): Promise<boolean> => {
    setError(null);

    if (stepIndex === 0) {
      if (!selectedSource) {
        setError('Please select an import source');
        return false;
      }
      return true;
    }

    if (stepIndex === 1) {
      if (selectedSource === 'spreadsheet') {
        if (!file) {
          setError('Please select a CSV file to upload');
          return false;
        }
        setIsLoading(true);
        try {
          const res = await vendorsApi.importCsv(file);
          setResult(res);
          return true;
        } catch (err: any) {
          setError(err?.message || 'Failed to import CSV');
          return false;
        } finally {
          setIsLoading(false);
        }
      }

      if (selectedSource === 'quickbooks') {
        setIsLoading(true);
        try {
          const res = await quickbooksApi.syncVendors();
          // Map SyncResult to ImportVendorsResult shape
          setResult({
            imported: res.created ?? 0,
            skipped: res.synced ?? 0,
            errors: res.errors?.length ?? 0,
            error_details: res.errors ?? [],
          });
          return true;
        } catch (err: any) {
          setError(err?.message || 'QuickBooks sync failed. Make sure QuickBooks is connected.');
          return false;
        } finally {
          setIsLoading(false);
        }
      }
    }

    return true;
  };

  const handleComplete = () => {
    router.push('/vendors');
  };

  return (
    <div className="min-h-[80vh] flex items-center justify-center p-4">
      <div className="w-full max-w-2xl bg-card border border-border rounded-2xl shadow-lg p-8">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 mb-3">
            <Upload className="w-6 h-6 text-primary" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">Import Vendors</h1>
          <p className="text-muted-foreground mt-1">Bring your existing vendor data into BillForge</p>
        </div>

        {error && (
          <div className="mb-6 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}

        <StepperWithContent
          steps={STEPS}
          variant="numbered"
          onStepComplete={handleStepComplete}
          onComplete={handleComplete}
          isLoading={isLoading}
          completeLabel="Go to Vendors"
        >
          {/* Step 1: Source */}
          <StepContent>
            <p className="text-sm text-muted-foreground mb-4">
              Choose where to import your vendor data from.
            </p>
            <div className="space-y-3">
              {SOURCE_OPTIONS.map((opt) => (
                <button
                  key={opt.id}
                  onClick={() => setSelectedSource(opt.id as 'quickbooks' | 'spreadsheet')}
                  className={`w-full text-left p-4 rounded-lg border-2 transition-all flex items-center gap-4 ${
                    selectedSource === opt.id
                      ? 'border-primary bg-primary/5'
                      : 'border-border hover:border-primary/30'
                  }`}
                >
                  <div className="text-muted-foreground">{opt.icon}</div>
                  <div>
                    <div className="font-medium text-foreground">{opt.name}</div>
                    <div className="text-sm text-muted-foreground mt-0.5">{opt.description}</div>
                  </div>
                </button>
              ))}
            </div>
          </StepContent>

          {/* Step 2: Import */}
          <StepContent>
            {selectedSource === 'quickbooks' && (
              <div className="space-y-4">
                <p className="text-sm text-muted-foreground">
                  Click <strong>Next</strong> to pull all vendors from your connected QuickBooks Online account.
                  If QuickBooks is not connected, you will see an error.
                </p>
                <div className="bg-muted/30 rounded-lg p-4 text-sm text-muted-foreground">
                  Make sure you have connected QuickBooks via <strong>Settings &gt; Integrations</strong> before importing.
                </div>
              </div>
            )}
            {selectedSource === 'spreadsheet' && (
              <div className="space-y-4">
                <p className="text-sm text-muted-foreground">
                  Upload a CSV file with your vendor data. The file must have a <code className="bg-muted px-1 rounded">name</code> column.
                </p>
                <div className="space-y-2">
                  <input
                    type="file"
                    accept=".csv"
                    onChange={(e) => setFile(e.target.files?.[0] ?? null)}
                    className="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-semibold file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer"
                  />
                  {file && (
                    <p className="text-xs text-muted-foreground">
                      Selected: {file.name} ({(file.size / 1024).toFixed(1)} KB)
                    </p>
                  )}
                </div>
                <div className="bg-muted/30 rounded-lg p-3 text-xs text-muted-foreground space-y-1">
                  <p className="font-medium text-foreground text-sm mb-1">Expected columns</p>
                  <p><strong>name</strong> (required) - Vendor name</p>
                  <p>email, vendor_type, phone, tax_id, payment_terms, vendor_code (optional)</p>
                  <p className="pt-1">Tip: Use the Vendors CSV export as a template.</p>
                </div>
              </div>
            )}
          </StepContent>

          {/* Step 3: Review */}
          <StepContent>
            {result ? (
              <div className="space-y-4">
                <div className="text-center mb-4">
                  <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-green-500/20 mb-3">
                    <CheckCircle2 className="w-6 h-6 text-green-400" />
                  </div>
                  <h3 className="text-lg font-bold text-foreground">Import Complete</h3>
                </div>
                <div className="grid grid-cols-3 gap-4 text-center">
                  <div className="bg-muted/30 rounded-lg p-4">
                    <div className="text-2xl font-bold text-green-400">{result.imported}</div>
                    <div className="text-sm text-muted-foreground">Imported</div>
                  </div>
                  <div className="bg-muted/30 rounded-lg p-4">
                    <div className="text-2xl font-bold text-yellow-400">{result.skipped}</div>
                    <div className="text-sm text-muted-foreground">Skipped</div>
                  </div>
                  <div className="bg-muted/30 rounded-lg p-4">
                    <div className="text-2xl font-bold text-red-400">{result.errors}</div>
                    <div className="text-sm text-muted-foreground">Errors</div>
                  </div>
                </div>
                {result.error_details.length > 0 && (
                  <div className="bg-red-500/5 border border-red-500/20 rounded-lg p-3">
                    <p className="text-sm font-medium text-red-400 mb-2">Error details</p>
                    <ul className="text-xs text-red-400/80 space-y-1">
                      {result.error_details.map((d, i) => (
                        <li key={i}>{d}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-center py-6 text-muted-foreground">
                <p>No import results yet.</p>
              </div>
            )}
          </StepContent>
        </StepperWithContent>
      </div>
    </div>
  );
}
