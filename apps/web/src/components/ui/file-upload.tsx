'use client';

import * as React from 'react';
import { useDropzone, type Accept, type FileRejection } from 'react-dropzone';
import { cn } from '@/lib/utils';
import {
  Upload,
  X,
  FileText,
  Image,
  FileSpreadsheet,
  File,
  AlertCircle,
  CheckCircle,
  Loader2,
  Cloud,
  Trash2,
} from 'lucide-react';
import { Button } from './button';
import { Progress } from './progress';

export interface UploadedFile {
  id: string;
  file: File;
  name: string;
  size: number;
  type: string;
  progress: number;
  status: 'pending' | 'uploading' | 'success' | 'error';
  error?: string;
  url?: string;
}

export interface FileUploadProps {
  value?: UploadedFile[];
  onChange?: (files: UploadedFile[]) => void;
  onUpload?: (files: File[]) => Promise<void>;
  accept?: Accept;
  maxFiles?: number;
  maxSize?: number; // in bytes
  multiple?: boolean;
  disabled?: boolean;
  className?: string;
  variant?: 'default' | 'compact' | 'avatar';
  showPreview?: boolean;
  showProgress?: boolean;
  autoUpload?: boolean;
  acceptDescription?: string;
}

const fileIcons: Record<string, React.ReactNode> = {
  'image': <Image className="w-8 h-8 text-capture" />,
  'application/pdf': <FileText className="w-8 h-8 text-error" />,
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': (
    <FileSpreadsheet className="w-8 h-8 text-processing" />
  ),
  'application/vnd.ms-excel': <FileSpreadsheet className="w-8 h-8 text-processing" />,
  'text/csv': <FileSpreadsheet className="w-8 h-8 text-processing" />,
  'default': <File className="w-8 h-8 text-muted-foreground" />,
};

function getFileIcon(type: string): React.ReactNode {
  if (type.startsWith('image/')) return fileIcons['image'];
  return fileIcons[type] || fileIcons['default'];
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

export function FileUpload({
  value = [],
  onChange,
  onUpload,
  accept,
  maxFiles = 10,
  maxSize = 10 * 1024 * 1024, // 10MB default
  multiple = true,
  disabled = false,
  className,
  variant = 'default',
  showPreview = true,
  showProgress = true,
  autoUpload = false,
  acceptDescription,
}: FileUploadProps) {
  const [isDragActive, setIsDragActive] = React.useState(false);
  const [isUploading, setIsUploading] = React.useState(false);

  const handleDrop = React.useCallback(
    async (acceptedFiles: File[], rejectedFiles: FileRejection[]) => {
      setIsDragActive(false);

      // Handle rejected files
      if (rejectedFiles.length > 0) {
        const errors = rejectedFiles.map((rejection) => ({
          file: rejection.file,
          errors: rejection.errors.map((e) => e.message).join(', '),
        }));
        console.error('Rejected files:', errors);
      }

      // Create new file entries
      const newFiles: UploadedFile[] = acceptedFiles.map((file) => ({
        id: `${file.name}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
        file,
        name: file.name,
        size: file.size,
        type: file.type,
        progress: 0,
        status: 'pending',
      }));

      const updatedFiles = [...value, ...newFiles].slice(0, maxFiles);
      onChange?.(updatedFiles);

      // Auto upload if enabled
      if (autoUpload && onUpload) {
        setIsUploading(true);
        try {
          await onUpload(acceptedFiles);
          onChange?.(
            updatedFiles.map((f) =>
              newFiles.some((nf) => nf.id === f.id)
                ? { ...f, status: 'success' as const, progress: 100 }
                : f
            )
          );
        } catch (error) {
          onChange?.(
            updatedFiles.map((f) =>
              newFiles.some((nf) => nf.id === f.id)
                ? { ...f, status: 'error' as const, error: 'Upload failed' }
                : f
            )
          );
        } finally {
          setIsUploading(false);
        }
      }
    },
    [value, onChange, maxFiles, autoUpload, onUpload]
  );

  const { getRootProps, getInputProps, open } = useDropzone({
    onDrop: handleDrop,
    accept,
    maxFiles: maxFiles - value.length,
    maxSize,
    multiple,
    disabled: disabled || value.length >= maxFiles,
    noClick: variant === 'compact',
    onDragEnter: () => setIsDragActive(true),
    onDragLeave: () => setIsDragActive(false),
  });

  const removeFile = (id: string) => {
    onChange?.(value.filter((f) => f.id !== id));
  };

  const clearAll = () => {
    onChange?.([]);
  };

  // Compact variant for inline use
  if (variant === 'compact') {
    return (
      <div className={cn('space-y-2', className)}>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={open}
            disabled={disabled || value.length >= maxFiles}
          >
            <Upload className="w-4 h-4 mr-1.5" />
            Choose Files
          </Button>
          {value.length > 0 && (
            <span className="text-sm text-muted-foreground">
              {value.length} file{value.length !== 1 ? 's' : ''} selected
            </span>
          )}
        </div>
        <input {...getInputProps()} />
        {value.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {value.map((file) => (
              <div
                key={file.id}
                className="flex items-center gap-2 px-2.5 py-1.5 bg-secondary rounded-lg text-sm"
              >
                <span className="truncate max-w-[150px]">{file.name}</span>
                <button
                  type="button"
                  onClick={() => removeFile(file.id)}
                  className="p-0.5 hover:bg-secondary-foreground/10 rounded"
                >
                  <X className="w-3.5 h-3.5 text-muted-foreground" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // Avatar variant for profile pictures
  if (variant === 'avatar') {
    const currentFile = value[0];
    const previewUrl = currentFile?.file
      ? URL.createObjectURL(currentFile.file)
      : currentFile?.url;

    return (
      <div
        {...getRootProps()}
        className={cn(
          'relative w-24 h-24 rounded-full overflow-hidden cursor-pointer group',
          'border-2 border-dashed border-border hover:border-primary transition-colors',
          isDragActive && 'border-primary bg-primary/5',
          disabled && 'opacity-50 cursor-not-allowed',
          className
        )}
      >
        <input {...getInputProps()} />
        {previewUrl ? (
          <>
            <img
              src={previewUrl}
              alt="Preview"
              className="w-full h-full object-cover"
            />
            <div className="absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
              <Upload className="w-6 h-6 text-white" />
            </div>
          </>
        ) : (
          <div className="w-full h-full flex flex-col items-center justify-center bg-secondary">
            <Upload className="w-6 h-6 text-muted-foreground mb-1" />
            <span className="text-xs text-muted-foreground">Upload</span>
          </div>
        )}
      </div>
    );
  }

  // Default variant
  return (
    <div className={cn('space-y-4', className)}>
      {/* Drop zone */}
      <div
        {...getRootProps()}
        className={cn(
          'relative border-2 border-dashed rounded-2xl p-8 transition-all cursor-pointer',
          'hover:border-primary/50 hover:bg-primary/5',
          isDragActive && 'border-primary bg-primary/10 scale-[1.02]',
          disabled && 'opacity-50 cursor-not-allowed',
          value.length >= maxFiles && 'opacity-50 cursor-not-allowed'
        )}
      >
        <input {...getInputProps()} />
        <div className="flex flex-col items-center text-center">
          <div
            className={cn(
              'w-14 h-14 rounded-2xl flex items-center justify-center mb-4 transition-all',
              isDragActive
                ? 'bg-primary text-primary-foreground scale-110'
                : 'bg-secondary text-muted-foreground'
            )}
          >
            <Upload className="w-7 h-7" />
          </div>
          <h3 className="font-semibold text-foreground mb-1">
            {isDragActive ? 'Drop files here' : 'Drag & drop files here'}
          </h3>
          <p className="text-sm text-muted-foreground mb-3">
            or click to browse from your computer
          </p>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            {acceptDescription && <span>{acceptDescription}</span>}
            {!acceptDescription && maxSize && (
              <span>Max {formatFileSize(maxSize)} per file</span>
            )}
            {maxFiles > 1 && (
              <>
                <span className="w-1 h-1 bg-muted-foreground rounded-full" />
                <span>Up to {maxFiles} files</span>
              </>
            )}
          </div>
        </div>
      </div>

      {/* File list */}
      {value.length > 0 && showPreview && (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h4 className="text-sm font-medium text-foreground">
              {value.length} file{value.length !== 1 ? 's' : ''} selected
            </h4>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={clearAll}
              className="text-muted-foreground hover:text-foreground"
            >
              <Trash2 className="w-4 h-4 mr-1" />
              Clear all
            </Button>
          </div>

          <div className="space-y-2">
            {value.map((file) => (
              <div
                key={file.id}
                className={cn(
                  'flex items-center gap-3 p-3 rounded-xl border border-border bg-card',
                  file.status === 'error' && 'border-error/50 bg-error/5'
                )}
              >
                {/* File icon or preview */}
                <div className="w-12 h-12 rounded-lg bg-secondary flex items-center justify-center flex-shrink-0 overflow-hidden">
                  {file.type.startsWith('image/') && file.file ? (
                    <img
                      src={URL.createObjectURL(file.file)}
                      alt={file.name}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    getFileIcon(file.type)
                  )}
                </div>

                {/* File info */}
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground truncate">
                    {file.name}
                  </p>
                  <div className="flex items-center gap-2 mt-0.5">
                    <span className="text-xs text-muted-foreground">
                      {formatFileSize(file.size)}
                    </span>
                    {file.status === 'uploading' && showProgress && (
                      <Progress value={file.progress} className="h-1.5 w-20" />
                    )}
                    {file.status === 'success' && (
                      <span className="flex items-center gap-1 text-xs text-success">
                        <CheckCircle className="w-3.5 h-3.5" />
                        Uploaded
                      </span>
                    )}
                    {file.status === 'error' && (
                      <span className="flex items-center gap-1 text-xs text-error">
                        <AlertCircle className="w-3.5 h-3.5" />
                        {file.error || 'Failed'}
                      </span>
                    )}
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center gap-1">
                  {file.status === 'uploading' ? (
                    <Loader2 className="w-4 h-4 animate-spin text-primary" />
                  ) : (
                    <button
                      type="button"
                      onClick={() => removeFile(file.id)}
                      className="p-1.5 rounded-lg hover:bg-secondary transition-colors"
                    >
                      <X className="w-4 h-4 text-muted-foreground" />
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Upload button (when not auto-uploading) */}
      {!autoUpload && onUpload && value.some((f) => f.status === 'pending') && (
        <Button
          type="button"
          onClick={async () => {
            const pendingFiles = value.filter((f) => f.status === 'pending');
            setIsUploading(true);
            onChange?.(
              value.map((f) =>
                f.status === 'pending' ? { ...f, status: 'uploading' as const } : f
              )
            );
            try {
              await onUpload(pendingFiles.map((f) => f.file));
              onChange?.(
                value.map((f) =>
                  pendingFiles.some((pf) => pf.id === f.id)
                    ? { ...f, status: 'success' as const, progress: 100 }
                    : f
                )
              );
            } catch {
              onChange?.(
                value.map((f) =>
                  pendingFiles.some((pf) => pf.id === f.id)
                    ? { ...f, status: 'error' as const, error: 'Upload failed' }
                    : f
                )
              );
            } finally {
              setIsUploading(false);
            }
          }}
          disabled={isUploading}
          loading={isUploading}
          className="w-full"
        >
          <Upload className="w-4 h-4 mr-2" />
          Upload {value.filter((f) => f.status === 'pending').length} file
          {value.filter((f) => f.status === 'pending').length !== 1 ? 's' : ''}
        </Button>
      )}
    </div>
  );
}
