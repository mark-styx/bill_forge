'use client';

import { WeeklyLearningPanel } from '@/components/learning/WeeklyLearningPanel';
import { useAuthStore } from '@/stores/auth';

export default function LearningPage() {
  const { hasModule } = useAuthStore();

  if (!hasModule('invoice_processing')) {
    return (
      <div className="max-w-5xl mx-auto p-6 text-sm text-muted-foreground">
        Continuous learning insights are part of the Invoice Processing
        module. Upgrade to enable it for your tenant.
      </div>
    );
  }

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-semibold text-foreground">
          Learning Insights
        </h1>
        <p className="text-muted-foreground mt-0.5">
          Per-tenant continuous learning surface. Every correction your team
          applies (GL recode, approver reroute, autopilot override, duplicate
          dismissal) feeds the weekly model update below.
        </p>
      </div>
      <WeeklyLearningPanel />
    </div>
  );
}
