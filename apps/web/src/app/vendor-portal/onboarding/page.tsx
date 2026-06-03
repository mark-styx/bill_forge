'use client';

import { useState, useEffect } from 'react';
import { useSearchParams } from 'next/navigation';
import { FileText, CheckCircle } from 'lucide-react';
import { vendorPortalApi } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface RemitContact {
  name: string;
  email: string;
  phone: string;
}

export default function VendorOnboardingPage() {
  const searchParams = useSearchParams();
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);

  // Legal info
  const [legalName, setLegalName] = useState('');
  const [dba, setDba] = useState('');

  // Address
  const [line1, setLine1] = useState('');
  const [line2, setLine2] = useState('');
  const [city, setCity] = useState('');
  const [stateVal, setStateVal] = useState('');
  const [postalCode, setPostalCode] = useState('');
  const [country, setCountry] = useState('US');

  // Tax form
  const [taxFormType, setTaxFormType] = useState<'w9' | 'w8ben'>('w9');
  const [taxFile, setTaxFile] = useState<File | null>(null);

  // Banking
  const [bankName, setBankName] = useState('');
  const [accountType, setAccountType] = useState('checking');
  const [accountNumber, setAccountNumber] = useState('');
  const [routingNumber, setRoutingNumber] = useState('');

  // Remit-to contacts
  const [remitContacts, setRemitContacts] = useState<RemitContact[]>([
    { name: '', email: '', phone: '' },
  ]);

  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const queryToken = searchParams.get('token');
    if (queryToken) {
      localStorage.setItem('vendor_portal_token', queryToken);
      setToken(queryToken);
    } else {
      const stored = localStorage.getItem('vendor_portal_token');
      if (stored) {
        setToken(stored);
      } else {
        setError('No access token provided. Please use the link sent by the AP team.');
      }
    }
  }, [searchParams]);

  const addRemitContact = () => {
    setRemitContacts([...remitContacts, { name: '', email: '', phone: '' }]);
  };

  const removeRemitContact = (index: number) => {
    if (remitContacts.length > 1) {
      setRemitContacts(remitContacts.filter((_, i) => i !== index));
    }
  };

  const updateRemitContact = (index: number, field: keyof RemitContact, value: string) => {
    const updated = [...remitContacts];
    updated[index] = { ...updated[index], [field]: value };
    setRemitContacts(updated);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token) return;

    if (!legalName.trim()) {
      setError('Legal name is required');
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const address =
        line1 || city || postalCode
          ? { line1, line2: line2 || undefined, city, state: stateVal || undefined, postal_code: postalCode, country }
          : undefined;

      const banking =
        bankName || accountNumber || routingNumber
          ? { bank_name: bankName, account_type: accountType, account_number: accountNumber, routing_number: routingNumber }
          : undefined;

      const contacts = remitContacts
        .filter((c) => c.name.trim() || c.email.trim())
        .map((c) => ({ name: c.name, email: c.email, phone: c.phone }));

      await vendorPortalApi.submitOnboarding(
        token,
        {
          legal_name: legalName,
          dba: dba || undefined,
          address,
          tax_form_type: taxFormType,
          banking,
          remit_contacts: contacts.length > 0 ? contacts : undefined,
        },
        taxFile || undefined,
      );

      setSubmitted(true);
    } catch (err: any) {
      setError(err?.message || 'Failed to submit onboarding form');
    } finally {
      setSubmitting(false);
    }
  };

  if (!token) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <div className="w-full max-w-md bg-card border border-border rounded-2xl shadow-2xl p-8 text-center">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-red-500/20 mb-3">
            <FileText className="w-6 h-6 text-red-400" />
          </div>
          <h1 className="text-xl font-bold text-foreground mb-2">Access Required</h1>
          <p className="text-muted-foreground text-sm">
            {error || 'No access token found. Please use the link provided by the AP team.'}
          </p>
        </div>
      </div>
    );
  }

  if (submitted) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <div className="w-full max-w-md bg-card border border-border rounded-2xl shadow-2xl p-8 text-center">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-green-500/20 mb-3">
            <CheckCircle className="w-6 h-6 text-green-400" />
          </div>
          <h1 className="text-xl font-bold text-foreground mb-2">Submission Received</h1>
          <p className="text-muted-foreground text-sm">
            Your onboarding information has been submitted and is now in review. The AP team will
            follow up if any additional information is needed.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4 md:p-8">
      <div className="max-w-2xl mx-auto">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-blue-500/20 mb-3">
            <FileText className="w-6 h-6 text-blue-400" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">Vendor Onboarding</h1>
          <p className="text-muted-foreground mt-1">
            Complete your profile to get set up for payments
          </p>
        </div>

        {error && (
          <div className="mb-6 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-6">
          {/* Legal Information */}
          <div className="bg-card border border-border rounded-xl shadow-lg p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Legal Information</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <Label htmlFor="legalName">Legal Name *</Label>
                <Input
                  id="legalName"
                  placeholder="Acme Corporation"
                  value={legalName}
                  onChange={(e) => setLegalName(e.target.value)}
                  required
                  className="mt-1"
                />
              </div>
              <div>
                <Label htmlFor="dba">DBA (optional)</Label>
                <Input
                  id="dba"
                  placeholder="Acme Corp"
                  value={dba}
                  onChange={(e) => setDba(e.target.value)}
                  className="mt-1"
                />
              </div>
            </div>
          </div>

          {/* Address */}
          <div className="bg-card border border-border rounded-xl shadow-lg p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Address</h2>
            <div className="space-y-4">
              <div>
                <Label htmlFor="line1">Street Address</Label>
                <Input
                  id="line1"
                  placeholder="123 Main St"
                  value={line1}
                  onChange={(e) => setLine1(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div>
                <Label htmlFor="line2">Suite / Unit (optional)</Label>
                <Input
                  id="line2"
                  placeholder="Suite 100"
                  value={line2}
                  onChange={(e) => setLine2(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div>
                  <Label htmlFor="city">City</Label>
                  <Input
                    id="city"
                    placeholder="City"
                    value={city}
                    onChange={(e) => setCity(e.target.value)}
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="state">State</Label>
                  <Input
                    id="state"
                    placeholder="CA"
                    value={stateVal}
                    onChange={(e) => setStateVal(e.target.value)}
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="postalCode">Postal Code</Label>
                  <Input
                    id="postalCode"
                    placeholder="90210"
                    value={postalCode}
                    onChange={(e) => setPostalCode(e.target.value)}
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="country">Country</Label>
                  <Input
                    id="country"
                    placeholder="US"
                    value={country}
                    onChange={(e) => setCountry(e.target.value)}
                    className="mt-1"
                  />
                </div>
              </div>
            </div>
          </div>

          {/* Tax Form */}
          <div className="bg-card border border-border rounded-xl shadow-lg p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Tax Form</h2>
            <div className="space-y-4">
              <div>
                <Label>Form Type *</Label>
                <div className="flex gap-4 mt-2">
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="taxFormType"
                      value="w9"
                      checked={taxFormType === 'w9'}
                      onChange={() => setTaxFormType('w9')}
                      className="accent-primary"
                    />
                    <span className="text-sm text-foreground">W-9 (US)</span>
                  </label>
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="taxFormType"
                      value="w8ben"
                      checked={taxFormType === 'w8ben'}
                      onChange={() => setTaxFormType('w8ben')}
                      className="accent-primary"
                    />
                    <span className="text-sm text-foreground">W-8BEN (International)</span>
                  </label>
                </div>
              </div>
              <div>
                <Label htmlFor="taxDocument">Upload Tax Form (optional)</Label>
                <input
                  id="taxDocument"
                  type="file"
                  accept=".pdf,image/*"
                  onChange={(e) => setTaxFile(e.target.files?.[0] ?? null)}
                  className="mt-1 block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary/10 file:text-primary hover:file:bg-primary/20"
                />
                {taxFile && (
                  <p className="mt-1 text-xs text-muted-foreground">
                    {taxFile.name} ({(taxFile.size / 1024 / 1024).toFixed(2)} MB)
                  </p>
                )}
              </div>
            </div>
          </div>

          {/* Banking */}
          <div className="bg-card border border-border rounded-xl shadow-lg p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Banking Details</h2>
            <div className="space-y-4">
              <div>
                <Label htmlFor="bankName">Bank Name</Label>
                <Input
                  id="bankName"
                  placeholder="First National Bank"
                  value={bankName}
                  onChange={(e) => setBankName(e.target.value)}
                  className="mt-1"
                />
              </div>
              <div>
                <Label htmlFor="accountType">Account Type</Label>
                <select
                  id="accountType"
                  value={accountType}
                  onChange={(e) => setAccountType(e.target.value)}
                  className="mt-1 block w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                >
                  <option value="checking">Checking</option>
                  <option value="savings">Savings</option>
                </select>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="accountNumber">Account Number</Label>
                  <Input
                    id="accountNumber"
                    placeholder="Account number"
                    value={accountNumber}
                    onChange={(e) => setAccountNumber(e.target.value)}
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="routingNumber">Routing Number</Label>
                  <Input
                    id="routingNumber"
                    placeholder="Routing number"
                    value={routingNumber}
                    onChange={(e) => setRoutingNumber(e.target.value)}
                    className="mt-1"
                  />
                </div>
              </div>
            </div>
          </div>

          {/* Remit-to Contacts */}
          <div className="bg-card border border-border rounded-xl shadow-lg p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Remit-to Contacts</h2>
            <div className="space-y-4">
              {remitContacts.map((contact, index) => (
                <div key={index} className="flex gap-3 items-end">
                  <div className="flex-1">
                    <Label htmlFor={`contact-name-${index}`}>Name</Label>
                    <Input
                      id={`contact-name-${index}`}
                      placeholder="Contact name"
                      value={contact.name}
                      onChange={(e) => updateRemitContact(index, 'name', e.target.value)}
                      className="mt-1"
                    />
                  </div>
                  <div className="flex-1">
                    <Label htmlFor={`contact-email-${index}`}>Email</Label>
                    <Input
                      id={`contact-email-${index}`}
                      type="email"
                      placeholder="email@example.com"
                      value={contact.email}
                      onChange={(e) => updateRemitContact(index, 'email', e.target.value)}
                      className="mt-1"
                    />
                  </div>
                  <div className="flex-1">
                    <Label htmlFor={`contact-phone-${index}`}>Phone</Label>
                    <Input
                      id={`contact-phone-${index}`}
                      placeholder="555-123-4567"
                      value={contact.phone}
                      onChange={(e) => updateRemitContact(index, 'phone', e.target.value)}
                      className="mt-1"
                    />
                  </div>
                  {remitContacts.length > 1 && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => removeRemitContact(index)}
                      className="mb-0.5"
                    >
                      Remove
                    </Button>
                  )}
                </div>
              ))}
              <Button type="button" variant="outline" onClick={addRemitContact}>
                Add Contact
              </Button>
            </div>
          </div>

          <Button type="submit" disabled={submitting} className="w-full">
            {submitting ? 'Submitting...' : 'Submit Onboarding Information'}
          </Button>
        </form>
      </div>
    </div>
  );
}
