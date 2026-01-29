'use client';

import { useEffect, useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { Command } from 'cmdk';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets } from '@/stores/theme';
import { cn } from '@/lib/utils';
import {
  Search,
  FileText,
  Users,
  BarChart3,
  Settings,
  Upload,
  Plus,
  Home,
  ClipboardCheck,
  ScanLine,
  Sun,
  Moon,
  Palette,
  FolderOpen,
  ListChecks,
  Workflow,
  LogOut,
  ArrowRight,
} from 'lucide-react';

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  icon: React.ComponentType<{ className?: string }>;
  href?: string;
  action?: () => void;
  keywords?: string[];
  module?: string;
  group: 'navigation' | 'actions' | 'settings' | 'theme';
}

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const router = useRouter();
  const { hasModule, logout } = useAuthStore();
  const { mode, setMode, setPreset, presetId } = useThemeStore();

  // Toggle the menu when Cmd+K or Ctrl+K is pressed
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === 'k' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((open) => !open);
      }
    };

    document.addEventListener('keydown', down);
    return () => document.removeEventListener('keydown', down);
  }, []);

  const runCommand = useCallback((command: () => void) => {
    setOpen(false);
    command();
  }, []);

  const commands: CommandItem[] = [
    // Navigation
    {
      id: 'dashboard',
      label: 'Go to Dashboard',
      icon: Home,
      href: '/dashboard',
      keywords: ['home', 'main'],
      group: 'navigation',
    },
    {
      id: 'invoices',
      label: 'Go to Invoices',
      icon: FileText,
      href: '/invoices',
      keywords: ['bills', 'documents'],
      module: 'invoice_capture',
      group: 'navigation',
    },
    {
      id: 'upload',
      label: 'Upload Invoice',
      description: 'Scan and process new invoice',
      icon: Upload,
      href: '/invoices/upload',
      keywords: ['scan', 'new', 'add'],
      module: 'invoice_capture',
      group: 'navigation',
    },
    {
      id: 'queues',
      label: 'Go to Work Queues',
      icon: FolderOpen,
      href: '/processing/queues',
      keywords: ['work', 'tasks'],
      module: 'invoice_processing',
      group: 'navigation',
    },
    {
      id: 'approvals',
      label: 'Go to Approvals',
      icon: ListChecks,
      href: '/processing/approvals',
      keywords: ['approve', 'review', 'pending'],
      module: 'invoice_processing',
      group: 'navigation',
    },
    {
      id: 'assignment-rules',
      label: 'Go to Assignment Rules',
      icon: Workflow,
      href: '/processing/assignment-rules',
      keywords: ['routing', 'rules'],
      module: 'invoice_processing',
      group: 'navigation',
    },
    {
      id: 'vendors',
      label: 'Go to Vendors',
      icon: Users,
      href: '/vendors',
      keywords: ['suppliers', 'companies'],
      module: 'vendor_management',
      group: 'navigation',
    },
    {
      id: 'new-vendor',
      label: 'Add New Vendor',
      description: 'Create a new vendor profile',
      icon: Plus,
      href: '/vendors/new',
      keywords: ['create', 'supplier'],
      module: 'vendor_management',
      group: 'navigation',
    },
    {
      id: 'reports',
      label: 'Go to Reports',
      icon: BarChart3,
      href: '/reports',
      keywords: ['analytics', 'metrics', 'charts'],
      module: 'reporting',
      group: 'navigation',
    },
    {
      id: 'settings',
      label: 'Go to Settings',
      icon: Settings,
      href: '/settings',
      keywords: ['preferences', 'config'],
      group: 'navigation',
    },
    // Actions
    {
      id: 'logout',
      label: 'Log Out',
      icon: LogOut,
      action: () => {
        logout();
        router.push('/login');
      },
      keywords: ['sign out', 'exit'],
      group: 'actions',
    },
    // Theme Settings
    {
      id: 'theme-light',
      label: 'Switch to Light Mode',
      icon: Sun,
      action: () => setMode('light'),
      keywords: ['bright', 'day'],
      group: 'theme',
    },
    {
      id: 'theme-dark',
      label: 'Switch to Dark Mode',
      icon: Moon,
      action: () => setMode('dark'),
      keywords: ['night', 'dark'],
      group: 'theme',
    },
  ];

  // Add theme preset commands
  const themeCommands: CommandItem[] = themePresets.slice(0, 8).map((preset) => ({
    id: `theme-${preset.id}`,
    label: `Apply ${preset.name} Theme`,
    description: preset.description,
    icon: Palette,
    action: () => setPreset(preset.id),
    keywords: ['color', 'theme', preset.category],
    group: 'theme' as const,
  }));

  const allCommands = [...commands, ...themeCommands];

  // Filter commands by module access
  const filteredCommands = allCommands.filter((cmd) => {
    if (cmd.module && !hasModule(cmd.module)) return false;
    return true;
  });

  const groupedCommands = {
    navigation: filteredCommands.filter((c) => c.group === 'navigation'),
    actions: filteredCommands.filter((c) => c.group === 'actions'),
    theme: filteredCommands.filter((c) => c.group === 'theme'),
  };

  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      label="Command Menu"
      className={cn(
        'fixed inset-0 z-50',
        'flex items-start justify-center pt-[20vh]',
        'bg-background/80 backdrop-blur-sm'
      )}
    >
      <div
        className={cn(
          'w-full max-w-lg mx-4',
          'bg-card border border-border rounded-2xl shadow-soft-lg',
          'overflow-hidden animate-scale-in'
        )}
      >
        {/* Search Input */}
        <div className="flex items-center gap-3 px-4 border-b border-border">
          <Search className="w-5 h-5 text-muted-foreground flex-shrink-0" />
          <Command.Input
            value={search}
            onValueChange={setSearch}
            placeholder="Type a command or search..."
            className={cn(
              'flex-1 h-14 bg-transparent',
              'text-base text-foreground placeholder:text-muted-foreground',
              'focus:outline-none'
            )}
          />
          <kbd className="hidden sm:inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-muted-foreground bg-secondary rounded-md">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <Command.List className="max-h-[400px] overflow-y-auto p-2">
          <Command.Empty className="py-8 text-center text-muted-foreground">
            No results found.
          </Command.Empty>

          {/* Navigation Group */}
          {groupedCommands.navigation.length > 0 && (
            <Command.Group heading="Navigation" className="px-2">
              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2 mt-3">
                Navigation
              </p>
              {groupedCommands.navigation.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.keywords?.join(' ')}`}
                  onSelect={() =>
                    runCommand(() =>
                      cmd.href ? router.push(cmd.href) : cmd.action?.()
                    )
                  }
                  className={cn(
                    'flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer',
                    'text-foreground data-[selected=true]:bg-primary/10 data-[selected=true]:text-primary',
                    'transition-colors'
                  )}
                >
                  <cmd.icon className="w-4 h-4 flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate">{cmd.label}</p>
                    {cmd.description && (
                      <p className="text-xs text-muted-foreground truncate">
                        {cmd.description}
                      </p>
                    )}
                  </div>
                  <ArrowRight className="w-4 h-4 text-muted-foreground opacity-0 data-[selected=true]:opacity-100" />
                </Command.Item>
              ))}
            </Command.Group>
          )}

          {/* Actions Group */}
          {groupedCommands.actions.length > 0 && (
            <Command.Group heading="Actions">
              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2 mt-4 px-2">
                Actions
              </p>
              {groupedCommands.actions.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.keywords?.join(' ')}`}
                  onSelect={() =>
                    runCommand(() =>
                      cmd.href ? router.push(cmd.href) : cmd.action?.()
                    )
                  }
                  className={cn(
                    'flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer',
                    'text-foreground data-[selected=true]:bg-primary/10 data-[selected=true]:text-primary',
                    'transition-colors'
                  )}
                >
                  <cmd.icon className="w-4 h-4 flex-shrink-0" />
                  <span className="text-sm font-medium">{cmd.label}</span>
                </Command.Item>
              ))}
            </Command.Group>
          )}

          {/* Theme Group */}
          {groupedCommands.theme.length > 0 && (
            <Command.Group heading="Theme">
              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2 mt-4 px-2">
                Theme
              </p>
              {groupedCommands.theme.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.keywords?.join(' ')}`}
                  onSelect={() =>
                    runCommand(() =>
                      cmd.href ? router.push(cmd.href) : cmd.action?.()
                    )
                  }
                  className={cn(
                    'flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer',
                    'text-foreground data-[selected=true]:bg-primary/10 data-[selected=true]:text-primary',
                    'transition-colors'
                  )}
                >
                  <cmd.icon className="w-4 h-4 flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate">{cmd.label}</p>
                    {cmd.description && (
                      <p className="text-xs text-muted-foreground truncate">
                        {cmd.description}
                      </p>
                    )}
                  </div>
                </Command.Item>
              ))}
            </Command.Group>
          )}
        </Command.List>

        {/* Footer */}
        <div className="flex items-center justify-between px-4 py-2.5 border-t border-border bg-secondary/30">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <kbd className="px-1.5 py-0.5 bg-secondary rounded font-mono">↑↓</kbd>
            <span>Navigate</span>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <kbd className="px-1.5 py-0.5 bg-secondary rounded font-mono">↵</kbd>
            <span>Select</span>
          </div>
        </div>
      </div>
    </Command.Dialog>
  );
}

// Trigger button for the command palette
export function CommandPaletteTrigger() {
  return (
    <button
      onClick={() => {
        const event = new KeyboardEvent('keydown', {
          key: 'k',
          metaKey: true,
          bubbles: true,
        });
        document.dispatchEvent(event);
      }}
      className={cn(
        'flex items-center gap-2 px-3 py-1.5 rounded-lg',
        'bg-secondary/50 hover:bg-secondary text-muted-foreground',
        'transition-colors text-sm'
      )}
    >
      <Search className="w-4 h-4" />
      <span className="hidden sm:inline">Search...</span>
      <kbd className="hidden sm:inline-flex items-center gap-0.5 px-1.5 py-0.5 text-xs font-medium bg-background/50 rounded border border-border">
        <span className="text-xs">⌘</span>K
      </kbd>
    </button>
  );
}
