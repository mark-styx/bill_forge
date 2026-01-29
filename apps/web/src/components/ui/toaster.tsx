'use client';

import { Toaster as Sonner } from 'sonner';
import { useTheme } from 'next-themes';

type ToasterProps = React.ComponentProps<typeof Sonner>;

export function Toaster({ ...props }: ToasterProps) {
  const { theme = 'system' } = useTheme();

  return (
    <Sonner
      theme={theme as ToasterProps['theme']}
      className="toaster group"
      position="bottom-right"
      toastOptions={{
        classNames: {
          toast:
            'group toast group-[.toaster]:bg-card group-[.toaster]:text-foreground group-[.toaster]:border-border group-[.toaster]:shadow-soft-lg group-[.toaster]:rounded-xl',
          description: 'group-[.toast]:text-muted-foreground',
          actionButton:
            'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground group-[.toast]:rounded-lg group-[.toast]:font-medium',
          cancelButton:
            'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground group-[.toast]:rounded-lg',
          success:
            'group-[.toaster]:border-success/30 group-[.toaster]:bg-success/5',
          error:
            'group-[.toaster]:border-error/30 group-[.toaster]:bg-error/5',
          warning:
            'group-[.toaster]:border-warning/30 group-[.toaster]:bg-warning/5',
          info:
            'group-[.toaster]:border-primary/30 group-[.toaster]:bg-primary/5',
        },
      }}
      {...props}
    />
  );
}
