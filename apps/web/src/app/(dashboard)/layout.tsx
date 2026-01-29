'use client';

import { useEffect, useState } from 'react';
import { useRouter, usePathname } from 'next/navigation';
import Link from 'next/link';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore } from '@/stores/theme';
import { sandboxApi, PersonaInfo } from '@/lib/api';
import { toast } from 'sonner';
import { CommandPalette, CommandPaletteTrigger } from '@/components/ui/command-palette';
import {
  FileText,
  ClipboardCheck,
  Users,
  BarChart3,
  Settings,
  LogOut,
  Home,
  ChevronDown,
  Bell,
  ScanLine,
  Layers,
  RefreshCw,
  ChevronLeft,
  ChevronRight,
  Check,
  AlertTriangle,
  FolderOpen,
  Workflow,
  ListChecks,
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
      { name: 'Errors', href: '/processing/queues/11111111-4444-5555-6666-777777770001', icon: AlertTriangle, module: 'invoice_capture' },
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
      { name: 'Assignment Rules', href: '/processing/assignment-rules', icon: Workflow, module: 'invoice_processing' },
    ]
  },
  { name: 'Vendors', href: '/vendors', icon: Users, module: 'vendor_management' },
  { name: 'Reports', href: '/reports', icon: BarChart3, module: 'reporting' },
];

const personaIcons: Record<string, typeof FileText> = {
  full_platform: Layers,
  invoice_ocr_only: ScanLine,
  invoice_processing_only: ClipboardCheck,
  vendor_management_only: Users,
  ap_lite: FileText,
};

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const { isAuthenticated, user, tenant, currentPersona, logout, hasModule, switchPersona, refreshTenantContext } = useAuthStore();
  const { sidebarCollapsed, toggleSidebar } = useThemeStore();
  
  const [personas, setPersonas] = useState<PersonaInfo[]>([]);
  const [showPersonaSwitcher, setShowPersonaSwitcher] = useState(false);
  const [switchingPersona, setSwitchingPersona] = useState(false);
  const [expandedSections, setExpandedSections] = useState<string[]>(['Invoice Capture', 'Processing']);

  useEffect(() => {
    if (!isAuthenticated) {
      router.push('/login');
    }
  }, [isAuthenticated, router]);

  useEffect(() => {
    if (isAuthenticated) {
      sandboxApi.listPersonas()
        .then(setPersonas)
        .catch(() => setPersonas([]));
    }
  }, [isAuthenticated]);

  if (!isAuthenticated) {
    return null;
  }

  const handleLogout = () => {
    logout();
    router.push('/login');
  };

  const handleSwitchPersona = async (personaId: string) => {
    setSwitchingPersona(true);
    try {
      await switchPersona(personaId);
      await refreshTenantContext();
      toast.success('Configuration updated');
      setShowPersonaSwitcher(false);
      router.refresh();
    } catch (error: any) {
      toast.error(error.message || 'Failed to switch configuration');
    } finally {
      setSwitchingPersona(false);
    }
  };

  const toggleSection = (sectionName: string) => {
    setExpandedSections(prev => 
      prev.includes(sectionName) 
        ? prev.filter(s => s !== sectionName)
        : [...prev, sectionName]
    );
  };

  const isNavItemActive = (item: NavItem): boolean => {
    if (pathname === item.href) return true;
    if (item.children) {
      return item.children.some(child => pathname === child.href || pathname.startsWith(`${child.href}/`));
    }
    return pathname.startsWith(`${item.href}/`);
  };

  const CurrentPersonaIcon = currentPersona ? personaIcons[currentPersona.id] || Layers : Layers;

  return (
    <div className="min-h-screen bg-background">
      {/* Sidebar */}
      <aside 
        className={`fixed inset-y-0 left-0 bg-card border-r border-border z-30 transition-all duration-300 ${
          sidebarCollapsed ? 'w-16' : 'w-60'
        }`}
      >
        {/* Logo */}
        <div className="h-14 flex items-center justify-between px-3 border-b border-border">
          <Link href="/dashboard" className="flex items-center gap-2">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-primary to-accent flex items-center justify-center shadow-sm">
              <FileText className="w-4 h-4 text-white" />
            </div>
            {!sidebarCollapsed && (
              <span className="text-base font-semibold text-foreground">BillForge</span>
            )}
          </Link>
          <button
            onClick={toggleSidebar}
            className="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
          >
            {sidebarCollapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
          </button>
        </div>

        {/* Navigation */}
        <nav className="p-2 space-y-0.5 overflow-y-auto" style={{ maxHeight: 'calc(100vh - 180px)' }}>
          {navigation.map((item) => {
            // Check if module is enabled
            if (item.module && !hasModule(item.module)) return null;
            
            const isActive = isNavItemActive(item);
            const hasChildren = item.children && item.children.length > 0;
            const isExpanded = expandedSections.includes(item.name);

            // Filter children by module access
            const visibleChildren = item.children?.filter(child => 
              !child.module || hasModule(child.module)
            ) || [];

            if (hasChildren && visibleChildren.length > 0) {
              return (
                <div key={item.name}>
                  {sidebarCollapsed ? (
                    <Link
                      href={item.href}
                      className={`nav-item justify-center px-2 ${isActive ? 'nav-item-active' : 'nav-item-inactive'}`}
                      title={item.name}
                    >
                      <item.icon className="w-5 h-5 flex-shrink-0" />
                    </Link>
                  ) : (
                    <>
                      <button
                        onClick={() => toggleSection(item.name)}
                        className={`nav-item w-full ${isActive ? 'nav-item-active' : 'nav-item-inactive'}`}
                      >
                        <item.icon className="w-5 h-5 flex-shrink-0" />
                        <span className="flex-1 text-left">{item.name}</span>
                        <ChevronDown className={`w-4 h-4 transition-transform ${isExpanded ? 'rotate-180' : ''}`} />
                      </button>
                      
                      {isExpanded && (
                        <div className="ml-4 pl-3 border-l border-border space-y-0.5 mt-0.5">
                          {visibleChildren.map((child) => {
                            const isChildActive = pathname === child.href || pathname.startsWith(`${child.href}/`);
                            return (
                              <Link
                                key={child.name}
                                href={child.href}
                                className={`nav-item text-sm py-1.5 ${isChildActive ? 'nav-item-active' : 'nav-item-inactive'}`}
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
                className={`nav-item ${isActive ? 'nav-item-active' : 'nav-item-inactive'} ${sidebarCollapsed ? 'justify-center px-2' : ''}`}
                title={sidebarCollapsed ? item.name : undefined}
              >
                <item.icon className="w-5 h-5 flex-shrink-0" />
                {!sidebarCollapsed && <span>{item.name}</span>}
              </Link>
            );
          })}
        </nav>

        {/* Bottom section */}
        <div className="absolute bottom-0 left-0 right-0 border-t border-border">
          {/* Module badges */}
          {!sidebarCollapsed && (
            <div className="p-3">
              <div className="flex flex-wrap gap-1">
                {hasModule('invoice_capture') && <span className="module-badge module-badge-capture">OCR</span>}
                {hasModule('invoice_processing') && <span className="module-badge module-badge-processing">Processing</span>}
                {hasModule('vendor_management') && <span className="module-badge module-badge-vendor">Vendors</span>}
              </div>
            </div>
          )}

          {/* User */}
          <div className={`p-3 border-t border-border ${sidebarCollapsed ? 'flex justify-center' : ''}`}>
            {sidebarCollapsed ? (
              <button
                onClick={handleLogout}
                className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
                title="Log out"
              >
                <LogOut className="w-5 h-5" />
              </button>
            ) : (
              <div className="flex items-center gap-2">
                <div className="w-8 h-8 rounded-full bg-gradient-to-br from-primary/80 to-accent/80 flex items-center justify-center text-white text-sm font-medium">
                  {user?.name?.[0]?.toUpperCase() || user?.email?.[0]?.toUpperCase() || 'U'}
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground truncate">{user?.name || user?.email}</p>
                  <p className="text-xs text-muted-foreground truncate">{tenant?.settings?.company_name}</p>
                </div>
                <button
                  onClick={handleLogout}
                  className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
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
        <header className="h-14 bg-card/80 backdrop-blur-sm border-b border-border sticky top-0 z-20">
          <div className="h-full px-4 flex items-center justify-between gap-4">
            {/* Command Palette Trigger */}
            <div className="flex-1 max-w-md">
              <CommandPaletteTrigger />
            </div>

            {/* Right actions */}
            <div className="flex items-center gap-2">
              {/* Persona Switcher */}
              <div className="relative">
                <button
                  onClick={() => setShowPersonaSwitcher(!showPersonaSwitcher)}
                  className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm bg-secondary hover:bg-secondary/80 transition-colors"
                >
                  <CurrentPersonaIcon className="w-4 h-4 text-primary" />
                  <span className="text-foreground font-medium hidden sm:inline">
                    {currentPersona?.name || 'Full Platform'}
                  </span>
                  <ChevronDown className={`w-4 h-4 text-muted-foreground transition-transform ${showPersonaSwitcher ? 'rotate-180' : ''}`} />
                </button>

                {showPersonaSwitcher && (
                  <>
                    <div className="fixed inset-0 z-40" onClick={() => setShowPersonaSwitcher(false)} />
                    <div className="absolute right-0 top-full mt-2 w-72 bg-card border border-border rounded-xl shadow-soft-lg z-50 animate-scale-in overflow-hidden">
                      <div className="p-3 border-b border-border">
                        <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Product Configuration</p>
                      </div>
                      <div className="max-h-80 overflow-y-auto p-1">
                        {personas.map((persona) => {
                          const Icon = personaIcons[persona.id] || Layers;
                          const isSelected = persona.id === currentPersona?.id;

                          return (
                            <button
                              key={persona.id}
                              onClick={() => handleSwitchPersona(persona.id)}
                              disabled={switchingPersona || isSelected}
                              className={`w-full p-2 rounded-lg text-left flex items-start gap-3 transition-colors disabled:opacity-50 ${
                                isSelected ? 'bg-primary/10' : 'hover:bg-secondary'
                              }`}
                            >
                              <div className={`w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0 ${
                                isSelected ? 'bg-primary text-white' : 'bg-secondary text-muted-foreground'
                              }`}>
                                <Icon className="w-4 h-4" />
                              </div>
                              <div className="flex-1 min-w-0">
                                <p className={`text-sm font-medium ${isSelected ? 'text-primary' : 'text-foreground'}`}>
                                  {persona.name}
                                </p>
                                <p className="text-xs text-muted-foreground truncate">
                                  {persona.modules?.filter(m => m.enabled).length || 0} modules enabled
                                </p>
                              </div>
                              {isSelected && <Check className="w-4 h-4 text-primary mt-1" />}
                            </button>
                          );
                        })}
                      </div>
                      {switchingPersona && (
                        <div className="p-3 border-t border-border flex items-center justify-center text-muted-foreground">
                          <RefreshCw className="w-4 h-4 animate-spin mr-2" />
                          <span className="text-sm">Switching...</span>
                        </div>
                      )}
                    </div>
                  </>
                )}
              </div>

              {/* Notifications */}
              <button className="relative p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors">
                <Bell className="w-5 h-5" />
                <span className="absolute top-1.5 right-1.5 w-2 h-2 bg-error rounded-full" />
              </button>

              {/* Settings */}
              <Link
                href="/settings"
                className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
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
