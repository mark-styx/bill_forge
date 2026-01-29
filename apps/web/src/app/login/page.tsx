'use client';

import { useState, useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useAuthStore } from '@/stores/auth';
import { useThemeStore, themePresets } from '@/stores/theme';
import { sandboxApi, PersonaInfo } from '@/lib/api';
import { toast } from 'sonner';
import {
  FileText,
  ScanLine,
  ClipboardCheck,
  Users,
  Layers,
  Check,
  ChevronDown,
  Sparkles,
  ArrowRight,
  Zap,
  Shield,
  BarChart3,
  Eye,
  EyeOff,
} from 'lucide-react';

const personaIcons: Record<string, typeof FileText> = {
  full_platform: Layers,
  invoice_ocr_only: ScanLine,
  invoice_processing_only: ClipboardCheck,
  vendor_management_only: Users,
  ap_lite: FileText,
};

const personaGradients: Record<string, string> = {
  full_platform: 'from-blue-500 via-primary to-cyan-400',
  invoice_ocr_only: 'from-sky-500 via-blue-500 to-blue-400',
  invoice_processing_only: 'from-emerald-500 via-teal-500 to-teal-400',
  vendor_management_only: 'from-violet-500 via-purple-500 to-purple-400',
  ap_lite: 'from-rose-500 via-pink-500 to-orange-400',
};

const features = [
  { icon: Zap, title: 'Lightning Fast OCR', desc: 'Process invoices in seconds' },
  { icon: Shield, title: 'Secure & Compliant', desc: 'SOC 2 & GDPR certified' },
  { icon: BarChart3, title: 'Real-time Analytics', desc: 'Track every dollar' },
];

export default function LoginPage() {
  const router = useRouter();
  const login = useAuthStore((state) => state.login);
  const switchPersona = useAuthStore((state) => state.switchPersona);
  const isLoading = useAuthStore((state) => state.isLoading);
  const { presetId, setPreset } = useThemeStore();

  const [personas, setPersonas] = useState<PersonaInfo[]>([]);
  const [selectedPersona, setSelectedPersona] = useState<string>('full_platform');
  const [showPersonaSelector, setShowPersonaSelector] = useState(false);
  const [loadingPersonas, setLoadingPersonas] = useState(true);

  const [showPassword, setShowPassword] = useState(false);
  const [formData, setFormData] = useState({
    tenantId: '11111111-1111-1111-1111-111111111111',
    email: 'admin@sandbox.local',
    password: 'sandbox123',
  });

  useEffect(() => {
    async function loadPersonas() {
      try {
        const data = await sandboxApi.listPersonas();
        setPersonas(data);
      } catch {
        setPersonas([
          {
            id: 'full_platform',
            name: 'Full Platform',
            description: 'Complete accounts payable solution with all features.',
            modules: [],
            roles: [],
            reporting_sections: [],
          },
        ]);
      } finally {
        setLoadingPersonas(false);
      }
    }
    loadPersonas();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await login(formData.tenantId, formData.email, formData.password);
      
      if (selectedPersona !== 'full_platform') {
        try {
          await switchPersona(selectedPersona);
        } catch {
          // Ignore errors
        }
      }
      
      toast.success('Welcome back!');
      router.push('/dashboard');
    } catch (error: any) {
      toast.error(error.message || 'Login failed');
    }
  };

  const currentPersona = personas.find((p) => p.id === selectedPersona);
  const PersonaIcon = personaIcons[selectedPersona] || Layers;
  const personaGradient = personaGradients[selectedPersona] || 'from-blue-500 to-cyan-400';

  return (
    <div className="min-h-screen flex bg-gradient-to-br from-background via-background to-primary/5">
      {/* Left side - Branding */}
      <div className="hidden lg:flex lg:w-1/2 xl:w-2/5 relative overflow-hidden">
        <div className={`absolute inset-0 bg-gradient-to-br ${personaGradient} transition-all duration-700`} />
        <div className="absolute inset-0 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjAiIGhlaWdodD0iNjAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGRlZnM+PHBhdHRlcm4gaWQ9ImdyaWQiIHdpZHRoPSI2MCIgaGVpZ2h0PSI2MCIgcGF0dGVyblVuaXRzPSJ1c2VyU3BhY2VPblVzZSI+PHBhdGggZD0iTSA2MCAwIEwgMCAwIDAgNjAiIGZpbGw9Im5vbmUiIHN0cm9rZT0icmdiYSgyNTUsMjU1LDI1NSwwLjA4KSIgc3Ryb2tlLXdpZHRoPSIxIi8+PC9wYXR0ZXJuPjwvZGVmcz48cmVjdCB3aWR0aD0iMTAwJSIgaGVpZ2h0PSIxMDAlIiBmaWxsPSJ1cmwoI2dyaWQpIi8+PC9zdmc+')] opacity-60" />

        {/* Floating orbs for visual interest */}
        <div className="absolute top-20 left-20 w-72 h-72 bg-white/10 rounded-full blur-3xl animate-pulse" />
        <div className="absolute bottom-32 right-10 w-96 h-96 bg-white/5 rounded-full blur-3xl animate-pulse" style={{ animationDelay: '1s' }} />

        <div className="relative z-10 flex flex-col justify-between p-12 text-white h-full">
          <div>
            <div className="flex items-center gap-3">
              <div className="w-12 h-12 rounded-xl bg-white/20 backdrop-blur-sm flex items-center justify-center shadow-lg">
                <FileText className="w-6 h-6" />
              </div>
              <span className="text-2xl font-bold tracking-tight">BillForge</span>
            </div>
          </div>

          <div className="space-y-8">
            <div className="space-y-4">
              <h1 className="text-5xl font-bold leading-tight tracking-tight">
                Streamline your<br />
                <span className="text-white/90">accounts payable</span>
              </h1>
              <p className="text-white/75 text-lg max-w-md leading-relaxed">
                Automate invoice processing, smart approvals, and vendor management with our modern AP platform.
              </p>
            </div>

            {/* Feature highlights */}
            <div className="space-y-3 pt-2">
              {features.map((feature, idx) => (
                <div key={idx} className="flex items-center gap-3 p-3 bg-white/10 backdrop-blur-sm rounded-xl border border-white/10">
                  <div className="w-10 h-10 rounded-lg bg-white/15 flex items-center justify-center">
                    <feature.icon className="w-5 h-5" />
                  </div>
                  <div>
                    <p className="font-semibold">{feature.title}</p>
                    <p className="text-sm text-white/70">{feature.desc}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-4 pt-8">
            <div className="flex -space-x-3">
              {['bg-blue-400', 'bg-emerald-400', 'bg-amber-400', 'bg-rose-400'].map((color, i) => (
                <div key={i} className={`w-10 h-10 rounded-full ${color} border-2 border-white/50 shadow-md flex items-center justify-center text-sm font-semibold`}>
                  {String.fromCharCode(65 + i)}
                </div>
              ))}
            </div>
            <p className="text-sm text-white/80">
              Trusted by <span className="font-bold text-white">500+</span> finance teams
            </p>
          </div>
        </div>
      </div>

      {/* Right side - Login form */}
      <div className="flex-1 flex items-center justify-center p-6 lg:p-12">
        <div className="w-full max-w-md">
          {/* Mobile logo */}
          <div className="lg:hidden text-center mb-8">
            <div className={`inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-gradient-to-br ${personaGradient} shadow-lg mb-4`}>
              <PersonaIcon className="w-7 h-7 text-white" />
            </div>
            <h1 className="text-2xl font-bold text-slate-900">BillForge</h1>
          </div>

          <div className="space-y-6">
            <div>
              <h2 className="text-2xl font-semibold text-slate-900">Welcome back</h2>
              <p className="text-slate-500 mt-1">Sign in to your account to continue</p>
            </div>

            <form onSubmit={handleSubmit} className="space-y-5">
              {/* Persona Selector */}
              <div>
                <label className="block text-sm font-medium text-slate-700 mb-1.5">
                  Product Configuration
                </label>
                <div className="relative">
                  <button
                    type="button"
                    onClick={() => setShowPersonaSelector(!showPersonaSelector)}
                    className="w-full px-4 py-3 bg-white border border-slate-200 rounded-xl text-left focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary transition-all flex items-center justify-between hover:border-slate-300"
                  >
                    <div className="flex items-center gap-3">
                      <div className={`w-9 h-9 rounded-lg bg-gradient-to-br ${personaGradient} flex items-center justify-center shadow-sm`}>
                        <PersonaIcon className="w-4.5 h-4.5 text-white" />
                      </div>
                      <div>
                        <p className="font-medium text-slate-900">{currentPersona?.name || 'Select'}</p>
                        <p className="text-xs text-slate-500">
                          {currentPersona?.modules?.filter(m => m.enabled).length || 0} modules
                        </p>
                      </div>
                    </div>
                    <ChevronDown className={`w-5 h-5 text-slate-400 transition-transform ${showPersonaSelector ? 'rotate-180' : ''}`} />
                  </button>

                  {showPersonaSelector && !loadingPersonas && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setShowPersonaSelector(false)} />
                      <div className="absolute top-full left-0 right-0 mt-2 bg-white border border-slate-200 rounded-xl shadow-xl z-50 overflow-hidden animate-scale-in">
                        <div className="p-2">
                          {personas.map((persona) => {
                            const Icon = personaIcons[persona.id] || Layers;
                            const gradient = personaGradients[persona.id] || 'from-blue-500 to-cyan-400';
                            const isSelected = persona.id === selectedPersona;

                            return (
                              <button
                                key={persona.id}
                                type="button"
                                onClick={() => {
                                  setSelectedPersona(persona.id);
                                  setShowPersonaSelector(false);
                                }}
                                className={`w-full p-3 rounded-lg text-left flex items-center gap-3 transition-colors ${
                                  isSelected ? 'bg-primary/5' : 'hover:bg-slate-50'
                                }`}
                              >
                                <div className={`w-9 h-9 rounded-lg bg-gradient-to-br ${gradient} flex items-center justify-center shadow-sm`}>
                                  <Icon className="w-4 h-4 text-white" />
                                </div>
                                <div className="flex-1 min-w-0">
                                  <p className={`font-medium ${isSelected ? 'text-primary' : 'text-slate-900'}`}>{persona.name}</p>
                                  <p className="text-xs text-slate-500 truncate">{persona.description}</p>
                                </div>
                                {isSelected && <Check className="w-5 h-5 text-primary" />}
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    </>
                  )}
                </div>
              </div>

              {/* Module Preview */}
              {currentPersona && currentPersona.modules && currentPersona.modules.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {currentPersona.modules.map((module) => (
                    <span
                      key={module.id}
                      className={`px-2 py-0.5 text-xs rounded-full font-medium ${
                        module.enabled
                          ? 'bg-emerald-50 text-emerald-700 border border-emerald-200'
                          : 'bg-slate-100 text-slate-400 border border-slate-200 line-through'
                      }`}
                    >
                      {module.name.replace(' (OCR)', '')}
                    </span>
                  ))}
                </div>
              )}

              <div>
                <label className="block text-sm font-medium text-slate-700 mb-1.5">
                  Tenant ID
                </label>
                <input
                  type="text"
                  value={formData.tenantId}
                  onChange={(e) => setFormData({ ...formData, tenantId: e.target.value })}
                  className="w-full px-4 py-3 bg-white border border-slate-200 rounded-xl text-slate-900 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary transition-all"
                  placeholder="Enter your tenant ID"
                  required
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-slate-700 mb-1.5">
                  Email
                </label>
                <input
                  type="email"
                  value={formData.email}
                  onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                  className="w-full px-4 py-3 bg-white border border-slate-200 rounded-xl text-slate-900 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary transition-all"
                  placeholder="name@company.com"
                  required
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-slate-700 mb-1.5">
                  Password
                </label>
                <div className="relative">
                  <input
                    type={showPassword ? 'text' : 'password'}
                    value={formData.password}
                    onChange={(e) => setFormData({ ...formData, password: e.target.value })}
                    className="w-full px-4 py-3 pr-12 bg-white border border-slate-200 rounded-xl text-slate-900 placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary transition-all"
                    placeholder="••••••••"
                    required
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword(!showPassword)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 p-1 text-slate-400 hover:text-slate-600 transition-colors"
                  >
                    {showPassword ? <EyeOff className="w-5 h-5" /> : <Eye className="w-5 h-5" />}
                  </button>
                </div>
              </div>

              <button
                type="submit"
                disabled={isLoading}
                className={`group w-full py-3.5 px-4 bg-gradient-to-r ${personaGradient} text-white font-semibold rounded-xl shadow-lg shadow-primary/25 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 hover:shadow-xl hover:shadow-primary/30 active:scale-[0.98]`}
              >
                {isLoading ? (
                  <span className="flex items-center justify-center gap-2">
                    <svg className="animate-spin h-5 w-5" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                    </svg>
                    Signing in...
                  </span>
                ) : (
                  <span className="flex items-center justify-center gap-2">
                    Sign in
                    <ArrowRight className="w-4 h-4 transition-transform group-hover:translate-x-1" />
                  </span>
                )}
              </button>
            </form>

            {/* Theme Switcher */}
            <div className="pt-4 border-t border-slate-100">
              <p className="text-xs font-medium text-slate-500 mb-2">Color Theme</p>
              <div className="flex gap-2">
                {themePresets.slice(0, 6).map((preset) => (
                  <button
                    key={preset.id}
                    onClick={() => setPreset(preset.id)}
                    className={`w-7 h-7 rounded-full bg-gradient-to-br ${preset.preview} transition-transform hover:scale-110 ${
                      presetId === preset.id ? 'ring-2 ring-offset-2 ring-slate-400' : ''
                    }`}
                    title={preset.name}
                  />
                ))}
              </div>
            </div>

            {/* Sandbox Notice */}
            <div className="p-4 bg-blue-50 border border-blue-100 rounded-xl">
              <div className="flex gap-3">
                <Sparkles className="w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm font-medium text-blue-900">Sandbox Mode</p>
                  <p className="text-sm text-blue-700 mt-0.5">
                    Select a product configuration to test different subscription scenarios.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
