'use client';

import { useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { useDropzone } from 'react-dropzone';
import { useMutation } from '@tanstack/react-query';
import { invoicesApi } from '@/lib/api';
import { toast } from 'sonner';
import { Upload, FileText, X, Loader2 } from 'lucide-react';

export default function UploadInvoicePage() {
  const router = useRouter();
  const [files, setFiles] = useState<File[]>([]);

  const uploadMutation = useMutation({
    mutationFn: (file: File) => invoicesApi.upload(file),
    onSuccess: (data) => {
      toast.success('Invoice uploaded successfully!');
      router.push(`/invoices/${data.invoice_id}`);
    },
    onError: (error: any) => {
      toast.error(error.message || 'Upload failed');
    },
  });

  const onDrop = useCallback((acceptedFiles: File[]) => {
    setFiles(acceptedFiles);
  }, []);

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
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
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6">
      {/* Page header */}
      <div>
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
          Upload Invoice
        </h1>
        <p className="text-slate-500 dark:text-slate-400">
          Upload an invoice document for OCR processing
        </p>
      </div>

      {/* Upload area */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6">
        {files.length === 0 ? (
          <div
            {...getRootProps()}
            className={`border-2 border-dashed rounded-xl p-12 text-center cursor-pointer transition-colors ${
              isDragActive
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                : 'border-slate-300 dark:border-slate-600 hover:border-slate-400 dark:hover:border-slate-500'
            }`}
          >
            <input {...getInputProps()} />
            <div className="flex flex-col items-center">
              <div className="p-4 bg-slate-100 dark:bg-slate-700 rounded-full mb-4">
                <Upload className="w-8 h-8 text-slate-400" />
              </div>
              <p className="text-lg font-medium text-slate-900 dark:text-white mb-2">
                {isDragActive ? 'Drop the file here' : 'Drag & drop your invoice'}
              </p>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">
                or click to browse
              </p>
              <p className="text-xs text-slate-400">
                Supports PDF, PNG, JPEG, TIFF (max 10MB)
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="flex items-center p-4 bg-slate-50 dark:bg-slate-700/50 rounded-lg">
              <div className="p-3 bg-blue-100 dark:bg-blue-900/30 rounded-lg mr-4">
                <FileText className="w-6 h-6 text-blue-500" />
              </div>
              <div className="flex-1 min-w-0">
                <p className="font-medium text-slate-900 dark:text-white truncate">
                  {files[0].name}
                </p>
                <p className="text-sm text-slate-500 dark:text-slate-400">
                  {(files[0].size / 1024 / 1024).toFixed(2)} MB
                </p>
              </div>
              <button
                onClick={removeFile}
                className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <div className="flex justify-end space-x-3">
              <button
                onClick={removeFile}
                className="px-4 py-2 text-slate-600 dark:text-slate-300 bg-slate-100 dark:bg-slate-700 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleUpload}
                disabled={uploadMutation.isPending}
                className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center space-x-2"
              >
                {uploadMutation.isPending ? (
                  <>
                    <Loader2 className="w-4 h-4 animate-spin" />
                    <span>Processing...</span>
                  </>
                ) : (
                  <>
                    <Upload className="w-4 h-4" />
                    <span>Upload & Process</span>
                  </>
                )}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Info box */}
      <div className="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-xl p-4">
        <h3 className="font-medium text-blue-900 dark:text-blue-200 mb-2">
          What happens next?
        </h3>
        <ol className="text-sm text-blue-700 dark:text-blue-300 space-y-2">
          <li>1. Your invoice will be processed using OCR technology</li>
          <li>2. Key fields like vendor, amount, and date will be extracted</li>
          <li>3. You'll be able to review and correct any extracted data</li>
          <li>4. Once reviewed, submit the invoice for approval</li>
        </ol>
      </div>
    </div>
  );
}
