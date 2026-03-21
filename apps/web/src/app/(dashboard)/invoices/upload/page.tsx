'use client';

import { useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { useDropzone } from 'react-dropzone';
import { useMutation, useQuery } from '@tanstack/react-query';
import { ocrPipelineApi } from '@/lib/api';
import type { OcrJob, OcrPipelineStats, BatchUploadResult } from '@/lib/api';
import { toast } from 'sonner';
import {
  Upload,
  FileText,
  X,
  Loader2,
  CheckCircle,
  Image,
  File,
  ArrowLeft,
  Sparkles,
  ScanLine,
  Clock,
  ClipboardCheck,
  AlertCircle,
  RotateCcw,
  XCircle,
  BarChart3,
} from 'lucide-react';

type FileWithStatus = {
  file: File;
  status: 'pending' | 'uploading' | 'success' | 'error';
  jobId?: string;
  error?: string;
};

export default function UploadInvoicePage() {
  const router = useRouter();
  const [fileItems, setFileItems] = useState<FileWithStatus[]>([]);
  const [batchResult, setBatchResult] = useState<BatchUploadResult | null>(null);

  const statsQuery = useQuery({
    queryKey: ['ocr-stats'],
    queryFn: () => ocrPipelineApi.getStats(),
  });

  const uploadMutation = useMutation({
    mutationFn: async (files: File[]) => {
      setFileItems((prev) =>
        prev.map((f) => ({ ...f, status: 'uploading' as const }))
      );
      return ocrPipelineApi.batchUpload(files);
    },
    onSuccess: (data) => {
      setBatchResult(data);
      setFileItems((prev) => {
        const updated = [...prev];
        // Mark successful uploads
        let jobIdx = 0;
        for (let i = 0; i < updated.length; i++) {
          const hasError = data.errors.find(
            (e) => e.file_name === updated[i].file.name
          );
          if (hasError) {
            updated[i] = { ...updated[i], status: 'error', error: hasError.error };
          } else if (jobIdx < data.job_ids.length) {
            updated[i] = { ...updated[i], status: 'success', jobId: data.job_ids[jobIdx] };
            jobIdx++;
          }
        }
        return updated;
      });
      if (data.jobs_created > 0) {
        toast.success(`${data.jobs_created} file${data.jobs_created > 1 ? 's' : ''} uploaded for OCR processing`);
      }
      if (data.errors.length > 0) {
        toast.error(`${data.errors.length} file${data.errors.length > 1 ? 's' : ''} failed to upload`);
      }
    },
    onError: (error: any) => {
      setFileItems((prev) =>
        prev.map((f) => ({ ...f, status: 'error', error: error.message || 'Upload failed' }))
      );
      toast.error(error.message || 'Batch upload failed');
    },
  });

  const onDrop = useCallback((acceptedFiles: File[]) => {
    const newItems: FileWithStatus[] = acceptedFiles.map((file) => ({
      file,
      status: 'pending' as const,
    }));
    setFileItems((prev) => [...prev, ...newItems]);
    setBatchResult(null);
  }, []);

  const { getRootProps, getInputProps, isDragActive, isDragReject } = useDropzone({
    onDrop,
    accept: {
      'application/pdf': ['.pdf'],
      'image/png': ['.png'],
      'image/jpeg': ['.jpg', '.jpeg'],
      'image/tiff': ['.tiff', '.tif'],
    },
    maxSize: 50 * 1024 * 1024, // 50MB per file
  });

  const handleUpload = () => {
    const pendingFiles = fileItems
      .filter((f) => f.status === 'pending' || f.status === 'error')
      .map((f) => f.file);
    if (pendingFiles.length === 0) return;
    uploadMutation.mutate(pendingFiles);
  };

  const removeFile = (index: number) => {
    setFileItems((prev) => prev.filter((_, i) => i !== index));
  };

  const clearAll = () => {
    setFileItems([]);
    setBatchResult(null);
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

  const getStatusIcon = (status: FileWithStatus['status']) => {
    switch (status) {
      case 'uploading':
        return <Loader2 className="w-5 h-5 text-capture animate-spin" />;
      case 'success':
        return <CheckCircle className="w-5 h-5 text-success" />;
      case 'error':
        return <AlertCircle className="w-5 h-5 text-error" />;
      default:
        return <Clock className="w-5 h-5 text-muted-foreground" />;
    }
  };

  const stats = statsQuery.data as OcrPipelineStats | undefined;
  const pendingCount = fileItems.filter((f) => f.status === 'pending' || f.status === 'error').length;
  const successCount = fileItems.filter((f) => f.status === 'success').length;
  const errorCount = fileItems.filter((f) => f.status === 'error').length;

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <Link
          href="/invoices"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Invoices
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Batch Upload Invoices</h1>
        <p className="text-muted-foreground mt-0.5">
          Upload multiple invoice documents for automatic OCR processing
        </p>
      </div>

      {/* Pipeline Stats */}
      {stats && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="card p-4">
            <div className="flex items-center gap-2 mb-1">
              <BarChart3 className="w-4 h-4 text-capture" />
              <span className="text-xs text-muted-foreground">Total Jobs</span>
            </div>
            <p className="text-xl font-semibold text-foreground">{stats.total_jobs}</p>
          </div>
          <div className="card p-4">
            <div className="flex items-center gap-2 mb-1">
              <Loader2 className="w-4 h-4 text-processing" />
              <span className="text-xs text-muted-foreground">Processing</span>
            </div>
            <p className="text-xl font-semibold text-foreground">{stats.processing_jobs}</p>
          </div>
          <div className="card p-4">
            <div className="flex items-center gap-2 mb-1">
              <CheckCircle className="w-4 h-4 text-success" />
              <span className="text-xs text-muted-foreground">Completed</span>
            </div>
            <p className="text-xl font-semibold text-foreground">{stats.completed_jobs}</p>
          </div>
          <div className="card p-4">
            <div className="flex items-center gap-2 mb-1">
              <AlertCircle className="w-4 h-4 text-error" />
              <span className="text-xs text-muted-foreground">Failed</span>
            </div>
            <p className="text-xl font-semibold text-foreground">{stats.failed_jobs}</p>
          </div>
        </div>
      )}

      {/* Upload Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-capture to-capture/50" />
        <div className="p-6">
          {/* Drop Zone */}
          <div
            {...getRootProps()}
            className={`border-2 border-dashed rounded-xl p-8 text-center cursor-pointer transition-all duration-200 ${
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
                  className={`w-8 h-8 ${isDragActive ? 'text-capture' : 'text-muted-foreground'}`}
                />
              </div>
              <p className="text-lg font-medium text-foreground mb-1">
                {isDragReject
                  ? 'File type not supported'
                  : isDragActive
                  ? 'Drop your invoices here'
                  : 'Drag & drop invoices here'}
              </p>
              <p className="text-sm text-muted-foreground mb-3">
                or click to browse — upload multiple files at once
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
              <p className="text-xs text-muted-foreground mt-2">
                Maximum 50MB per file
              </p>
            </div>
          </div>

          {/* File List */}
          {fileItems.length > 0 && (
            <div className="mt-6 space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-foreground">
                  {fileItems.length} file{fileItems.length > 1 ? 's' : ''} selected
                </h3>
                <button
                  onClick={clearAll}
                  className="text-xs text-muted-foreground hover:text-foreground transition-colors"
                  disabled={uploadMutation.isPending}
                >
                  Clear all
                </button>
              </div>

              <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
                {fileItems.map((item, index) => (
                  <div
                    key={`${item.file.name}-${index}`}
                    className="flex items-center gap-3 p-3 bg-card hover:bg-secondary/30 transition-colors"
                  >
                    <div className="p-2 bg-secondary rounded-lg">
                      {getFileIcon(item.file)}
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-foreground truncate">
                        {item.file.name}
                      </p>
                      <div className="flex items-center gap-2">
                        <span className="text-xs text-muted-foreground">
                          {(item.file.size / 1024 / 1024).toFixed(2)} MB
                        </span>
                        {item.error && (
                          <span className="text-xs text-error">{item.error}</span>
                        )}
                        {item.jobId && (
                          <span className="text-xs text-success">Job created</span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {getStatusIcon(item.status)}
                      {!uploadMutation.isPending && item.status !== 'uploading' && (
                        <button
                          onClick={() => removeFile(index)}
                          className="p-1 text-muted-foreground hover:text-foreground rounded transition-colors"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>

              {/* Batch Summary */}
              {batchResult && (
                <div className="p-4 bg-secondary/50 rounded-xl space-y-2">
                  <h4 className="text-sm font-medium text-foreground">Upload Summary</h4>
                  <div className="grid grid-cols-3 gap-4 text-center">
                    <div>
                      <p className="text-lg font-semibold text-foreground">{batchResult.total_files}</p>
                      <p className="text-xs text-muted-foreground">Total Files</p>
                    </div>
                    <div>
                      <p className="text-lg font-semibold text-success">{batchResult.jobs_created}</p>
                      <p className="text-xs text-muted-foreground">Jobs Created</p>
                    </div>
                    <div>
                      <p className="text-lg font-semibold text-error">{batchResult.errors.length}</p>
                      <p className="text-xs text-muted-foreground">Errors</p>
                    </div>
                  </div>
                </div>
              )}

              {/* Actions */}
              <div className="flex justify-end gap-3">
                <button
                  onClick={clearAll}
                  className="btn btn-secondary"
                  disabled={uploadMutation.isPending}
                >
                  Cancel
                </button>
                <button
                  onClick={handleUpload}
                  className="btn bg-capture text-capture-foreground hover:bg-capture/90 shadow-sm"
                  disabled={uploadMutation.isPending || pendingCount === 0}
                >
                  {uploadMutation.isPending ? (
                    <>
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      Uploading...
                    </>
                  ) : (
                    <>
                      <ScanLine className="w-4 h-4 mr-2" />
                      Upload {pendingCount} File{pendingCount !== 1 ? 's' : ''}
                    </>
                  )}
                </button>
              </div>
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
