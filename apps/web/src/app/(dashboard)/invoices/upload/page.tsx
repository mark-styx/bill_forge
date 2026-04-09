'use client';

import { useState, useCallback, useRef } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { useDropzone } from 'react-dropzone';
import { invoicesApi } from '@/lib/api';
import { toast } from 'sonner';
import {
  Upload,
  FileText,
  X,
  Loader2,
  CheckCircle,
  AlertCircle,
  Image,
  File,
  ArrowLeft,
  Sparkles,
  ScanLine,
  Clock,
  ClipboardCheck,
} from 'lucide-react';

type UploadEntry = {
  id: string;
  file: File;
  status: 'queued' | 'uploading' | 'success' | 'error';
  invoiceId?: string;
  error?: string;
};

async function runBatch(
  batchEntries: UploadEntry[],
  limit: number,
  onUpdate: (id: string, patch: Partial<UploadEntry>) => void
) {
  const queue = [...batchEntries];
  const workers = Array.from(
    { length: Math.min(limit, queue.length) },
    async () => {
      while (queue.length) {
        const entry = queue.shift()!;
        onUpdate(entry.id, { status: 'uploading' });
        try {
          const result = await invoicesApi.upload(entry.file);
          onUpdate(entry.id, { status: 'success', invoiceId: result.invoice_id });
        } catch (e: any) {
          onUpdate(entry.id, { status: 'error', error: e?.message ?? 'Upload failed' });
        }
      }
    }
  );
  await Promise.all(workers);
}

export default function UploadInvoicePage() {
  const router = useRouter();
  const [entries, setEntries] = useState<UploadEntry[]>([]);
  const [isUploading, setIsUploading] = useState(false);
  const entriesRef = useRef<UploadEntry[]>([]);
  entriesRef.current = entries;

  const updateEntry = useCallback(
    (id: string, patch: Partial<UploadEntry>) => {
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, ...patch } : e))
      );
    },
    []
  );

  const onDrop = useCallback(
    (acceptedFiles: File[]) => {
      const newEntries: UploadEntry[] = acceptedFiles.map((file) => ({
        id: crypto.randomUUID(),
        file,
        status: 'queued' as const,
      }));
      setEntries((prev) => {
        const combined = [...prev, ...newEntries];
        return combined.slice(0, 50);
      });
    },
    []
  );

  const { getRootProps, getInputProps, isDragActive, isDragReject } = useDropzone({
    onDrop,
    accept: {
      'application/pdf': ['.pdf'],
      'image/png': ['.png'],
      'image/jpeg': ['.jpg', '.jpeg'],
      'image/tiff': ['.tiff', '.tif'],
    },
    maxFiles: 50,
    maxSize: 10 * 1024 * 1024, // 10MB
  });

  const removeEntry = (id: string) => {
    setEntries((prev) => prev.filter((e) => e.id !== id));
  };

  const handleUpload = async () => {
    const queued = entries.filter((e) => e.status === 'queued');
    if (queued.length === 0) return;

    setIsUploading(true);
    await runBatch(queued, 3, updateEntry);
    setIsUploading(false);

    // Check final state after batch completes
    const finalEntries = entriesRef.current;
    const succeeded = finalEntries.filter((e) => e.status === 'success');
    const failed = finalEntries.filter((e) => e.status === 'error');

    if (failed.length === 0) {
      toast.success(`${succeeded.length} invoice${succeeded.length > 1 ? 's' : ''} uploaded`);
    } else {
      toast.error(`${succeeded.length} of ${succeeded.length + failed.length} uploaded, ${failed.length} failed`);
    }

    // Single file success → redirect (preserves existing UX)
    if (finalEntries.length === 1 && succeeded.length === 1) {
      router.push(`/invoices/${succeeded[0].invoiceId}`);
    }
  };

  const getFileIcon = (file: File) => {
    if (file.type === 'application/pdf') {
      return <FileText className="w-6 h-6 text-error" />;
    }
    if (file.type.startsWith('image/')) {
      return <Image className="w-6 h-6 text-capture" />;
    }
    return <File className="w-6 h-6 text-muted-foreground" />;
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  };

  const statusIcon = (entry: UploadEntry) => {
    switch (entry.status) {
      case 'uploading':
        return <Loader2 className="w-4 h-4 text-capture animate-spin" />;
      case 'success':
        return <CheckCircle className="w-4 h-4 text-success" />;
      case 'error':
        return <AlertCircle className="w-4 h-4 text-error" />;
      default:
        return null;
    }
  };

  const isDone = entries.length > 0 && entries.every((e) => e.status === 'success' || e.status === 'error');
  const succeededCount = entries.filter((e) => e.status === 'success').length;
  const failedCount = entries.filter((e) => e.status === 'error').length;

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <Link
          href="/invoices"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Invoices
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Upload Invoice</h1>
        <p className="text-muted-foreground mt-0.5">
          Upload invoice documents for automatic OCR processing
        </p>
      </div>

      {/* Upload Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-capture to-capture/50" />
        <div className="p-6">
          {entries.length === 0 ? (
            <div
              {...getRootProps()}
              className={`border-2 border-dashed rounded-xl p-12 text-center cursor-pointer transition-all duration-200 ${
                isDragReject
                  ? 'border-error bg-error/5'
                  : isDragActive
                  ? 'border-capture bg-capture/5 scale-[1.01]'
                  : 'border-border hover:border-capture/50 hover:bg-capture/5'
              }`}
            >
              <input {...getInputProps()} />
              <div className="flex flex-col items-center">
                <div
                  className={`p-4 rounded-2xl mb-4 transition-colors ${
                    isDragActive ? 'bg-capture/10' : 'bg-secondary'
                  }`}
                >
                  <Upload
                    className={`w-10 h-10 ${isDragActive ? 'text-capture' : 'text-muted-foreground'}`}
                  />
                </div>
                <p className="text-lg font-medium text-foreground mb-2">
                  {isDragReject
                    ? 'File type not supported'
                    : isDragActive
                    ? 'Drop your invoices here'
                    : 'Drag & drop your invoices'}
                </p>
                <p className="text-sm text-muted-foreground mb-4">
                  or click to browse (up to 50 files)
                </p>
                <div className="flex flex-wrap justify-center gap-2">
                  {['PDF', 'PNG', 'JPEG', 'TIFF'].map((format) => (
                    <span
                      key={format}
                      className="px-2 py-0.5 bg-secondary rounded text-xs font-medium text-muted-foreground"
                    >
                      {format}
                    </span>
                  ))}
                </div>
                <p className="text-xs text-muted-foreground mt-3">
                  Maximum file size: 10MB per file
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              {/* File List */}
              <div className="max-h-80 overflow-y-auto space-y-2 pr-1">
                {entries.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex items-center p-3 bg-secondary/50 rounded-xl border border-border"
                  >
                    <div className="p-2 bg-card rounded-lg shadow-sm mr-3">
                      {getFileIcon(entry.file)}
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-foreground truncate text-sm">
                        {entry.file.name}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {formatSize(entry.file.size)}
                      </p>
                      {entry.status === 'error' && entry.error && (
                        <p className="text-xs text-error mt-0.5">{entry.error}</p>
                      )}
                    </div>
                    <div className="flex items-center gap-2 ml-3">
                      {statusIcon(entry)}
                      {entry.status === 'queued' && !isUploading && (
                        <button
                          onClick={() => removeEntry(entry.id)}
                          className="p-1 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>

              {/* Add more files (while not uploading) */}
              {!isUploading && !isDone && (
                <div
                  {...getRootProps()}
                  className="border border-dashed border-border rounded-lg p-3 text-center cursor-pointer hover:border-capture/50 hover:bg-capture/5 transition-colors"
                >
                  <input {...getInputProps()} />
                  <p className="text-sm text-muted-foreground">
                    Add more files ({entries.length}/50)
                  </p>
                </div>
              )}

              {/* Batch Summary (shown when done) */}
              {isDone && (
                <div className="flex items-center justify-between p-4 bg-secondary/50 rounded-xl border border-border">
                  <div className="flex items-center gap-2">
                    {failedCount === 0 ? (
                      <CheckCircle className="w-5 h-5 text-success" />
                    ) : (
                      <AlertCircle className="w-5 h-5 text-warning" />
                    )}
                    <span className="text-sm font-medium text-foreground">
                      {succeededCount} of {entries.length} uploaded
                      {failedCount > 0 && `, ${failedCount} failed`}
                    </span>
                  </div>
                  <Link
                    href="/invoices"
                    className="text-sm text-capture hover:text-capture/80 transition-colors font-medium"
                  >
                    View Invoices
                  </Link>
                </div>
              )}

              {/* Actions */}
              {!isUploading && !isDone && (
                <div className="flex justify-end gap-3">
                  <button
                    onClick={() => setEntries([])}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleUpload}
                    className="btn bg-capture text-capture-foreground hover:bg-capture/90 shadow-sm"
                  >
                    <ScanLine className="w-4 h-4 mr-2" />
                    Upload &amp; Process ({entries.length})
                  </button>
                </div>
              )}

              {/* Uploading indicator */}
              {isUploading && (
                <div className="flex items-center justify-center gap-2 p-3 text-sm text-muted-foreground">
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Uploading {entries.filter((e) => e.status === 'uploading').length} of{' '}
                  {entries.filter((e) => e.status === 'queued' || e.status === 'uploading').length} remaining...
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Process Steps */}
      <div className="card p-6">
        <div className="flex items-center gap-3 mb-4">
          <div className="p-2 rounded-lg bg-capture/10">
            <Sparkles className="w-5 h-5 text-capture" />
          </div>
          <div>
            <h2 className="font-semibold text-foreground">What happens next?</h2>
            <p className="text-sm text-muted-foreground">Our AI-powered OCR pipeline</p>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          {[
            {
              icon: Upload,
              title: 'Upload',
              description: 'Your invoices are securely uploaded',
            },
            {
              icon: ScanLine,
              title: 'OCR Processing',
              description: 'AI extracts key data automatically',
            },
            {
              icon: Clock,
              title: 'Review',
              description: 'Verify and correct any fields',
            },
            {
              icon: ClipboardCheck,
              title: 'Submit',
              description: 'Route to approval workflow',
            },
          ].map((step, index) => (
            <div
              key={step.title}
              className="relative p-4 bg-secondary/50 rounded-xl"
            >
              {index < 3 && (
                <div className="hidden md:block absolute top-1/2 -right-2 w-4 h-0.5 bg-border" />
              )}
              <div className="p-2 bg-card rounded-lg shadow-sm w-fit mb-3">
                <step.icon className="w-5 h-5 text-capture" />
              </div>
              <p className="font-medium text-foreground text-sm">{step.title}</p>
              <p className="text-xs text-muted-foreground mt-1">{step.description}</p>
            </div>
          ))}
        </div>
      </div>

      {/* Tips */}
      <div className="p-4 bg-capture/5 border border-capture/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">Tips for best results</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-capture mt-0.5 flex-shrink-0" />
            Use high-quality scans or clear photos
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-capture mt-0.5 flex-shrink-0" />
            Ensure all text is legible and not cut off
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-capture mt-0.5 flex-shrink-0" />
            Include the full invoice with all line items
          </li>
        </ul>
      </div>
    </div>
  );
}
