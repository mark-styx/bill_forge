'use client';

import { useEffect, useState } from 'react';
import { useRouter, usePathname } from 'next/navigation';
import Link from 'next/link';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore } from '@/stores/theme';
import { useOrganizationTheme } from '@/components/organization-theme-provider';
import { CommandPalette, CommandPaletteTrigger } from '@/components/ui/command-palette';
import { NotificationCenter, type Notification } from '@/components/ui/notification-center';
import {
  FileText,
  ClipboardCheck,
  Users,
  BarChart3,
  Settings,
  LogOut,
  Home,
  ChevronDown,
  ScanLine,
  ChevronLeft,
  ChevronRight,
  AlertTriangle,
  FolderOpen,
  Workflow,
  ListChecks,
  Sparkles,
  Bell,
} from 'lucide-react';

interface NavItem {
  name: string;
  href: string;
  icon: typeof Home;
  module: string | null;
  children?: NavItem[];
}

const navigation: NavItem[] = [
  { name: 'Dashboard', href: '/dashboard', icon: Home, module: null },
  {
    name: 'Invoice Capture',
    href: '/invoices',
    icon: ScanLine,
    module: 'invoice_capture',
    children: [
      { name: 'Invoices', href: '/invoices', icon: FileText, module: 'invoice_capture' },
      { name: 'Errors', href: '/processing/queues?type=exception', icon: AlertTriangle, module: 'invoice_capture' },
    ]
  },
  {
    name: 'Processing',
    href: '/processing',
    icon: ClipboardCheck,
    module: 'invoice_processing',
    children: [
      { name: 'Work Queues', href: '/processing/queues', icon: FolderOpen, module: 'invoice_processing' },
      { name: 'Approvals', href: '/processing/approvals', icon: ListChecks, module: 'invoice_processing' },
      { name: 'Workflows', href: '/processing/workflows', icon: Sparkles, module: 'invoice_processing' },
      { name: 'Assignment Rules', href: '/processing/assignment-rules', icon: Workflow, module: 'invoice_processing' },
    ]
  },
  { name: 'Vendors', href: '/vendors', icon: Users, module: 'vendor_management' },
  { name: 'Reports', href: '/reports', icon: BarChart3, module: 'reporting' },
  { name: 'Integrations', href: '/integrations', icon: Workflow, module: null },
];

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const { isAuthenticated, user, tenant, logout, hasModule } = useAuthStore();
  const { sidebarCollapsed, toggleSidebar, getCurrentColors } = useThemeStore();
  const { getBrandGradient } = useOrganizationTheme();

  const [expandedSections, setExpandedSections] = useState<string[]>(['Invoice Capture', 'Processing']);

  const colors = getCurrentColors();
  const brandGradient = getBrandGradient();

  // Notifications driven by real data in production — using realistic samples for demo
  const [notifications, setNotifications] = useState<Notification[]>([
    {
      id: '1',
      type: 'success',
      title: 'Invoice approved',
      message: 'AWS-2024-JAN ($15,234.67) approved by Sarah Chen.',
      timestamp: new Date(Date.now() - 5 * 60 * 1000),
      read: false,
      actionUrl: '/processing/approvals',
      actionLabel: 'View',
      module: 'processing',
    },
    {
      id: '2',
      type: 'warning',
      title: 'Overdue invoice',
      message: 'ACME-2023-OLD is 60+ days past due. Needs attention.',
      timestamp: new Date(Date.now() - 30 * 60 * 1000),
      read: false,
      actionUrl: '/processing/approvals',
      module: 'processing',
    },
    {
      id: '3',
      type: 'info',
      title: '5 invoices ready for payment',
      message: '$7,213.45 total across 5 approved invoices.',
      timestamp: new Date(Date.now() - 2 * 60 * 60 * 1000),
      read: true,
      actionUrl: '/processing/queues',
      module: 'processing',
    },
  ]);

  // Close sidebar on Escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !sidebarCollapsed) {
        toggleSidebar();
      }
    };
    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [sidebarCollapsed, toggleSidebar]);

  const hasHydrated = useAuthStore((s) => s.hasHydrated);

  useEffect(() => {
    if (hasHydrated && !isAuthenticated) {
      router.push('/login');
    }
  }, [hasHydrated, isAuthenticated, router]);

  if (!hasHydrated) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="animate-pulse text-muted-foreground">Loading...</div>
      </div>
    );
  }

  if (!isAuthenticated) return null;

  const handleLogout = () => {
    logout();
    router.push('/login');
  };

  const toggleSection = (name: string) => {
    setExpandedSections(prev =>
      prev.includes(name) ? prev.filter(s => s !== name) : [...prev, name]
    );
  };

  const isNavItemActive = (item: NavItem): boolean => {
    if (pathname === item.href) return true;
    if (item.children) {
      return item.children.some(child => pathname === child.href || pathname.startsWith(`${child.href}/`));
    }
    return pathname.startsWith(`${item.href}/`);
  };

  return (
    <div className="min-h-screen bg-background">
      {/* Sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 bg-card border-r border-border z-30 transition-all duration-300 flex flex-col ${
          sidebarCollapsed ? 'w-16' : 'w-60'
        }`}
      >
        {/* Logo */}
        <div className="h-14 flex items-center justify-between px-3 border-b border-border flex-shrink-0">
          <Link href="/dashboard" className="flex items-center gap-2.5 group">
            <div
              className="w-8 h-8 rounded-xl flex items-center justify-center shadow-lg transition-transform group-hover:scale-105"
              style={{ background: brandGradient }}
            >
              <FileText className="w-4 h-4 text-white" />
            </div>
            {!sidebarCollapsed && (
              <span className="text-base font-semibold text-foreground">
                {tenant?.settings?.company_name || 'BillForge'}
              </span>
            )}
          </Link>
          <button
            onClick={toggleSidebar}
            className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
          >
            {sidebarCollapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
          </button>
        </div>

        {/* Navigation */}
        <nav className="flex-1 p-2 space-y-0.5 overflow-y-auto">
          {navigation.map((item) => {
            if (item.module && !hasModule(item.module)) return null;

            const isActive = isNavItemActive(item);
            const hasChildren = item.children && item.children.length > 0;
            const isExpanded = expandedSections.includes(item.name);

            const visibleChildren = item.children?.filter(child =>
              !child.module || hasModule(child.module)
            ) || [];

            if (hasChildren && visibleChildren.length > 0) {
              return (
                <div key={item.name}>
                  {sidebarCollapsed ? (
                    <Link
                      href={item.href}
                      className={`bright-nav-item justify-center px-2 ${isActive ? 'active' : ''}`}
                      title={item.name}
                    >
                      <item.icon className="w-5 h-5 flex-shrink-0" />
                    </Link>
                  ) : (
                    <>
                      <button
                        onClick={() => toggleSection(item.name)}
                        className={`bright-nav-item w-full ${isActive ? 'active' : ''}`}
                      >
                        <item.icon className="w-5 h-5 flex-shrink-0" />
                        <span className="flex-1 text-left">{item.name}</span>
                        <ChevronDown className={`w-4 h-4 transition-transform ${isExpanded ? 'rotate-180' : ''}`} />
                      </button>

                      {isExpanded && (
                        <div className="ml-4 pl-3 border-l border-border/50 space-y-0.5 mt-0.5">
                          {visibleChildren.map((child) => {
                            const isChildActive = pathname === child.href || pathname.startsWith(`${child.href}/`);
                            return (
                              <Link
                                key={child.name}
                                href={child.href}
                                className={`bright-nav-item text-sm py-2 ${isChildActive ? 'active' : ''}`}
                              >
                                <child.icon className="w-4 h-4 flex-shrink-0" />
                                <span>{child.name}</span>
                              </Link>
                            );
                          })}
                        </div>
                      )}
                    </>
                  )}
                </div>
              );
            }

            return (
              <Link
                key={item.name}
                href={item.href}
                className={`bright-nav-item ${isActive ? 'active' : ''} ${sidebarCollapsed ? 'justify-center px-2' : ''}`}
                title={sidebarCollapsed ? item.name : undefined}
              >
                <item.icon className="w-5 h-5 flex-shrink-0" />
                {!sidebarCollapsed && <span>{item.name}</span>}
              </Link>
            );
          })}
        </nav>

        {/* Bottom section */}
        <div className="border-t border-border flex-shrink-0">
          {/* User */}
          <div className={`p-3 ${sidebarCollapsed ? 'flex justify-center' : ''}`}>
            {sidebarCollapsed ? (
              <button
                onClick={handleLogout}
                className="p-2 rounded-xl text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                title="Log out"
              >
                <LogOut className="w-5 h-5" />
              </button>
            ) : (
              <div className="flex items-center gap-2.5">
                <div
                  className="w-9 h-9 rounded-xl flex items-center justify-center text-white text-sm font-medium shadow-md"
                  style={{ background: brandGradient }}
                >
                  {user?.name?.[0]?.toUpperCase() || user?.email?.[0]?.toUpperCase() || 'U'}
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground truncate">{user?.name || user?.email}</p>
                  <p className="text-xs text-muted-foreground truncate">{tenant?.settings?.company_name}</p>
                </div>
                <button
                  onClick={handleLogout}
                  className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                >
                  <LogOut className="w-4 h-4" />
                </button>
              </div>
            )}
          </div>
        </div>
      </aside>

      {/* Command Palette */}
      <CommandPalette />

      {/* Main content */}
      <div className={`transition-all duration-300 ${sidebarCollapsed ? 'pl-16' : 'pl-60'}`}>
        {/* Header */}
        <header className="h-14 bg-card/80 backdrop-blur-md border-b border-border sticky top-0 z-20">
          <div className="h-full px-4 flex items-center justify-between gap-4">
            {/* Command Palette Trigger */}
            <div className="flex-1 max-w-md">
              <CommandPaletteTrigger />
            </div>

            {/* Right actions */}
            <div className="flex items-center gap-2">
              {/* Notifications */}
              <NotificationCenter
                notifications={notifications}
                onMarkAsRead={(id) => {
                  setNotifications((prev) =>
                    prev.map((n) => (n.id === id ? { ...n, read: true } : n))
                  );
                }}
                onMarkAllAsRead={() => {
                  setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
                }}
                onDelete={(id) => {
                  setNotifications((prev) => prev.filter((n) => n.id !== id));
                }}
                onClearAll={() => setNotifications([])}
              />

              {/* Settings */}
              <Link
                href="/settings"
                className="p-2 rounded-xl text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
              >
                <Settings className="w-5 h-5" />
              </Link>
            </div>
          </div>
        </header>

        {/* Page content */}
        <main className="p-6 animate-fade-in">{children}</main>
      </div>
    </div>
  );
}
