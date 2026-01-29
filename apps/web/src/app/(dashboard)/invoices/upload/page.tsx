'use client';

import { useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { useDropzone } from 'react-dropzone';
import { useMutation } from '@tanstack/react-query';
import { invoicesApi } from '@/lib/api';
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
} from 'lucide-react';

export default function UploadInvoicePage() {
  const router = useRouter();
  const [files, setFiles] = useState<File[]>([]);
  const [uploadProgress, setUploadProgress] = useState(0);

  const uploadMutation = useMutation({
    mutationFn: async (file: File) => {
      // Simulate progress
      const progressInterval = setInterval(() => {
        setUploadProgress((prev) => Math.min(prev + 10, 90));
      }, 200);

      try {
        const result = await invoicesApi.upload(file);
        clearInterval(progressInterval);
        setUploadProgress(100);
        return result;
      } catch (error) {
        clearInterval(progressInterval);
        throw error;
      }
    },
    onSuccess: (data) => {
      toast.success('Invoice uploaded successfully!');
      setTimeout(() => {
        router.push(`/invoices/${data.invoice_id}`);
      }, 500);
    },
    onError: (error: any) => {
      setUploadProgress(0);
      toast.error(error.message || 'Upload failed');
    },
  });

  const onDrop = useCallback((acceptedFiles: File[]) => {
    setFiles(acceptedFiles);
    setUploadProgress(0);
  }, []);

  const { getRootProps, getInputProps, isDragActive, isDragReject } = useDropzone({
    onDrop,
    accept: {
      'application/pdf': ['.pdf'],
      'image/png': ['.png'],
      'image/jpeg': ['.jpg', '.jpeg'],
      'image/tiff': ['.tiff', '.tif'],
    },
    maxFiles: 1,
    maxSize: 10 * 1024 * 1024, // 10MB
  });

  const handleUpload = () => {
    if (files.length === 0) return;
    uploadMutation.mutate(files[0]);
  };

  const removeFile = () => {
    setFiles([]);
    setUploadProgress(0);
  };

  const getFileIcon = (file: File) => {
    if (file.type === 'application/pdf') {
      return <FileText className="w-8 h-8 text-error" />;
    }
    if (file.type.startsWith('image/')) {
      return <Image className="w-8 h-8 text-capture" />;
    }
    return <File className="w-8 h-8 text-muted-foreground" />;
  };

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
          Upload an invoice document for automatic OCR processing
        </p>
      </div>

      {/* Upload Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-capture to-capture/50" />
        <div className="p-6">
          {files.length === 0 ? (
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
                <div className={`p-4 rounded-2xl mb-4 transition-colors ${
                  isDragActive ? 'bg-capture/10' : 'bg-secondary'
                }`}>
                  <Upload className={`w-10 h-10 ${isDragActive ? 'text-capture' : 'text-muted-foreground'}`} />
                </div>
                <p className="text-lg font-medium text-foreground mb-2">
                  {isDragReject
                    ? 'File type not supported'
                    : isDragActive
                    ? 'Drop your invoice here'
                    : 'Drag & drop your invoice'}
                </p>
                <p className="text-sm text-muted-foreground mb-4">
                  or click to browse your files
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
                  Maximum file size: 10MB
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              {/* File Preview */}
              <div className="flex items-center p-4 bg-secondary/50 rounded-xl border border-border">
                <div className="p-3 bg-card rounded-lg shadow-sm mr-4">
                  {getFileIcon(files[0])}
                </div>
                <div className="flex-1 min-w-0">
                  <p className="font-medium text-foreground truncate">
                    {files[0].name}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {(files[0].size / 1024 / 1024).toFixed(2)} MB
                  </p>
                </div>
                {!uploadMutation.isPending && (
                  <button
                    onClick={removeFile}
                    className="p-2 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors"
                  >
                    <X className="w-5 h-5" />
                  </button>
                )}
              </div>

              {/* Progress Bar */}
              {uploadMutation.isPending && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Processing...</span>
                    <span className="font-medium text-foreground">{uploadProgress}%</span>
                  </div>
                  <div className="h-2 bg-secondary rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-capture to-capture/70 rounded-full transition-all duration-300"
                      style={{ width: `${uploadProgress}%` }}
                    />
                  </div>
                </div>
              )}

              {/* Actions */}
              {!uploadMutation.isPending && (
                <div className="flex justify-end gap-3">
                  <button
                    onClick={removeFile}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleUpload}
                    className="btn bg-capture text-capture-foreground hover:bg-capture/90 shadow-sm"
                  >
                    <ScanLine className="w-4 h-4 mr-2" />
                    Upload & Process
                  </button>
                </div>
              )}

              {/* Success State */}
              {uploadProgress === 100 && (
                <div className="flex items-center justify-center gap-2 text-success p-4">
                  <CheckCircle className="w-5 h-5" />
                  <span className="font-medium">Upload complete! Redirecting...</span>
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
              description: 'Your invoice is securely uploaded',
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
