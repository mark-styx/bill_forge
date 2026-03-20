'use client';

import Link from 'next/link';
import { useThemeStore } from '@/stores/theme';
import { GradientButton } from '@/components/ui/gradient-card';
import { Button } from '@/components/ui/button';
import { FileText, Menu, X } from 'lucide-react';
import { useState } from 'react';

function MarketingHeader() {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();
  const [mobileOpen, setMobileOpen] = useState(false);

  const navLinks = [
    { label: 'Features', href: '#features' },
    { label: 'How It Works', href: '#how-it-works' },
    { label: 'Pricing', href: '#pricing' },
    { label: 'Demo', href: '#demo' },
  ];

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="container mx-auto flex h-16 items-center justify-between px-4">
        {/* Logo */}
        <Link href="/" className="flex items-center gap-2 flex-shrink-0">
          <div
            className="flex h-9 w-9 items-center justify-center rounded-xl"
            style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
          >
            <FileText className="h-5 w-5 text-white" />
          </div>
          <span className="text-xl font-bold text-foreground">BillForge</span>
        </Link>

        {/* Desktop Nav */}
        <nav className="hidden items-center gap-8 md:flex">
          {navLinks.map((link) => (
            <Link
              key={link.label}
              href={link.href}
              className="text-sm text-muted-foreground hover:text-foreground transition-colors font-medium"
            >
              {link.label}
            </Link>
          ))}
        </nav>

        {/* Desktop Actions */}
        <div className="hidden md:flex items-center gap-3">
          <Link
            href="/login"
            className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
          >
            Sign In
          </Link>
          <Link href="/login">
            <GradientButton gradient="primary" size="sm">
              Request Demo
            </GradientButton>
          </Link>
        </div>

        {/* Mobile Menu Toggle */}
        <button
          className="md:hidden p-2 text-muted-foreground hover:text-foreground transition-colors"
          onClick={() => setMobileOpen((prev) => !prev)}
          aria-label={mobileOpen ? 'Close menu' : 'Open menu'}
        >
          {mobileOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
        </button>
      </div>

      {/* Mobile Menu */}
      {mobileOpen && (
        <div className="md:hidden border-t border-border bg-background/95 backdrop-blur px-4 py-4 flex flex-col gap-4">
          {navLinks.map((link) => (
            <Link
              key={link.label}
              href={link.href}
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors py-1"
              onClick={() => setMobileOpen(false)}
            >
              {link.label}
            </Link>
          ))}
          <div className="flex flex-col gap-2 pt-2 border-t border-border">
            <Link href="/login" onClick={() => setMobileOpen(false)}>
              <Button variant="outline" className="w-full">
                Sign In
              </Button>
            </Link>
            <Link href="/login" onClick={() => setMobileOpen(false)}>
              <GradientButton gradient="primary" className="w-full">
                Request Demo
              </GradientButton>
            </Link>
          </div>
        </div>
      )}
    </header>
  );
}

export default function MarketingLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <>
      <MarketingHeader />
      {children}
    </>
  );
}
