import { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'BillForge - Modern Accounts Payable Automation',
  description:
    'Streamline your invoice processing with AI-powered capture, automated workflows, and real-time analytics. Process invoices 10x faster with fewer errors.',
  openGraph: {
    title: 'BillForge - Modern Accounts Payable Automation',
    description:
      'Streamline your invoice processing with AI-powered capture, automated workflows, and real-time analytics.',
    type: 'website',
    images: ['/og-image.png'],
  },
  twitter: {
    card: 'summary_large_image',
    title: 'BillForge - Modern Accounts Payable Automation',
    description:
      'Streamline your invoice processing with AI-powered capture, automated workflows, and real-time analytics.',
  },
};

export default function MarketingLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <>{children}</>;
}
