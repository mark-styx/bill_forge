'use client';

import * as React from 'react';
import * as AlertDialog from '@radix-ui/react-alert-dialog';
import { cn } from '@/lib/utils';
import { AlertTriangle, Check, Info, Trash2, X, Loader2 } from 'lucide-react';
import { Button } from './button';

export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void | Promise<void>;
  onCancel?: () => void;
  variant?: 'default' | 'destructive' | 'warning' | 'info';
  loading?: boolean;
  icon?: React.ReactNode;
  children?: React.ReactNode;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  onConfirm,
  onCancel,
  variant = 'default',
  loading = false,
  icon,
  children,
}: ConfirmDialogProps) {
  const [isLoading, setIsLoading] = React.useState(false);

  const handleConfirm = async () => {
    setIsLoading(true);
    try {
      await onConfirm();
      onOpenChange(false);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCancel = () => {
    onCancel?.();
    onOpenChange(false);
  };

  const variantStyles = {
    default: {
      icon: <Check className="w-6 h-6 text-primary" />,
      iconBg: 'bg-primary/10',
      confirmVariant: 'default' as const,
    },
    destructive: {
      icon: <Trash2 className="w-6 h-6 text-destructive" />,
      iconBg: 'bg-destructive/10',
      confirmVariant: 'destructive' as const,
    },
    warning: {
      icon: <AlertTriangle className="w-6 h-6 text-warning" />,
      iconBg: 'bg-warning/10',
      confirmVariant: 'warning' as const,
    },
    info: {
      icon: <Info className="w-6 h-6 text-primary" />,
      iconBg: 'bg-primary/10',
      confirmVariant: 'default' as const,
    },
  };

  const styles = variantStyles[variant];

  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm animate-fade-in" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-2xl bg-card border border-border shadow-lg animate-scale-in p-6">
          <div className="flex flex-col items-center text-center">
            {/* Icon */}
            <div className={cn('w-14 h-14 rounded-full flex items-center justify-center mb-4', styles.iconBg)}>
              {icon || styles.icon}
            </div>

            {/* Title */}
            <AlertDialog.Title className="text-lg font-semibold text-foreground">
              {title}
            </AlertDialog.Title>

            {/* Description */}
            {description && (
              <AlertDialog.Description className="mt-2 text-sm text-muted-foreground">
                {description}
              </AlertDialog.Description>
            )}

            {/* Custom content */}
            {children && <div className="mt-4 w-full">{children}</div>}

            {/* Actions */}
            <div className="flex items-center justify-center gap-3 mt-6 w-full">
              <AlertDialog.Cancel asChild>
                <Button
                  variant="outline"
                  onClick={handleCancel}
                  disabled={isLoading || loading}
                  className="flex-1"
                >
                  {cancelLabel}
                </Button>
              </AlertDialog.Cancel>
              <Button
                variant={styles.confirmVariant}
                onClick={handleConfirm}
                disabled={isLoading || loading}
                className="flex-1"
              >
                {(isLoading || loading) && (
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                )}
                {confirmLabel}
              </Button>
            </div>
          </div>

          {/* Close button */}
          <button
            onClick={handleCancel}
            className="absolute top-4 right-4 p-1 rounded-lg hover:bg-secondary transition-colors"
          >
            <X className="w-4 h-4 text-muted-foreground" />
          </button>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

// Confirmation hook for easy usage
export interface UseConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: ConfirmDialogProps['variant'];
}

export function useConfirm() {
  const [dialogState, setDialogState] = React.useState<{
    open: boolean;
    options: UseConfirmOptions;
    resolve: ((value: boolean) => void) | null;
  }>({
    open: false,
    options: { title: '' },
    resolve: null,
  });

  const confirm = React.useCallback((options: UseConfirmOptions): Promise<boolean> => {
    return new Promise((resolve) => {
      setDialogState({
        open: true,
        options,
        resolve,
      });
    });
  }, []);

  const handleConfirm = () => {
    dialogState.resolve?.(true);
    setDialogState((prev) => ({ ...prev, open: false }));
  };

  const handleCancel = () => {
    dialogState.resolve?.(false);
    setDialogState((prev) => ({ ...prev, open: false }));
  };

  const ConfirmDialogComponent = (
    <ConfirmDialog
      open={dialogState.open}
      onOpenChange={(open) => {
        if (!open) handleCancel();
      }}
      title={dialogState.options.title}
      description={dialogState.options.description}
      confirmLabel={dialogState.options.confirmLabel}
      cancelLabel={dialogState.options.cancelLabel}
      variant={dialogState.options.variant}
      onConfirm={handleConfirm}
      onCancel={handleCancel}
    />
  );

  return { confirm, ConfirmDialog: ConfirmDialogComponent };
}

// Delete confirmation dialog
export interface DeleteDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title?: string;
  description?: string;
  itemName?: string;
  onConfirm: () => void | Promise<void>;
  loading?: boolean;
}

export function DeleteDialog({
  open,
  onOpenChange,
  title = 'Delete Item',
  description,
  itemName,
  onConfirm,
  loading,
}: DeleteDialogProps) {
  const defaultDescription = itemName
    ? `Are you sure you want to delete "${itemName}"? This action cannot be undone.`
    : 'Are you sure you want to delete this item? This action cannot be undone.';

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={description || defaultDescription}
      confirmLabel="Delete"
      cancelLabel="Cancel"
      variant="destructive"
      onConfirm={onConfirm}
      loading={loading}
    />
  );
}

// Unsaved changes dialog
export interface UnsavedChangesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave?: () => void | Promise<void>;
  onDiscard: () => void;
  loading?: boolean;
}

export function UnsavedChangesDialog({
  open,
  onOpenChange,
  onSave,
  onDiscard,
  loading,
}: UnsavedChangesDialogProps) {
  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm animate-fade-in" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-2xl bg-card border border-border shadow-lg animate-scale-in p-6">
          <div className="flex flex-col items-center text-center">
            <div className="w-14 h-14 rounded-full flex items-center justify-center bg-warning/10 mb-4">
              <AlertTriangle className="w-6 h-6 text-warning" />
            </div>

            <AlertDialog.Title className="text-lg font-semibold text-foreground">
              Unsaved Changes
            </AlertDialog.Title>

            <AlertDialog.Description className="mt-2 text-sm text-muted-foreground">
              You have unsaved changes. Do you want to save them before leaving?
            </AlertDialog.Description>

            <div className="flex items-center justify-center gap-3 mt-6 w-full">
              <AlertDialog.Cancel asChild>
                <Button
                  variant="outline"
                  onClick={onDiscard}
                  disabled={loading}
                >
                  Discard
                </Button>
              </AlertDialog.Cancel>
              {onSave && (
                <Button onClick={onSave} disabled={loading}>
                  {loading && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
                  Save Changes
                </Button>
              )}
            </div>
          </div>

          <button
            onClick={() => onOpenChange(false)}
            className="absolute top-4 right-4 p-1 rounded-lg hover:bg-secondary transition-colors"
          >
            <X className="w-4 h-4 text-muted-foreground" />
          </button>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
