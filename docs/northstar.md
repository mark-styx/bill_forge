# BillForge North Star

## Mission

Eliminate the manual grind of accounts payable for mid-market companies. Finance teams processing 300-5,000 invoices/month deserve modern tooling that is fast, modular, and transparent - not bloated enterprise software or underpowered SMB tools.

## Vision

BillForge becomes the default AP automation platform for growing companies - the tool finance teams actually want to use. Invoices flow from capture to payment with minimal human intervention. Approvals happen from email inboxes, not clunky portals. Teams get real visibility into spend, bottlenecks, and cash flow.

## Core Thesis

**Speed wins deals. Simplicity retains customers. Transparent pricing builds trust.**

The mid-market AP space is stuck between two bad options:
- **Enterprise platforms** (Coupa, SAP Concur) - overbuilt, overpriced, months to implement
- **SMB tools** (BILL) - lack workflow sophistication, poor OCR, limited multi-tenant support

BillForge occupies the gap: enterprise-grade automation with SMB-grade simplicity.

## Target Customer

Mid-market companies, 50-500 employees, processing 300-5,000 invoices/month.

**Primary ICP:** AP managers at companies using QuickBooks + spreadsheets, processing 300-800 invoices/month, budget up to $1,500/month without CFO approval.

**Why they switch:** Manual data entry consumes 8+ hours/week. Approval cycles take 5-7 business days. Zero visibility into cash flow obligations.

## Five Pillars

| Pillar | Promise |
|--------|---------|
| **Speed** | Set up in an afternoon, not a quarter. Sub-second UI, 2-week implementation. |
| **Automation** | 90%+ OCR accuracy. Exception-only review. AI does the grunt work. |
| **Transparency** | Real-time status on every invoice. Complete audit trail. Published pricing. |
| **Modularity** | Independent modules (Capture, Processing, Vendors, Reporting). Pay for what you use. |
| **Privacy** | Local OCR option. Strict tenant isolation. Your data stays yours. |

## Product Scope

### What BillForge does
- **Invoice Capture** - Multi-provider OCR (Tesseract, Textract, Vision), confidence scoring, bulk upload
- **Invoice Processing** - Work queues, assignment rules, multi-level approval chains, delegations, workflow templates
- **Vendor Management** - Full lifecycle, tax documents, communication log, vendor-specific routing
- **Reporting & Analytics** - Real-time KPIs, aging analysis, spend summaries, predictive analytics
- **Integrations** - QuickBooks Online, Xero (OAuth 2.0), email approvals, Slack/Teams notifications, mobile push

### What BillForge does not do
- Payment execution (partner ecosystem play)
- Government/public sector procurement
- 3-way PO matching (deferred to Phase 3)
- Multi-currency GL (beyond MVP scope)
- Companies processing fewer than 150 or more than 10,000 invoices/month

## Technical Principles

- **Rust backend** - compile-time safety, sub-200ms API responses, memory efficiency
- **Multi-tenant by default** - every query, storage op, and audit log scoped to tenant. Cross-tenant access is architecturally impossible.
- **Modular monorepo** - each module (capture, processing, vendors, reporting) is an independent crate/package that can be enabled per org
- **Offline-first mobile** - delta sync protocol, push notifications, mobile approval workflows

## Success Metrics

- 5 pilot customers processing 3,500+ invoices by end of Q1 2026
- 90%+ OCR accuracy on first pass
- Sub-5-minute median invoice processing time (capture to approval queue)
- 80% reduction in manual data entry hours for pilot customers
- Net Promoter Score > 50

## Category

**Intelligent AP** - not just digitizing manual processes, but building native intelligence that learns and adapts. Self-optimizing rules, proactive insights, AI-assisted workflows.

## Tagline

*Simplified invoice processing for the mid-market.*
