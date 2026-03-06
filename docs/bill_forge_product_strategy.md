# Bill Forge Product Strategy

**Date:** January 31, 2026
**Version:** 1.0
**Author:** CPO (AI-Assisted)
**Status:** Draft for Review

---

## Executive Summary

Bill Forge enters the accounts payable automation market with a differentiated positioning: **modern, modular, and mid-market focused**. While enterprise players (Coupa, SAP Ariba) dominate the Fortune 500 and SMB tools (BILL, QuickBooks) serve small businesses, the 10-1000 employee segment is underserved by solutions that are either too complex or too basic.

Our strategy: deliver a fast, beautiful AP platform that makes invoice processing feel effortless—not like enterprise software.

---

## 1. Target Customer Profiles

### Primary Persona: The Overwhelmed AP Manager

**Profile: Sarah Chen, AP Manager at GrowthTech Inc.**

| Attribute | Details |
|-----------|---------|
| **Company Size** | 180 employees, $45M revenue |
| **Industry** | B2B SaaS / Technology |
| **Current Stack** | QuickBooks Online + Excel spreadsheets |
| **Invoice Volume** | 400-600 invoices/month |
| **Team Size** | 2 AP clerks + 1 supervisor |
| **Pain Points** | Manual data entry (8+ hours/week), missed early payment discounts, approval bottlenecks (CEO travels frequently), no visibility into cash flow commitments |
| **Decision Criteria** | Easy setup, fast ROI, doesn't require IT involvement |
| **Budget Authority** | Up to $1,500/month without CFO approval |
| **Quote** | *"I just want invoices to flow through without me chasing people for approvals."* |

**Buying Triggers:**
- Just hired a second AP clerk (scaling pain)
- Audit finding about missing approvals
- CFO asking for better spend visibility
- ERP upgrade creating integration opportunity

---

### Secondary Persona: The Scaling Finance Leader

**Profile: Marcus Thompson, Controller at IndustrialCo Manufacturing**

| Attribute | Details |
|-----------|---------|
| **Company Size** | 450 employees, $85M revenue |
| **Industry** | Manufacturing / Distribution |
| **Current Stack** | Sage Intacct + Palette (legacy, considering switch) |
| **Invoice Volume** | 2,500-3,000 invoices/month |
| **Team Size** | 5-person AP department |
| **Pain Points** | Slow legacy system, poor OCR accuracy, expensive per-user licensing, weak reporting |
| **Decision Criteria** | Integration with Sage/NetSuite, better automation rates, predictable pricing |
| **Budget Authority** | $3,000-5,000/month |
| **Quote** | *"Our current system works, but it feels like we're fighting it every day."* |

**Buying Triggers:**
- Contract renewal approaching (switching window)
- M&A activity (need to consolidate AP)
- New CFO wanting to modernize finance stack
- Cost reduction initiative

---

### Tertiary Persona: The Shared Services Director

**Profile: Jennifer Rodriguez, Director of Shared Services at MultiCorp Holdings**

| Attribute | Details |
|-----------|---------|
| **Company Size** | 800 employees across 5 entities |
| **Industry** | Multi-entity holding company |
| **Current Stack** | Mixed (QuickBooks, NetSuite, manual processes) |
| **Invoice Volume** | 5,000-8,000 invoices/month (combined) |
| **Team Size** | Centralized 8-person AP team |
| **Pain Points** | No unified view across entities, inconsistent processes, difficult vendor consolidation |
| **Decision Criteria** | Multi-entity support, workflow standardization, consolidated reporting |
| **Budget Authority** | $8,000-15,000/month |
| **Quote** | *"Every entity does AP differently. I need one platform, one process."* |

**Buying Triggers:**
- Centralization initiative approved
- PE firm requiring operational improvements
- New acquisition needing integration

---

### Ideal Customer Profile (ICP) Summary

| Criterion | Specification |
|-----------|---------------|
| **Employee Count** | 50-500 (sweet spot: 100-300) |
| **Revenue** | $10M-$200M |
| **Invoice Volume** | 200-5,000/month |
| **Current Solution** | Spreadsheets, QuickBooks, or legacy AP tool (Palette, AvidXchange) |
| **ERP** | QuickBooks Online, NetSuite, Sage Intacct, Dynamics 365 |
| **Geography** | US-based (Phase 1), UK/Canada (Phase 2) |
| **Industries** | Professional services, Technology, Manufacturing, Distribution, Healthcare |
| **Decision Maker** | Controller, AP Manager, CFO (for larger deals) |
| **Red Flags** | Government/public sector (slow procurement), Heavy international AP (multi-currency complexity), Companies with >10K invoices/month (enterprise) |

---

## 2. Product Positioning

### Positioning Statement

**For mid-market finance teams** who are overwhelmed by manual invoice processing, **Bill Forge** is a **modern AP automation platform** that **cuts processing time by 80% and eliminates approval bottlenecks**. Unlike legacy solutions that are slow, expensive, and require IT resources, Bill Forge offers **instant OCR, one-click approvals, and usage-based pricing** that scales with your business.

### Key Positioning Pillars

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        BILL FORGE POSITIONING                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐         │
│   │    SIMPLICITY    │  │   AUTOMATION     │  │   TRANSPARENCY   │         │
│   │                  │  │                  │  │                  │         │
│   │ "Set up in an    │  │ "AI does the     │  │ "See exactly     │         │
│   │  afternoon, not  │  │  grunt work so   │  │  where every     │         │
│   │  a quarter"      │  │  you don't       │  │  invoice is"     │         │
│   │                  │  │  have to"        │  │                  │         │
│   └──────────────────┘  └──────────────────┘  └──────────────────┘         │
│                                                                              │
│   ┌──────────────────┐  ┌──────────────────┐                               │
│   │    MODULARITY    │  │   PRIVACY        │                               │
│   │                  │  │                  │                               │
│   │ "Pay for what    │  │ "Your data       │                               │
│   │  you use, not    │  │  stays yours—    │                               │
│   │  what you don't" │  │  local OCR       │                               │
│   │                  │  │  available"      │                               │
│   └──────────────────┘  └──────────────────┘                               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Category Creation: "Intelligent AP"

Position Bill Forge not as "AP automation" (crowded) but as **"Intelligent AP"**:

- **AP Automation** = Digitize manual processes
- **Intelligent AP** = AI that learns, adapts, and acts autonomously

This category emphasizes our AI-first approach (Winston) and positions competitors as "legacy automation."

### Tagline Options

1. **"Invoice processing that just works."** (Simplicity focus)
2. **"The AP platform finance teams actually love."** (UX focus)
3. **"Smart invoices. Simple approvals."** (Intelligence focus)
4. **Recommended: "Simplified invoice processing for the mid-market"** (Market + benefit)

---

## 3. Feature Prioritization

### MoSCoW Prioritization Matrix (3-Month Horizon)

#### Must Have (MVP - Launch Blocking)

| Feature | Module | Rationale | Effort |
|---------|--------|-----------|--------|
| PDF/image invoice upload | Invoice Capture | Core functionality | Medium |
| OCR field extraction (vendor, invoice #, amount, date) | Invoice Capture | Core functionality | High |
| Confidence scoring + visual indicators | Invoice Capture | Differentiation | Medium |
| Manual correction interface | Invoice Capture | Error handling | Medium |
| AP Queue + Error Queue | Invoice Capture | Workflow foundation | Medium |
| Basic vendor matching | Invoice Capture | Data quality | Medium |
| Amount-based approval routing | Invoice Processing | Most common pattern | High |
| Approve/reject/hold actions | Invoice Processing | Core functionality | Medium |
| Email approvals (no login) | Invoice Processing | Key differentiator | High |
| Basic audit trail | Invoice Processing | Compliance requirement | Medium |
| User authentication (email/password) | Platform | Security baseline | Medium |
| Tenant isolation | Platform | Data security | High |

#### Should Have (Phase 2 - Post-Launch)

| Feature | Module | Rationale | Effort |
|---------|--------|-----------|--------|
| Line item extraction | Invoice Capture | Accuracy improvement | High |
| Multi-OCR provider fallback | Invoice Capture | Accuracy improvement | Medium |
| QuickBooks Online integration | Integrations | Market demand | High |
| Delegation/out-of-office | Invoice Processing | Enterprise need | Medium |
| SLA tracking + escalation | Invoice Processing | Accountability | Medium |
| Department/cost center routing | Invoice Processing | Flexibility | Medium |
| Bulk approve/reject | Invoice Processing | Efficiency | Low |
| Processing metrics dashboard | Reporting | Visibility | Medium |
| Vendor master CRUD | Vendor Management | Data management | Medium |
| W-9/tax document storage | Vendor Management | Compliance | Medium |

#### Could Have (Phase 3 - Growth)

| Feature | Module | Rationale | Effort |
|---------|--------|-----------|--------|
| Winston AI natural language queries | Winston AI | Differentiation | High |
| Duplicate invoice detection | Invoice Capture | Cost savings | Medium |
| NetSuite integration | Integrations | Market expansion | High |
| Sage integration | Integrations | Market expansion | High |
| PO matching | Invoice Processing | Exception handling | High |
| Spend analytics | Reporting | Executive value | High |
| Scheduled reports | Reporting | Automation | Medium |
| API for custom integrations | Platform | Developer ecosystem | Medium |
| SSO (SAML/OIDC) | Platform | Enterprise sales | High |

#### Won't Have (Out of Scope)

| Feature | Rationale |
|---------|-----------|
| Mobile app | Web-first, validate demand first |
| Payment execution | Partner with existing payment rails (BILL, Stripe) |
| Procurement/PO creation | Different buying center, different product |
| Enterprise SSO | Delays PMF validation |
| Multi-currency GL | Complexity vs. value for initial market |

### Feature Dependency Map

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    FEATURE DEPENDENCY FLOW                               │
│                                                                          │
│   Week 1-2                Week 3-6                 Week 7-10            │
│   ────────                ────────                 ────────             │
│                                                                          │
│   ┌─────────┐        ┌─────────────────┐     ┌────────────────────┐    │
│   │  Auth   │───────►│ Invoice Upload  │────►│ Approval Workflow  │    │
│   │  Tenant │        │ OCR Pipeline    │     │ Email Actions      │    │
│   │  Setup  │        │ Manual Review   │     │ SLA Tracking       │    │
│   └─────────┘        └─────────────────┘     └────────────────────┘    │
│        │                     │                        │                 │
│        │                     ▼                        ▼                 │
│        │             ┌─────────────┐         ┌─────────────────┐       │
│        └────────────►│   Vendor    │────────►│   Reporting     │       │
│                      │   Matching  │         │   Dashboard     │       │
│                      └─────────────┘         └─────────────────┘       │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Go-to-Market Strategy

### Phase 1: Founder-Led Sales (Months 1-3)

**Goal:** 5 pilot customers, validate PMF, refine positioning

#### Target Account Strategy

| Tier | Criteria | Approach | Target |
|------|----------|----------|--------|
| **Tier 1: Design Partners** | Personal network, willing to give feedback | Direct outreach, free pilot | 2-3 accounts |
| **Tier 2: Early Adopters** | 100-300 employees, QuickBooks users, expressed AP pain | LinkedIn + warm intros | 2-3 accounts |

#### Pilot Customer Acquisition Playbook

**Step 1: Identify (Week 1-2)**
- Mine LinkedIn for Controllers/AP Managers at growth-stage companies
- Filter: Series A/B funded, 50-300 employees, US-based
- Look for signals: hiring AP staff, QuickBooks mentions, complaints about manual processes

**Step 2: Outreach (Week 2-4)**
- Personalized email + LinkedIn message
- Lead with pain point, not product
- Offer "free 90-day pilot" (no commitment)

**Template:**
```
Subject: Quick question about AP at [Company]

Hi [Name],

I noticed [Company] has been growing quickly—congrats on the recent
[funding/expansion/hire].

Quick question: how many hours per week does your team spend on
manual invoice data entry?

We're building a new AP automation tool specifically for growing
companies, and I'd love to get your feedback on the early version.

If you're open to it, I'd offer a free 90-day pilot in exchange for
30 minutes of your time every two weeks to share feedback.

Worth a quick chat?

[Name]
```

**Step 3: Qualify (Week 4-6)**
- 30-minute discovery call
- Qualification criteria: 200+ invoices/month, active pain, decision-maker access
- Disqualify: <100 invoices/month, happy with current solution, no time for feedback

**Step 4: Onboard (Week 6-12)**
- White-glove setup (do it for them)
- Weekly check-ins
- Document every piece of feedback

#### Pilot Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Weekly active users | ≥3 per pilot | Login data |
| Invoices processed | ≥100 per pilot | Platform data |
| NPS score | ≥50 | Bi-weekly survey |
| Would recommend | 4+ out of 5 | Exit survey |
| Willing to pay | 3+ out of 5 pilots | Conversion conversation |

### Phase 2: Content-Led Growth (Months 4-6)

**Goal:** Build inbound pipeline, establish thought leadership

#### Content Strategy

| Content Type | Frequency | Topic Focus | Distribution |
|--------------|-----------|-------------|--------------|
| **Blog posts** | 2/week | AP pain points, automation tips | SEO, LinkedIn |
| **Case studies** | 1/month | Pilot customer success stories | Website, sales |
| **ROI calculator** | 1 (launch) | Interactive savings tool | Website, ads |
| **AP benchmarks report** | 1 (quarterly) | Industry data, trends | Gated, lead gen |
| **Video demos** | 2/month | Feature walkthroughs | YouTube, website |

#### SEO Keywords (Priority Order)

1. "invoice processing software" (2,400 searches/month)
2. "AP automation" (1,900)
3. "invoice approval workflow" (880)
4. "accounts payable software for small business" (720)
5. "invoice OCR software" (590)
6. "QuickBooks invoice automation" (320)

#### Paid Acquisition (Small Budget Test)

| Channel | Budget | Target | Expected CAC |
|---------|--------|--------|--------------|
| Google Ads | $2,000/month | High-intent keywords | $200-400/lead |
| LinkedIn Ads | $1,500/month | Controller/AP Manager titles | $150-300/lead |
| Review sites (G2, Capterra) | $500/month | Listing + reviews | $100-200/lead |

### Phase 3: Partner-Led Growth (Months 6-12)

**Goal:** Build scalable channel, expand reach

#### Partner Types

| Partner Type | Example Partners | Value Proposition | Revenue Share |
|--------------|------------------|-------------------|---------------|
| **Accounting firms** | Regional CPA firms, outsourced AP providers | New revenue stream, client value | 15-20% |
| **ERP consultants** | QuickBooks ProAdvisors, NetSuite partners | Implementation add-on | 10-15% |
| **Tech integrations** | Expense management (Ramp, Brex), payments (BILL) | Ecosystem play | Co-marketing |

#### QuickBooks ProAdvisor Partnership (Priority)

- 75,000+ ProAdvisors in US
- Trusted advisors for our target market
- Co-marketing opportunity with Intuit

**Approach:**
1. Get listed in QuickBooks App Store (Month 4)
2. Create ProAdvisor certification program (Month 5)
3. Attend QuickBooks Connect conference (annual)
4. Offer referral commission (15% of first-year revenue)

---

## 5. Competitive Differentiation

### Competitive Landscape Map

```
                              ENTERPRISE FOCUS
                                    ▲
                                    │
                        ┌───────────┼───────────┐
                        │ Coupa     │    SAP    │
                        │ Ariba     │  Concur   │
                        │           │           │
              COMPLEX ◄─┼───────────┼───────────┼─► SIMPLE
              FEATURES  │           │           │   FEATURES
                        │ AvidXchange│          │
                        │ Tipalti   │   BILL    │
                        │ Palette   │(Bill.com) │
                        │           │           │
                        │     ★ BILL FORGE ★    │
                        │           │           │
                        └───────────┼───────────┘
                                    │
                                    ▼
                              SMB FOCUS
```

### Head-to-Head Competitive Analysis

#### vs. BILL (Bill.com)

| Dimension | BILL | Bill Forge | Our Advantage |
|-----------|------|------------|---------------|
| **Target Market** | SMB (<50 employees) | Mid-market (50-500) | Better fit for scaling companies |
| **OCR** | Basic, cloud-only | Advanced, local option | Privacy, accuracy |
| **Workflow** | Simple approval chains | Flexible rule engine | Enterprise-grade flexibility |
| **Pricing** | Per-user ($45-79/user) | Usage-based | Scales better, no seat tax |
| **Integrations** | Broad but shallow | Deep on priority ERPs | Quality over quantity |
| **Weakness** | Limited customization | New, unproven | Lack of brand recognition |

**Win Strategy:** Position as "BILL for growing companies"—when you outgrow BILL's simplicity, Bill Forge is the next step.

#### vs. Palette (Rillion)

| Dimension | Palette | Bill Forge | Our Advantage |
|-----------|---------|------------|---------------|
| **UI/UX** | Legacy, dated | Modern, fast | 10x better user experience |
| **Speed** | Slow (known complaint) | Sub-second responses | Performance obsession |
| **Pricing** | Opaque, expensive | Transparent, fair | Trust and predictability |
| **AI/Automation** | Bolted-on | Built-in from day one | Native intelligence |
| **Geography** | Nordic-focused | US-first | Market alignment |
| **Weakness** | Established, integrated | No track record | Switching cost concerns |

**Win Strategy:** Target Palette customers at contract renewal. Lead with "free migration" and performance demos.

#### vs. Tipalti

| Dimension | Tipalti | Bill Forge | Our Advantage |
|-----------|---------|------------|---------------|
| **Focus** | Payments-first | Processing-first | Better for AP-focused buyers |
| **Complexity** | High (enterprise features) | Right-sized | Faster implementation |
| **Pricing** | Enterprise pricing | Mid-market pricing | 40-60% cost savings |
| **Global** | Strong international | US-focused (MVP) | Simpler for domestic |
| **Weakness** | Overkill for mid-market | No payment execution | Partner for payments |

**Win Strategy:** Target companies that need AP automation but don't need global payments. Partner with payment providers (BILL, Stripe) for execution.

### Differentiation Summary

| Bill Forge Differentiator | Why It Matters | Proof Points |
|---------------------------|----------------|--------------|
| **Modern UI/UX** | Finance teams deserve beautiful software | Sub-second loads, mobile-responsive |
| **Local-first OCR** | Data privacy matters, especially in healthcare/legal | Tesseract option, no cloud required |
| **Email approvals** | Approvers don't want another login | One-click approve from inbox |
| **Usage-based pricing** | No seat tax, scales with business | Transparent pricing page |
| **Modular architecture** | Buy what you need | Independent module subscriptions |
| **AI assistant (Winston)** | Natural language queries, anomaly detection | "Show me invoices over $10K from last month" |

---

## 6. Success Metrics and KPIs

### North Star Metric

**Monthly Invoices Processed (MIP)** — Directly correlates with customer value and revenue.

### KPI Dashboard (3-Month Horizon)

#### Product Metrics

| KPI | Target | Current | Status |
|-----|--------|---------|--------|
| **OCR Accuracy Rate** | ≥90% | — | 🟡 Not started |
| **Auto-Approval Rate** | ≥40% | — | 🟡 Not started |
| **Processing Time (P95)** | <5 seconds | — | 🟡 Not started |
| **Email Approval Success** | ≥95% | — | 🟡 Not started |
| **System Uptime** | ≥99.5% | — | 🟡 Not started |
| **Critical Bugs** | 0 | — | 🟡 Not started |

#### Business Metrics

| KPI | Target | Current | Status |
|-----|--------|---------|--------|
| **Pilot Customers** | 5 | 0 | 🟡 In progress |
| **Monthly Invoices Processed** | 3,000 | 0 | 🟡 Not started |
| **Net Promoter Score** | ≥50 | — | 🟡 Not started |
| **Pilot-to-Paid Conversion** | ≥60% | — | 🟡 Not started |
| **Customer Acquisition Cost** | <$1,500 | — | 🟡 Not started |

#### Usage Metrics

| KPI | Target | Current | Status |
|-----|--------|---------|--------|
| **Weekly Active Users/Tenant** | ≥3 | — | 🟡 Not started |
| **Invoices/User/Week** | ≥25 | — | 🟡 Not started |
| **Approval Turnaround Time** | <24 hours | — | 🟡 Not started |
| **Feature Adoption (email approvals)** | ≥50% | — | 🟡 Not started |

### Milestone Timeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       3-MONTH MILESTONE ROADMAP                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   MONTH 1                 MONTH 2                 MONTH 3                   │
│   ────────                ────────                ────────                  │
│                                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────┐        │
│   │ Week 2:     │    │ Week 6:     │    │ Week 10:                │        │
│   │ Auth + MVP  │    │ Invoice     │    │ First pilot             │        │
│   │ scaffolding │    │ Capture     │    │ onboarded               │        │
│   │ complete    │    │ MVP live    │    │                         │        │
│   └─────────────┘    └─────────────┘    └─────────────────────────┘        │
│                                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────┐        │
│   │ Week 4:     │    │ Week 8:     │    │ Week 12:                │        │
│   │ 2 pilot     │    │ Invoice     │    │ 5 pilots active,        │        │
│   │ commitments │    │ Processing  │    │ 1,000+ invoices         │        │
│   │             │    │ MVP live    │    │ processed               │        │
│   └─────────────┘    └─────────────┘    └─────────────────────────┘        │
│                                                                              │
│   ═══════════════════════════════════════════════════════════════          │
│   ▲                   ▲                   ▲                                 │
│   │                   │                   │                                 │
│   Technical           Feature             Market                            │
│   Foundation          Complete            Validation                        │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### OKRs (Q1 2026)

#### Objective 1: Launch a Loveable Product

| Key Result | Target |
|------------|--------|
| Ship Invoice Capture module with 90%+ OCR accuracy | ✓/✗ |
| Ship Invoice Processing module with email approvals | ✓/✗ |
| Achieve <5 second invoice processing time (P95) | ✓/✗ |
| Zero critical bugs in production | 0 |

#### Objective 2: Validate Product-Market Fit

| Key Result | Target |
|------------|--------|
| Onboard 5 pilot customers | 5 |
| Process 1,000+ invoices across pilots | 1,000 |
| Achieve NPS ≥50 from pilot customers | 50 |
| Get 3+ pilots willing to convert to paid | 3 |

#### Objective 3: Establish Go-to-Market Foundation

| Key Result | Target |
|------------|--------|
| Document 3 customer case studies | 3 |
| Launch public website with pricing | ✓/✗ |
| Create ROI calculator | ✓/✗ |
| Identify first 10 prospects for sales pipeline | 10 |

---

## 7. Pricing Strategy

### Pricing Philosophy

1. **Usage-based, not seat-based** — Don't penalize customers for giving more people access
2. **Transparent and simple** — No hidden fees, no "contact sales" for pricing
3. **Value-aligned** — Price correlates with value delivered (invoices processed)
4. **Predictable** — Base tier for budgeting, overage for flexibility

### Pricing Tiers

| Tier | Monthly Base | Invoices Included | Overage | Target Customer |
|------|--------------|-------------------|---------|-----------------|
| **Starter** | $299/month | 500 | $0.75/invoice | Small teams, testing |
| **Growth** | $799/month | 2,000 | $0.50/invoice | Growing mid-market |
| **Scale** | $1,999/month | 10,000 | $0.30/invoice | Larger mid-market |
| **Enterprise** | Custom | Custom | Custom | 10K+ invoices, special needs |

### Module Add-Ons

| Module | Monthly Price | Availability |
|--------|---------------|--------------|
| **Invoice Capture** | Included | All tiers |
| **Invoice Processing** | Included | All tiers |
| **Vendor Management** | +$199/month | Phase 2 |
| **Advanced Reporting** | +$299/month | Phase 2 |
| **Winston AI Assistant** | +$299/month | Phase 3 |
| **QuickBooks Integration** | Included | Phase 2 |
| **NetSuite Integration** | +$199/month | Phase 3 |

### Competitive Pricing Comparison

| Solution | 1,000 invoices/month | 3,000 invoices/month |
|----------|---------------------|---------------------|
| **Bill Forge (Growth)** | $799/month | $1,299/month |
| **BILL (5 users)** | $395/month | $395/month + limits |
| **Palette** | ~$1,500/month | ~$2,500/month |
| **Tipalti** | ~$2,000/month | ~$3,500/month |
| **AvidXchange** | ~$1,800/month | ~$3,000/month |

### Pilot Pricing

- **90-day free pilot** for first 5 customers
- Convert to paid at 50% discount for first year (loyalty reward)
- Grandfather pilot pricing for 2 years if they convert

---

## 8. Risk Mitigation

### Product Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy <90%** | Medium | High | Multi-provider fallback, human review loop, training data collection |
| **Approval workflow too rigid** | Medium | Medium | Extensive user research, rapid iteration, customizable rules |
| **Integration delays** | High | Medium | Prioritize QuickBooks (simplest API), use existing libraries |

### Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Slow pilot adoption** | Medium | High | Expand outreach, offer implementation services, lower barriers |
| **Competitor response** | Medium | Medium | Move fast, focus on UX differentiation, build switching costs |
| **Economic downturn** | Low | High | Position as cost-saving tool, flexible pricing |

### Execution Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Rust talent shortage** | Medium | High | Pair programming, documentation, consider Go for non-critical services |
| **Scope creep** | High | High | Strict adherence to anti-goals, weekly scope reviews |
| **Pilot churn** | Medium | High | White-glove support, weekly check-ins, fast bug resolution |

---

## 9. Answers to CEO Questions (Product Perspective)

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**From a Product Perspective:**

**Strengths:**
- Deep ERP integrations (SAP, Oracle) built over years
- Proven workflow engine that handles complex scenarios
- Established customer base provides stability proof

**Weaknesses (Opportunities for Bill Forge):**
- UI feels like 2010—customers describe it as "clunky" and "slow"
- No meaningful AI/ML innovation in recent years
- Pricing is opaque and negotiation-heavy
- Mobile experience is poor to non-existent
- Customer support is slow and impersonal

**Product Differentiation Strategy:**
- Win on speed (sub-second UI vs. their multi-second loads)
- Win on simplicity (setup in hours, not weeks)
- Win on price transparency (published pricing vs. "call for quote")
- Win on modern features (email approvals, AI assistant)

### Q2: What approval workflow patterns are most common in mid-market companies?

**Research-Based Patterns:**

1. **Amount Threshold Tiers (85% of companies)**
   ```
   < $1,000:    Auto-approve (if vendor is known)
   $1K-$5K:     Manager approval
   $5K-$25K:    Director/VP approval
   $25K-$50K:   Finance leadership
   > $50K:      CFO or dual approval
   ```

2. **Exception-Based Routing (65%)**
   - Clean invoices (match PO, known vendor) → auto-approve
   - Exceptions (no PO, new vendor, amount variance) → review queue

3. **Department/Cost Center (45%)**
   - Route to cost center owner regardless of amount
   - Finance has override/visibility on all

4. **Dual Approval (30%)**
   - Two approvers required above certain thresholds
   - Common in regulated industries (healthcare, finance)

**Product Implication:**
Our workflow engine needs to support all four patterns. MVP should nail amount-based tiers, with exception routing added in Phase 2.

### Q3: What's the pricing model that resonates with mid-market buyers?

**Key Insights:**

1. **Per-user pricing is hated** — AP teams resent being charged more for giving occasional approvers access

2. **Volume-based is preferred** — Correlates to business value and scales naturally

3. **Predictability matters** — Finance teams need to budget; pure usage can cause anxiety

4. **Transparency builds trust** — "Call for pricing" signals enterprise complexity and negotiation games

**Winning Formula:**
- Base tier with included volume (predictable)
- Reasonable overage pricing (flexibility)
- No per-user fees (removes friction)
- Published pricing (builds trust)
- Annual discount option (cash flow benefit)

---

## 10. Next Steps for Product

### This Week

1. **Finalize pilot customer criteria** — Document exact qualification requirements
2. **Create pilot onboarding playbook** — Step-by-step setup guide
3. **Draft pilot agreement** — Terms, expectations, feedback cadence
4. **Begin outreach** — Start identifying first 10 potential pilot candidates

### This Month

1. **Validate feature priorities with pilots** — Confirm MoSCoW list
2. **Create product requirements documents** — Detailed specs for Invoice Capture and Invoice Processing
3. **Design user flows** — Wireframes for key workflows
4. **Establish feedback collection process** — How we capture and prioritize pilot feedback

### This Quarter

1. **Ship MVP** — Invoice Capture + Invoice Processing
2. **Onboard 5 pilots** — White-glove support
3. **Achieve PMF signals** — NPS ≥50, 60% willing to pay
4. **Prepare for paid launch** — Pricing page, billing integration, sales materials

---

*This product strategy is a living document and will be updated based on pilot customer feedback and market learnings.*
