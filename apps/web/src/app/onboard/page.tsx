'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { FileText, Building2, ScanLine, Link2, Rocket } from 'lucide-react';
import { useAuthStore, setupApiCallbacks } from '@/stores/auth';
import { authApi, api } from '@/lib/api';
import { StepperWithContent, StepContent, Step } from '@/components/ui/stepper';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

const STEPS: Step[] = [
  { id: 'company', title: 'Company Info', icon: <Building2 className="w-4 h-4" /> },
  { id: 'admin', title: 'Admin Account', icon: <FileText className="w-4 h-4" /> },
  { id: 'ocr', title: 'OCR Provider', icon: <ScanLine className="w-4 h-4" />, optional: true },
  { id: 'erp', title: 'ERP Connection', icon: <Link2 className="w-4 h-4" />, optional: true },
  { id: 'launch', title: 'Launch', icon: <Rocket className="w-4 h-4" /> },
];

const OCR_PROVIDERS = [
  { id: 'builtin', name: 'BillForge OCR', description: 'Built-in OCR powered by Tesseract. Free, runs locally.' },
  { id: 'textract', name: 'AWS Textract', description: 'High-accuracy OCR from AWS. Requires AWS credentials.' },
  { id: 'google', name: 'Google Document AI', description: 'Google Cloud document processing. Requires GCP project.' },
];

const ERP_OPTIONS = [
  { id: 'quickbooks', name: 'QuickBooks Online', description: 'Sync invoices and payments with QuickBooks.' },
  { id: 'xero', name: 'Xero', description: 'Export approved invoices to Xero.' },
  { id: 'sage', name: 'Sage Intacct', description: 'Enterprise ERP integration with Sage.' },
  { id: 'none', name: 'Skip for now', description: 'You can connect an ERP later from Settings.' },
];

export default function OnboardPage() {
  const router = useRouter();
  const login = useAuthStore((state) => state.login);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [companyName, setCompanyName] = useState('');
  const [timezone, setTimezone] = useState('UTC');
  const [currency, setCurrency] = useState('USD');
  const [adminName, setAdminName] = useState('');
  const [adminEmail, setAdminEmail] = useState('');
  const [adminPassword, setAdminPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [selectedOcr, setSelectedOcr] = useState('builtin');
  const [selectedErp, setSelectedErp] = useState('none');

  const handleStepComplete = async (stepIndex: number): Promise<boolean> => {
    setError(null);

    if (stepIndex === 0) {
      if (!companyName.trim()) {
        setError('Company name is required');
        return false;
      }
      return true;
    }

    if (stepIndex === 1) {
      if (!adminName.trim() || !adminEmail.trim() || !adminPassword) {
        setError('All admin fields are required');
        return false;
      }
      if (adminPassword.length < 8) {
        setError('Password must be at least 8 characters');
        return false;
      }
      if (adminPassword !== confirmPassword) {
        setError('Passwords do not match');
        return false;
      }
      return true;
    }

    // OCR and ERP steps always pass
    if (stepIndex === 2 || stepIndex === 3) {
      return true;
    }

    return true;
  };

  const handleComplete = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await authApi.provision({
        company_name: companyName,
        admin_email: adminEmail,
        admin_password: adminPassword,
        admin_name: adminName,
        timezone,
        default_currency: currency,
      });

      // Set full auth state (tokens, user, tenant, isAuthenticated)
      api.setToken(response.access_token);
      api.setRefreshToken(response.refresh_token);
      setupApiCallbacks();

      useAuthStore.setState({
        user: response.user,
        accessToken: response.access_token,
        refreshToken: response.refresh_token,
        isAuthenticated: true,
        tenant: {
          id: response.tenant.id,
          name: response.tenant.name,
          enabled_modules: response.tenant.enabled_modules,
          settings: response.tenant.settings,
        },
      });

      router.push('/dashboard');
    } catch (err: any) {
      setError(err?.message || 'Failed to create account. Please try again.');
      setIsLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
      <div className="w-full max-w-2xl bg-card border border-border rounded-2xl shadow-2xl p-8">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-blue-500/20 mb-3">
            <FileText className="w-6 h-6 text-blue-400" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">Set up BillForge</h1>
          <p className="text-muted-foreground mt-1">Get your AP automation running in minutes</p>
        </div>

        {error && (
          <div className="mb-6 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}

        <StepperWithContent
          steps={STEPS}
          variant="numbered"
          onStepComplete={handleStepComplete}
          onComplete={handleComplete}
          isLoading={isLoading}
          completeLabel="Launch BillForge"
        >
          {/* Step 1: Company Info */}
          <StepContent>
            <div className="space-y-4">
              <div>
                <Label htmlFor="company">Company Name</Label>
                <Input
                  id="company"
                  placeholder="Acme Corporation"
                  value={companyName}
                  onChange={(e) => setCompanyName(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="timezone">Timezone</Label>
                  <select
                    id="timezone"
                    value={timezone}
                    onChange={(e) => setTimezone(e.target.value)}
                    className="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  >
                    <option value="UTC">UTC</option>
                    <option value="America/New_York">Eastern (ET)</option>
                    <option value="America/Chicago">Central (CT)</option>
                    <option value="America/Denver">Mountain (MT)</option>
                    <option value="America/Los_Angeles">Pacific (PT)</option>
                    <option value="Europe/London">London (GMT)</option>
                    <option value="Europe/Berlin">Berlin (CET)</option>
                    <option value="Asia/Tokyo">Tokyo (JST)</option>
                  </select>
                </div>
                <div>
                  <Label htmlFor="currency">Default Currency</Label>
                  <select
                    id="currency"
                    value={currency}
                    onChange={(e) => setCurrency(e.target.value)}
                    className="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  >
                    <option value="USD">USD - US Dollar</option>
                    <option value="EUR">EUR - Euro</option>
                    <option value="GBP">GBP - British Pound</option>
                    <option value="CAD">CAD - Canadian Dollar</option>
                    <option value="AUD">AUD - Australian Dollar</option>
                    <option value="JPY">JPY - Japanese Yen</option>
                  </select>
                </div>
              </div>
            </div>
          </StepContent>

          {/* Step 2: Admin Account */}
          <StepContent>
            <div className="space-y-4">
              <div>
                <Label htmlFor="adminName">Full Name</Label>
                <Input
                  id="adminName"
                  placeholder="Jane Smith"
                  value={adminName}
                  onChange={(e) => setAdminName(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div>
                <Label htmlFor="adminEmail">Email</Label>
                <Input
                  id="adminEmail"
                  type="email"
                  placeholder="jane@acme.com"
                  value={adminEmail}
                  onChange={(e) => setAdminEmail(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="password">Password</Label>
                  <Input
                    id="password"
                    type="password"
                    placeholder="Min 8 characters"
                    value={adminPassword}
                    onChange={(e) => setAdminPassword(e.target.value)}
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="confirmPassword">Confirm Password</Label>
                  <Input
                    id="confirmPassword"
                    type="password"
                    placeholder="Confirm password"
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    className="mt-1"
                  />
                </div>
              </div>
            </div>
          </StepContent>

          {/* Step 3: OCR Provider */}
          <StepContent>
            <p className="text-sm text-muted-foreground mb-4">
              Choose how BillForge extracts data from your invoices. You can change this later.
            </p>
            <div className="space-y-3">
              {OCR_PROVIDERS.map((provider) => (
                <button
                  key={provider.id}
                  onClick={() => setSelectedOcr(provider.id)}
                  className={`w-full text-left p-4 rounded-lg border-2 transition-all ${
                    selectedOcr === provider.id
                      ? 'border-primary bg-primary/5'
                      : 'border-border hover:border-primary/30'
                  }`}
                >
                  <div className="font-medium text-foreground">{provider.name}</div>
                  <div className="text-sm text-muted-foreground mt-1">{provider.description}</div>
                </button>
              ))}
            </div>
          </StepContent>

          {/* Step 4: ERP Connection */}
          <StepContent>
            <p className="text-sm text-muted-foreground mb-4">
              Connect your accounting system to sync approved invoices automatically.
            </p>
            <div className="space-y-3">
              {ERP_OPTIONS.map((erp) => (
                <button
                  key={erp.id}
                  onClick={() => setSelectedErp(erp.id)}
                  className={`w-full text-left p-4 rounded-lg border-2 transition-all ${
                    selectedErp === erp.id
                      ? 'border-primary bg-primary/5'
                      : 'border-border hover:border-primary/30'
                  }`}
                >
                  <div className="font-medium text-foreground">{erp.name}</div>
                  <div className="text-sm text-muted-foreground mt-1">{erp.description}</div>
                </button>
              ))}
            </div>
          </StepContent>

          {/* Step 5: Launch */}
          <StepContent>
            <div className="text-center py-6">
              <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-green-500/20 mb-4">
                <Rocket className="w-8 h-8 text-green-400" />
              </div>
              <h3 className="text-xl font-bold text-foreground mb-2">Ready to launch!</h3>
              <p className="text-muted-foreground mb-6">
                Your BillForge workspace for <strong>{companyName || 'your company'}</strong> is ready.
              </p>
              <div className="text-left bg-muted/30 rounded-lg p-4 space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Company</span>
                  <span className="font-medium">{companyName}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Admin</span>
                  <span className="font-medium">{adminEmail}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">OCR</span>
                  <span className="font-medium">{OCR_PROVIDERS.find(p => p.id === selectedOcr)?.name}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">ERP</span>
                  <span className="font-medium">{ERP_OPTIONS.find(e => e.id === selectedErp)?.name}</span>
                </div>
              </div>
            </div>
          </StepContent>
        </StepperWithContent>

        <div className="mt-6 text-center">
          <button
            onClick={() => router.push('/login')}
            className="text-sm text-muted-foreground hover:text-foreground transition-colors"
          >
            Already have an account? Sign in
          </button>
        </div>
      </div>
    </div>
  );
}
