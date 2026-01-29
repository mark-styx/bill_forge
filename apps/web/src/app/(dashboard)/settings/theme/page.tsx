'use client';

import { ThemeShowcase } from '@/components/ui/theme-showcase';
import { ThemeSettings } from '@/components/ui/theme-settings';
import { useAuthStore } from '@/stores/auth';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Palette, Building2, Layers } from 'lucide-react';

export default function ThemePage() {
  const { hasRole } = useAuthStore();
  const isAdmin = hasRole('tenant_admin');

  return (
    <div className="max-w-6xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-semibold text-foreground">Theme Settings</h1>
        <p className="text-muted-foreground mt-1">
          Customize the appearance of BillForge for yourself or your organization
        </p>
      </div>

      <Tabs defaultValue="personal" className="space-y-6">
        <TabsList className="bg-secondary/50">
          <TabsTrigger value="personal" className="flex items-center gap-2">
            <Palette className="w-4 h-4" />
            Personal Theme
          </TabsTrigger>
          {isAdmin && (
            <TabsTrigger value="organization" className="flex items-center gap-2">
              <Building2 className="w-4 h-4" />
              Organization Theme
            </TabsTrigger>
          )}
          <TabsTrigger value="showcase" className="flex items-center gap-2">
            <Layers className="w-4 h-4" />
            Theme Showcase
          </TabsTrigger>
        </TabsList>

        {/* Personal Theme Tab */}
        <TabsContent value="personal" className="space-y-0">
          <ThemeSettings mode="user" showAdvanced />
        </TabsContent>

        {/* Organization Theme Tab (Admin Only) */}
        {isAdmin && (
          <TabsContent value="organization" className="space-y-0">
            <div className="p-4 mb-6 bg-primary/5 border border-primary/20 rounded-xl">
              <h3 className="font-medium text-foreground">Organization Theme</h3>
              <p className="text-sm text-muted-foreground mt-1">
                Changes here will affect all users in your organization. You can allow users to override with their personal preferences.
              </p>
            </div>
            <ThemeSettings mode="organization" showAdvanced />
          </TabsContent>
        )}

        {/* Theme Showcase Tab */}
        <TabsContent value="showcase" className="space-y-0">
          <ThemeShowcase
            showPresetSelector
            showComponentDemo
            showColorPalette
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
