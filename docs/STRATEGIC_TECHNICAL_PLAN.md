# Bill Forge: Strategic Technical Plan

**Document Version:** 1.0
**Date:** January 30, 2026
**Prepared by:** CTO Office
**Horizon:** 3 Months (MVP Launch)

---

## Executive Summary

Bill Forge has a solid architectural foundation with approximately 30-40% of MVP requirements implemented. The Rust/Axum backend with database-per-tenant isolation is well-designed, and the Next.js frontend has a rich component library. However, significant gaps exist in workflow execution, email integration, ERP connectivity, and reporting that must be closed before launching to the 5 pilot customers.

This plan outlines a phased approach to complete the MVP within the 3-month horizon, with clear priorities, resource requirements, and risk mitigations.

---

## 1. Technical Architecture Recommendations

### 1.1 Current Architecture Assessment

**Strengths to Preserve:**
- Database-per-tenant isolation (SQLite per tenant) - excellent for data privacy and GDPR compliance
- Modular crate architecture enables independent module development
- Rust backend provides performance headroom for OCR processing
- JWT-based stateless authentication scales horizontally
- OpenAPI/Swagger documentation infrastructure

**Architecture Gaps to Address:**

| Gap | Impact | Recommendation |
|-----|--------|----------------|
| No database migrations framework | High | Implement `sqlx` migrations or custom versioned SQL |
| SQLite only (no PostgreSQL) | Medium | Keep SQLite for MVP; add PostgreSQL adapter in Phase 3 |
| No DuckDB analytics | Medium | Defer to Phase 2; SQLite sufficient for MVP analytics |
| No caching layer | Low | Add Redis post-MVP for scale |
| No message queue | Medium | Use in-process queues for MVP; add RabbitMQ/SQS later |

### 1.2 Recommended Target Architecture (MVP)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Load Balancer (Nginx)                     │
└─────────────────────────────────────────────────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        │                          │                          │
        ▼                          ▼                          ▼
┌───────────────┐          ┌───────────────┐          ┌───────────────┐
│   Next.js     │          │   Next.js     │          │   Next.js     │
│   Frontend    │          │   Frontend    │          │   Frontend    │
│   (Replica)   │          │   (Replica)   │          │   (Replica)   │
└───────────────┘          └───────────────┘          └───────────────┘
        │                          │                          │
        └──────────────────────────┼──────────────────────────┘
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │    API Gateway      │
                        │  (Rate Limiting)    │
                        └─────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        │                          │                          │
        ▼                          ▼                          ▼
┌───────────────┐          ┌───────────────┐          ┌───────────────┐
│   Rust API    │          │   Rust API    │          │   Rust API    │
│   (Replica)   │          │   (Replica)   │          │   (Replica)   │
└───────────────┘          └───────────────┘          └───────────────┘
        │                          │                          │
        └──────────────────────────┼──────────────────────────┘
                                   │
        ┌──────────────┬───────────┼───────────┬──────────────┐
        │              │           │           │              │
        ▼              ▼           ▼           ▼              ▼
┌─────────────┐  ┌─────────┐  ┌────────┐  ┌────────┐  ┌─────────────┐
│   Metadata  │  │ Tenant  │  │ Tenant │  │ Tenant │  │     S3      │
│   Database  │  │  DB 1   │  │  DB 2  │  │  DB N  │  │   Storage   │
│  (SQLite)   │  │(SQLite) │  │(SQLite)│  │(SQLite)│  │   (MinIO)   │
└─────────────┘  └─────────┘  └────────┘  └────────┘  └─────────────┘
```

### 1.3 OCR Pipeline Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                        Invoice Upload                               │
└────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────┐
│                     Document Preprocessor                           │
│  • PDF to image conversion (multi-page support)                     │
│  • Image enhancement (contrast, deskew, noise reduction)            │
│  • Format detection                                                 │
└────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────┐
│                      OCR Provider Router                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │   Tesseract  │  │ AWS Textract │  │Google Vision │              │
│  │   (Local)    │  │   (Cloud)    │  │   (Cloud)    │              │
│  │   DEFAULT    │  │   FALLBACK   │  │   OPTIONAL   │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────┐
│                      Field Extractor                                │
│  • Pattern matching for invoice fields                              │
│  • ML-based field detection (future)                                │
│  • Line-item parsing                                                │
│  • Confidence scoring per field                                     │
└────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────┐
│                    Vendor Matcher                                   │
│  • Fuzzy name matching (Levenshtein distance)                       │
│  • Historical invoice correlation                                   │
│  • Confidence-based routing                                         │
└────────────────────────────────────────────────────────────────────┘
                                   │
              ┌────────────────────┴────────────────────┐
              │                                         │
              ▼                                         ▼
┌─────────────────────────┐              ┌─────────────────────────┐
│      AP Queue           │              │     Error Queue         │
│  (Confidence >= 85%)    │              │   (Confidence < 85%)    │
│  Ready for workflow     │              │   Needs manual review   │
└─────────────────────────┘              └─────────────────────────┘
```

### 1.4 Approval Workflow Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                    Invoice Ready for Approval                       │
└────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌────────────────────────────────────────────────────────────────────┐
│                      Workflow Engine                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Rule Evaluator                            │   │
│  │  • Amount-based rules ($0-5K auto-approve, $5K-50K manager) │   │
│  │  • Department routing rules                                  │   │
│  │  • Vendor-specific rules                                     │   │
│  │  • Custom conditional logic                                  │   │
│  └─────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        │                          │                          │
        ▼                          ▼                          ▼
┌───────────────┐          ┌───────────────┐          ┌───────────────┐
│  Auto-Approve │          │ Single-Level  │          │  Multi-Level  │
│   (< $5,000)  │          │   Approval    │          │   Approval    │
└───────────────┘          │ ($5K - $50K)  │          │   (> $50K)    │
        │                  └───────────────┘          └───────────────┘
        │                          │                          │
        │                          ▼                          ▼
        │                  ┌───────────────┐          ┌───────────────┐
        │                  │  Email Notify │          │  Email Notify │
        │                  │   + In-App    │          │   + In-App    │
        │                  └───────────────┘          └───────────────┘
        │                          │                          │
        │                          ▼                          │
        │                  ┌───────────────┐                  │
        │                  │   SLA Timer   │◄─────────────────┤
        │                  │  (Escalation) │                  │
        │                  └───────────────┘                  │
        │                          │                          │
        └──────────────────────────┼──────────────────────────┘
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │   Ready for Payment │
                        │       Queue         │
                        └─────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Stack Confirmation (Aligned with CEO Preferences)

| Layer | Technology | Status | Notes |
|-------|------------|--------|-------|
| **Backend Runtime** | Rust 1.75+ | ✅ Confirmed | Keep current |
| **Web Framework** | Axum 0.7 | ✅ Confirmed | Keep current |
| **Frontend** | Next.js 14+ | ✅ Confirmed | App Router in use |
| **UI Components** | shadcn/ui + Radix | ✅ Confirmed | 60+ components ready |
| **Styling** | Tailwind CSS 3.4 | ✅ Confirmed | Keep current |
| **State Management** | Zustand | ✅ Confirmed | Keep current |
| **Form Handling** | React Hook Form + Zod | ✅ Confirmed | Keep current |
| **Data Tables** | TanStack Table | ✅ Confirmed | Keep current |
| **OLTP Database** | SQLite (bundled) | ✅ MVP | PostgreSQL post-MVP |
| **Analytics DB** | SQLite | ✅ MVP | DuckDB post-MVP |
| **File Storage** | Local / S3-compatible | ✅ Confirmed | MinIO for dev |
| **OCR (Primary)** | Tesseract 5 | ✅ Confirmed | Local-first |
| **OCR (Cloud)** | AWS Textract | ⏳ Add | Feature flag |
| **Email** | SMTP (lettre crate) | ⏳ Add | Priority for MVP |
| **Authentication** | JWT + Argon2 | ✅ Confirmed | Keep current |

### 2.2 New Dependencies Required

**Backend (Cargo.toml additions):**

```toml
# Email
lettre = { version = "0.11", features = ["smtp-transport", "tokio1-native-tls"] }

# Image processing for OCR
image = "0.24"
pdf = "0.8"  # or lopdf for PDF parsing

# Fuzzy matching for vendors
strsim = "0.11"  # Levenshtein distance

# Background job processing (MVP: in-process)
tokio-cron-scheduler = "0.9"

# AWS Textract (optional)
aws-sdk-textract = { version = "1.0", optional = true }

# Database migrations
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"], optional = true }
```

**Frontend (package.json additions):**

```json
{
  "dependencies": {
    "@tanstack/react-virtual": "^3.0.0",  // Virtual scrolling for large lists
    "date-fns": "^3.0.0",  // Date manipulation
    "zod": "^3.22.0"  // Already present, ensure latest
  }
}
```

### 2.3 ERP Integration Decision

Based on the target market (mid-market companies with 10-1000 employees), the recommended first ERP integration is:

**Primary: QuickBooks Online**
- Rationale: Highest adoption in lower mid-market, REST API, well-documented
- API: QuickBooks Online API v3
- Auth: OAuth 2.0
- Effort: 2-3 weeks

**Secondary: NetSuite (Phase 2)**
- Rationale: Standard for growing mid-market
- API: SuiteTalk REST / SOAP
- Auth: Token-based
- Effort: 3-4 weeks

**Defer: SAP, Dynamics 365**
- Rationale: Enterprise complexity, not aligned with anti-goals
- Revisit after product-market fit

---

## 3. Development Priorities and Phases

### Phase 1: Core Completion (Weeks 1-4)

**Goal:** Complete invoice capture and processing modules to functional MVP state.

#### Week 1-2: OCR Pipeline Completion

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Implement PDF multi-page support | Backend | P0 | 3d |
| Add image preprocessing (contrast, deskew) | Backend | P0 | 2d |
| Implement confidence threshold routing | Backend | P0 | 2d |
| Wire OCR results to frontend review UI | Fullstack | P0 | 3d |
| Add vendor fuzzy matching (Levenshtein) | Backend | P0 | 2d |
| Create error queue UI | Frontend | P0 | 2d |
| Add field-level correction UI | Frontend | P1 | 3d |

**Files to modify:**
- `backend/crates/invoice-capture/src/ocr/mod.rs` - Add preprocessing
- `backend/crates/invoice-capture/src/service.rs` - Add confidence routing
- `backend/crates/core/src/traits.rs` - Add VendorMatcher trait
- `apps/web/src/app/(dashboard)/invoices/upload/page.tsx` - Wire OCR flow
- `apps/web/src/app/(dashboard)/processing/queues/` - Error queue UI

#### Week 3-4: Workflow Engine Activation

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Wire WorkflowEngine to invoice handlers | Backend | P0 | 3d |
| Implement approval request creation | Backend | P0 | 2d |
| Add email notification service (SMTP) | Backend | P0 | 3d |
| Create email approval links (tokenized) | Backend | P0 | 3d |
| Build approval UI with actions | Frontend | P0 | 3d |
| Add delegation support | Backend | P1 | 2d |
| Implement SLA tracking with escalation | Backend | P1 | 2d |

**Files to modify:**
- `backend/crates/invoice-processing/src/engine.rs` - Activate evaluation
- `backend/crates/api/src/routes/invoices.rs` - Wire workflow
- `backend/crates/email/src/lib.rs` - Implement SMTP
- `apps/web/src/app/(dashboard)/processing/approvals/page.tsx` - Approval UI

### Phase 2: Integration & Reporting (Weeks 5-8)

**Goal:** Add first ERP integration and essential reporting.

#### Week 5-6: QuickBooks Integration

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Create QuickBooks OAuth flow | Backend | P0 | 2d |
| Implement vendor sync | Backend | P0 | 3d |
| Implement GL account validation | Backend | P0 | 2d |
| Create invoice export to QuickBooks | Backend | P0 | 3d |
| Build integration settings UI | Frontend | P0 | 2d |
| Add connection status monitoring | Fullstack | P1 | 2d |

**New files:**
- `backend/crates/integrations/` - New crate for ERP integrations
- `backend/crates/integrations/src/quickbooks/` - QB-specific code
- `apps/web/src/app/(dashboard)/settings/integrations/` - Integration UI

#### Week 7-8: Reporting Dashboard

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Implement invoice processing metrics API | Backend | P0 | 3d |
| Create dashboard widgets (volume, automation %) | Frontend | P0 | 3d |
| Add approval performance metrics | Backend | P0 | 2d |
| Build spend analysis by vendor/dept | Backend | P1 | 3d |
| Create invoice aging report | Backend | P1 | 2d |
| Add Excel/CSV export | Backend | P0 | 2d |
| Wire Recharts components to real data | Frontend | P0 | 3d |

**Files to modify:**
- `backend/crates/reporting/src/` - Implement metrics
- `backend/crates/api/src/routes/reports.rs` - Wire endpoints
- `apps/web/src/app/(dashboard)/reports/page.tsx` - Dashboard UI
- `apps/web/src/components/ui/charts.tsx` - Real data

### Phase 3: Polish & Pilot (Weeks 9-12)

**Goal:** Prepare for 5 pilot customers with production hardening.

#### Week 9-10: Production Hardening

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Implement database migrations framework | Backend | P0 | 3d |
| Add comprehensive error handling | Backend | P0 | 2d |
| Implement request logging & audit trail | Backend | P0 | 2d |
| Add rate limiting enforcement | Backend | P1 | 1d |
| Create tenant provisioning workflow | Backend | P0 | 2d |
| Build admin tenant management UI | Frontend | P0 | 3d |
| Performance optimization (queries, indexes) | Backend | P1 | 3d |

#### Week 11-12: Testing & Documentation

| Task | Owner | Priority | Effort |
|------|-------|----------|--------|
| Write integration tests for invoice flow | QA | P0 | 5d |
| Write E2E tests with Playwright | QA | P0 | 5d |
| Load testing (100 concurrent users) | DevOps | P1 | 2d |
| OCR accuracy benchmarking (target 90%) | QA | P0 | 3d |
| User documentation for pilot | PM | P0 | 3d |
| API documentation completion | Backend | P1 | 2d |
| Pilot onboarding materials | PM | P0 | 2d |

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Tesseract OCR accuracy below 90%** | Medium | High | Add AWS Textract as fallback; implement preprocessing; test with real invoices early |
| **SQLite performance at scale** | Low (MVP) | Medium | Monitor tenant DB sizes; have PostgreSQL adapter ready for post-MVP |
| **Email deliverability issues** | Medium | High | Use reputable SMTP provider (SendGrid/Mailgun); implement SPF/DKIM; test approval links extensively |
| **QuickBooks API rate limits** | Medium | Medium | Implement exponential backoff; batch operations; cache validation results |
| **Multi-tenant data leakage** | Low | Critical | Extensive testing of tenant isolation; code review of all DB queries; middleware enforcement |
| **Browser compatibility** | Low | Medium | Test in Chrome, Firefox, Safari, Edge; use polyfills if needed |

### 4.2 Schedule Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Underestimated OCR complexity** | Medium | High | Allocate buffer week in Phase 1; consider third-party OCR service as backup |
| **QuickBooks OAuth complexity** | Medium | Medium | Use existing OAuth library; allocate extra time for edge cases |
| **Frontend-backend integration delays** | Medium | Medium | Define API contracts early; use mock services; daily standups |
| **Testing takes longer than expected** | High | Medium | Start automated testing in Phase 1; allocate 20% buffer |

### 4.3 Business Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Pilot customers have different requirements** | Medium | High | Early customer discovery calls; focus on common denominators |
| **Competitors move faster** | Medium | Medium | Focus on differentiators (local OCR, modular pricing) |
| **Team burnout** | Medium | High | Realistic estimates; avoid scope creep; maintain anti-goals |

---

## 5. Resource Requirements

### 5.1 Team Composition (Recommended)

| Role | FTE | Responsibilities |
|------|-----|------------------|
| **Backend Engineer (Rust)** | 2.0 | OCR pipeline, workflow engine, ERP integration, API |
| **Frontend Engineer (React)** | 1.5 | Dashboard, invoice review, approval flows, settings |
| **Full-Stack Engineer** | 1.0 | Integration work, email service, database |
| **DevOps/SRE** | 0.5 | CI/CD, deployment, monitoring, security |
| **QA Engineer** | 0.5 | Testing strategy, automation, OCR benchmarking |
| **Product Manager** | 0.5 | Requirements, pilot coordination, documentation |

**Total:** 6.0 FTE for 3-month MVP

### 5.2 Infrastructure Requirements

| Resource | Environment | Specification | Cost Estimate |
|----------|-------------|---------------|---------------|
| **Development VMs** | Dev | 3x 4 vCPU, 16GB RAM | ~$300/mo |
| **Staging Server** | Staging | 2 vCPU, 8GB RAM | ~$80/mo |
| **Production Cluster** | Prod | 3x 4 vCPU, 16GB RAM (HA) | ~$450/mo |
| **S3 Storage** | All | 100GB initial | ~$25/mo |
| **SMTP Service** | All | SendGrid Pro | ~$90/mo |
| **Domain + SSL** | Prod | Let's Encrypt | Free |
| **Monitoring** | Prod | Grafana Cloud Free | Free |
| **CI/CD** | All | GitHub Actions | Free (OSS limits) |

**Total Infrastructure:** ~$950/month

### 5.3 External Services

| Service | Purpose | Cost Model |
|---------|---------|------------|
| **AWS Textract** | Cloud OCR fallback | Pay per page (~$0.015/page) |
| **QuickBooks Developer** | ERP integration | Free (API access) |
| **SendGrid** | Transactional email | $89/mo (Pro plan) |

---

## 6. Timeline Estimates

### 6.1 Milestone Summary

| Milestone | Target Date | Deliverables |
|-----------|-------------|--------------|
| **M1: OCR Pipeline Complete** | Week 2 | Multi-page PDF, preprocessing, confidence routing |
| **M2: Workflow Activated** | Week 4 | Email approvals, SLA tracking, delegation |
| **M3: QuickBooks Live** | Week 6 | OAuth, vendor sync, GL validation |
| **M4: Reporting Dashboard** | Week 8 | Metrics, aging reports, exports |
| **M5: Production Ready** | Week 10 | Migrations, logging, rate limiting |
| **M6: Pilot Launch** | Week 12 | 5 customers onboarded, monitoring active |

### 6.2 Detailed Gantt View

```
Week:    1    2    3    4    5    6    7    8    9   10   11   12
         ├────┼────┼────┼────┼────┼────┼────┼────┼────┼────┼────┤
OCR      ████████████
         │   PDF │ Confidence
         │   Preprocess │ Routing

Workflow      ████████████
              │  Engine │ Email
              │  Activation │ Approvals

QuickBooks              ████████████
                        │ OAuth │ Sync
                        │      │ Export

Reporting                    ████████████
                             │ Metrics │ Export
                             │ Dashboard │

Hardening                              ████████████
                                       │ Migrations │ Logging
                                       │ Rate Limit │

Testing   ────────────────────────────────────████████████████
          Continuous                           Intensive

Pilot                                                    ████████
                                                         Onboard
```

---

## 7. Leveraging the Existing Codebase

### 7.1 What to Keep As-Is

| Component | Reason |
|-----------|--------|
| Database-per-tenant architecture | Excellent isolation, GDPR-ready |
| JWT authentication flow | Secure, well-implemented |
| Crate modular structure | Clean separation of concerns |
| shadcn/ui component library | Rich, consistent styling |
| API route structure | RESTful, documented |
| Docker multi-stage builds | Production-ready |
| GitHub Actions CI/CD | Comprehensive pipeline |

### 7.2 What to Extend

| Component | Current State | Extension Needed |
|-----------|---------------|------------------|
| `invoice-capture/src/ocr/` | Basic Tesseract | Add preprocessing, multi-page, Textract |
| `invoice-processing/src/engine.rs` | Rules defined | Wire to API handlers, add execution |
| `email/src/` | Service trait defined | Implement SMTP with lettre |
| `db/src/repositories/` | CRUD implemented | Add transaction handling |
| `reporting/src/` | Stub only | Implement metrics queries |
| Frontend pages | Layouts exist | Wire to actual API data |

### 7.3 What to Add

| New Component | Purpose | Location |
|---------------|---------|----------|
| Integrations crate | ERP adapters | `backend/crates/integrations/` |
| Vendor matcher | Fuzzy name matching | `backend/crates/invoice-capture/src/vendor_matcher.rs` |
| Email templates | Approval notifications | `backend/crates/email/src/templates/` |
| Migrations | Schema versioning | `backend/migrations/` |
| E2E tests | Invoice flow validation | `apps/web/e2e/` |

### 7.4 What to Remove/Refactor

| Component | Issue | Action |
|-----------|-------|--------|
| Unused personas enforcement | Roles defined but not enforced | Add middleware to enforce |
| Inconsistent error handling | Good types, inconsistent usage | Standardize across handlers |
| Mock data in frontend | Hardcoded values | Replace with API calls |
| Billing crate (stub) | Not needed for MVP | Keep stub, don't invest time |

---

## 8. CEO Questions Answered

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**Palette (Rillion) Strengths:**
- Strong Nordic/European market presence
- Mature AP automation features
- Good compliance tooling

**Palette Weaknesses:**
- Complex pricing (per-invoice fees)
- Older UI/UX design
- Limited AI/ML capabilities
- Slower innovation cycle

**Bill Forge Differentiation:**
1. **Local-first OCR** - Data never leaves tenant control
2. **Modular pricing** - Buy only what you need
3. **Modern tech stack** - Faster, better UX
4. **AI-ready architecture** - Winston foundation in place

### Q2: Ideal OCR accuracy threshold?

**Recommendation: 85% field-level confidence**

| Confidence | Routing |
|------------|---------|
| >= 95% | Auto-approve (high-value fields correct) |
| 85-94% | AP Queue (review but likely correct) |
| < 85% | Error Queue (manual review required) |

Rationale: Tesseract typically achieves 85-95% on clean documents. Start conservative, tune based on pilot feedback.

### Q3: Which ERP integration first?

**Recommendation: QuickBooks Online**

Rationale:
- Highest adoption in lower mid-market (target: 10-1000 employees)
- Modern REST API, well-documented
- OAuth 2.0 standard flow
- Fast time-to-market (2-3 weeks)
- Good validation: If it works for QB, NetSuite will be similar

### Q4: Common approval workflow patterns?

**Pattern 1: Amount-Based Escalation (Most Common)**
```
< $5,000      → Auto-approve (if vendor known)
$5K - $25K    → Manager approval
$25K - $100K  → Finance lead + Manager
> $100K       → CFO + Finance lead
```

**Pattern 2: Department-Based**
```
IT invoices    → IT Director
Marketing      → CMO
All others     → Finance team
```

**Pattern 3: Exception-Based**
```
Clean match    → Auto-approve
PO mismatch    → Requestor + Manager
New vendor     → Finance review
```

**Implementation Priority:** Amount-based first, then department routing.

### Q5: Multi-currency handling?

**Competitors approach:**
- BILL: USD-centric, basic multi-currency
- Tipalti: Strong multi-currency (global focus)
- Coupa: Full multi-currency (enterprise)

**Recommendation for MVP:**
- Store currency code with amount
- Display in original currency
- Defer conversion to ERP
- Add currency selector in UI

**Post-MVP:** Integrate exchange rate API (Open Exchange Rates) for reporting.

### Q6: Pricing model for mid-market?

**Competitive Landscape:**
| Vendor | Model | Typical Price |
|--------|-------|---------------|
| BILL | Per-user + per-transaction | $45/user/mo + $0.49/tx |
| Tipalti | Per-supplier | ~$3-5/supplier/mo |
| AvidXchange | Per-invoice | ~$5-8/invoice |
| Palette | Per-invoice | ~$4-6/invoice |

**Recommendation: Hybrid Usage-Based**

```
Base Platform Fee: $299/mo (up to 3 users)
Additional Users: $49/user/mo
Invoice Processing:
  - First 100/mo: Included
  - 101-500: $1.50/invoice
  - 501-1000: $1.00/invoice
  - 1000+: $0.75/invoice

Module Add-ons:
  - Vendor Management: +$99/mo
  - Advanced Reporting: +$149/mo
  - ERP Integration: +$199/mo per connector
```

This aligns with "buy what you need" positioning and scales with customer growth.

---

## 9. Success Metrics

### 9.1 Technical KPIs

| Metric | Target | Measurement |
|--------|--------|-------------|
| OCR Field Accuracy | >= 90% | Benchmark test suite |
| API Response Time (p95) | < 500ms | Application monitoring |
| Invoice Processing Time | < 2 min | End-to-end timing |
| System Uptime | >= 99.5% | Health check monitoring |
| Error Rate | < 1% | API error tracking |

### 9.2 Business KPIs (Pilot)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Pilot Customer NPS | >= 40 | Survey |
| Invoice Automation Rate | >= 50% | Auto-approved / total |
| User Adoption | >= 80% | Active users / total |
| Support Tickets | < 5/week/customer | Ticket system |
| Workflow Completion Rate | >= 95% | Invoices processed / submitted |

---

## 10. Appendix: File-Level Implementation Guide

### 10.1 Backend Changes Summary

| File | Changes |
|------|---------|
| `backend/Cargo.toml` | Add lettre, image, strsim dependencies |
| `backend/crates/invoice-capture/src/ocr/mod.rs` | Add preprocessing, multi-page PDF |
| `backend/crates/invoice-capture/src/ocr/textract.rs` | New - AWS Textract provider |
| `backend/crates/invoice-capture/src/vendor_matcher.rs` | New - Fuzzy matching |
| `backend/crates/invoice-processing/src/engine.rs` | Wire to API, add execution |
| `backend/crates/email/src/lib.rs` | Implement SMTP with lettre |
| `backend/crates/email/src/templates/` | New - Email templates |
| `backend/crates/api/src/routes/invoices.rs` | Wire workflow engine |
| `backend/crates/api/src/routes/approvals.rs` | New - Approval endpoints |
| `backend/crates/reporting/src/metrics.rs` | New - Metrics queries |
| `backend/crates/integrations/` | New crate - ERP adapters |

### 10.2 Frontend Changes Summary

| File | Changes |
|------|---------|
| `apps/web/src/lib/api.ts` | Add missing endpoints |
| `apps/web/src/app/(dashboard)/invoices/upload/page.tsx` | Wire OCR flow |
| `apps/web/src/app/(dashboard)/processing/queues/page.tsx` | Real queue data |
| `apps/web/src/app/(dashboard)/processing/approvals/page.tsx` | Approval actions |
| `apps/web/src/app/(dashboard)/reports/page.tsx` | Wire metrics |
| `apps/web/src/app/(dashboard)/settings/integrations/` | New - ERP settings |
| `apps/web/src/components/InvoicePanel.tsx` | Field editing, confidence |

---

## Approval

| Role | Name | Date | Signature |
|------|------|------|-----------|
| CEO | | | |
| CTO | | | |
| Engineering Lead | | | |

---

*This document should be reviewed weekly and updated as implementation progresses.*
