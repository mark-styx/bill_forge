# Bill Forge - CTO Execution Plan

**Version:** 1.0
**Date:** February 1, 2026
**Author:** CTO, Bill Forge
**Status:** Ready for Execution
**Horizon:** 12 Weeks (Q1 2026)

---

## Executive Summary

This execution plan operationalizes the strategic technical plan for Bill Forge, a modular B2B SaaS platform for mid-market invoice processing automation. The platform targets companies with 50-500 employees who are underserved by enterprise solutions and have outgrown SMB tools.

### Current State Assessment

| Component | Status | Location | Action Required |
|-----------|--------|----------|-----------------|
| **Strategic Planning** | Complete | `/docs/CTO_STRATEGIC_PLAN_FINAL.md` | Execute |
| **Product Strategy** | Complete | `/docs/cpo_product_strategy.md` | Align |
| **Locust Framework** | Functional | `/src/locust/` | Adapt for Winston (Phase 3) |
| **Bill Forge Core** | Not Started | N/A | Build from scratch |
| **Frontend** | Not Started | N/A | Build from scratch |
| **Database** | Designed | Strategic plan | Implement |

### Key Decision: Greenfield Build

Bill Forge will be built as a new application in **Rust/Axum + Next.js 14+** as specified in the CEO vision. The existing Locust Python framework will be repurposed for the Winston AI Assistant in Phase 3.

---

## 1. Technical Architecture

### 1.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            BILL FORGE PLATFORM                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                        NEXT.JS 14+ FRONTEND                            │ │
│  │                                                                         │ │
│  │  /invoices       /approvals       /vendors        /analytics           │ │
│  │  • Upload        • Inbox          • Directory     • Dashboards         │ │
│  │  • OCR Review    • Email Actions  • Tax Docs      • Reports            │ │
│  │  • Queues        • SLA Tracking   • Matching      • Export             │ │
│  │                                                                         │ │
│  │  Tech: shadcn/ui | Tailwind CSS | TanStack Query | React Hook Form    │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                      │                                       │
│                                      ▼                                       │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                        RUST API GATEWAY (bf-api)                       │ │
│  │                                                                         │ │
│  │  Axum 0.7+ | JWT Auth | Tenant Resolution | Rate Limiting | CORS      │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                      │                                       │
│         ┌────────────────────────────┼────────────────────────────┐         │
│         ▼                            ▼                            ▼         │
│  ┌──────────────┐          ┌──────────────┐          ┌──────────────┐      │
│  │  bf-invoice  │          │  bf-workflow │          │  bf-vendor   │      │
│  ├──────────────┤          ├──────────────┤          ├──────────────┤      │
│  │ Upload API   │          │ Rule Engine  │          │ Master Data  │      │
│  │ OCR Pipeline │          │ State Machine│          │ Tax Storage  │      │
│  │ Extraction   │          │ Notifications│          │ Fuzzy Match  │      │
│  │ Queue Route  │          │ Email Actions│          │ Spend View   │      │
│  └──────────────┘          └──────────────┘          └──────────────┘      │
│         │                            │                            │         │
│         └────────────────────────────┼────────────────────────────┘         │
│                                      ▼                                       │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                     bf-ocr (Provider Abstraction)                      │ │
│  │                                                                         │ │
│  │   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                │ │
│  │   │ Tesseract 5 │    │AWS Textract │    │Google Vision│                │ │
│  │   │ (Primary)   │    │ (Fallback)  │    │ (Fallback)  │                │ │
│  │   │   FREE      │    │  $0.01/pg   │    │ $0.0015/pg  │                │ │
│  │   └─────────────┘    └─────────────┘    └─────────────┘                │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                              DATA LAYER                                      │
│                                                                              │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐                 │
│  │ Control Plane  │  │  Tenant DBs    │  │  MinIO (S3)    │                 │
│  │  PostgreSQL    │  │   PostgreSQL   │  │                │                 │
│  │                │  │                │  │  /tenant-a/    │                 │
│  │ • tenants      │  │ bf_tenant_acme:│  │    /invoices/  │                 │
│  │ • users        │  │  • invoices    │  │    /tax-docs/  │                 │
│  │ • api_keys     │  │  • vendors     │  │                │                 │
│  │ • subscriptions│  │  • workflows   │  │  /tenant-b/    │                 │
│  └────────────────┘  └────────────────┘  └────────────────┘                 │
│                                                                              │
│  ┌────────────────┐  ┌────────────────┐                                     │
│  │    DuckDB      │  │    Redis       │                                     │
│  │  (Per-Tenant)  │  │                │                                     │
│  │                │  │ • Sessions     │                                     │
│  │ • Metrics      │  │ • Rate limits  │                                     │
│  │ • Aggregates   │  │ • Job queues   │                                     │
│  │ • Reports      │  │ • Pub/Sub      │                                     │
│  └────────────────┘  └────────────────┘                                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Database-per-Tenant Architecture

**Decision:** Complete tenant isolation via separate databases (not row-level security)

```
                    ┌───────────────────────────────────────┐
                    │           CONTROL PLANE               │
                    │                                       │
                    │  control_plane_db (PostgreSQL)        │
                    │  ┌───────────────────────────────┐    │
                    │  │ tenants                        │    │
                    │  │  - id: uuid                    │    │
                    │  │  - slug: "acme-corp"           │    │
                    │  │  - db_name: "bf_tenant_acme"   │    │
                    │  │  - modules: ["capture","proc"] │    │
                    │  │  - settings: JSONB             │    │
                    │  └───────────────────────────────┘    │
                    └───────────────────────────────────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    ▼                  ▼                  ▼
            ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
            │bf_tenant_acme│   │bf_tenant_tech│   │bf_tenant_mfg │
            ├──────────────┤   ├──────────────┤   ├──────────────┤
            │ • invoices   │   │ • invoices   │   │ • invoices   │
            │ • vendors    │   │ • vendors    │   │ • vendors    │
            │ • workflows  │   │ • workflows  │   │ • workflows  │
            │ • audit_log  │   │ • audit_log  │   │ • audit_log  │
            └──────────────┘   └──────────────┘   └──────────────┘
```

**Rationale:**
- Complete data isolation for compliance (HIPAA, SOC 2)
- Per-tenant backup/restore capability
- Easy data portability (customer can export entire database)
- No cross-tenant query risk
- Trade-off: Higher connection overhead (mitigated by connection pooling)

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          OCR PROCESSING PIPELINE                             │
│                                                                              │
│   ┌───────────┐                                                              │
│   │  INGEST   │  Supported: PDF, PNG, JPG, TIFF                             │
│   │           │  Max size: 25MB                                              │
│   │           │  Validation: file type, ClamAV malware scan                 │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌───────────┐                                                              │
│   │PREPROCESS │  • Deskew (correct rotation)                                │
│   │           │  • Contrast enhancement                                      │
│   │           │  • Noise reduction                                           │
│   │           │  • Document classification                                   │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────────┐       │
│   │                      PROVIDER ROUTER                             │       │
│   │                                                                  │       │
│   │   Privacy Mode = TRUE?                                           │       │
│   │      → Tesseract 5 ONLY (no cloud transmission)                  │       │
│   │                                                                  │       │
│   │   Privacy Mode = FALSE (default)?                                │       │
│   │      → Tesseract 5 (primary)                                     │       │
│   │          → If confidence < 75% → AWS Textract                    │       │
│   │              → If still < 75% → Google Vision                    │       │
│   │                  → If still < 70% → Error Queue                  │       │
│   └─────────────────────────────────────────────────────────────────┘       │
│         │                                                                    │
│         ▼                                                                    │
│   ┌───────────┐                                                              │
│   │  EXTRACT  │  Header Fields:                                             │
│   │           │   • vendor_name (+ normalized)                               │
│   │           │   • invoice_number                                           │
│   │           │   • invoice_date, due_date                                   │
│   │           │   • total_amount, currency                                   │
│   │           │   • tax_amount, subtotal                                     │
│   │           │                                                              │
│   │           │  Line Items (Phase 2):                                       │
│   │           │   • description, quantity, unit_price, amount                │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌───────────┐                                                              │
│   │ VALIDATE  │  • Required fields present?                                  │
│   │           │  • Date/amount format valid?                                 │
│   │           │  • Duplicate check (invoice# + vendor hash)                  │
│   │           │  • Vendor fuzzy match against master list                   │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────────┐       │
│   │                    CONFIDENCE ROUTER                             │       │
│   │                                                                  │       │
│   │   ≥85% Confidence ─────────────────────→ AP QUEUE               │       │
│   │                                          (auto-route to workflow)│       │
│   │                                                                  │       │
│   │   70-84% Confidence ───────────────────→ REVIEW QUEUE           │       │
│   │                                          (human verifies fields) │       │
│   │                                                                  │       │
│   │   <70% Confidence ─────────────────────→ ERROR QUEUE            │       │
│   │                                          (manual entry required) │       │
│   └─────────────────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | CEO preference, async-first, Tower middleware |
| Async Runtime | **Tokio** | 1.x | Industry standard for Rust async |
| Serialization | **Serde** | Latest | De facto Rust standard |
| Database | **SQLx** | 0.7+ | Compile-time query checking |
| Migrations | **sqlx-cli** | 0.7+ | Integrated with SQLx |
| Validation | **validator** | Latest | Derive macros for request validation |
| Errors | **thiserror** | Latest | Typed error definitions |
| Logging | **tracing** | Latest | Structured logging with spans |
| Config | **config-rs** | Latest | Multi-source configuration |
| HTTP Client | **reqwest** | Latest | OCR provider API calls |
| UUID | **uuid** | Latest | Entity identifiers |
| Date/Time | **chrono** | Latest | Timestamps, due dates |
| Password | **argon2** | Latest | Secure password hashing |
| JWT | **jsonwebtoken** | Latest | Token authentication |
| Testing | **tokio-test** | Latest | Async test utilities |
| Mocking | **wiremock** | Latest | HTTP mocking for tests |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | CEO preference, App Router, RSC |
| Language | **TypeScript** | 5.x | Strict mode enabled |
| Styling | **Tailwind CSS** | 3.x | CEO preference, utility-first |
| Components | **shadcn/ui** | Latest | CEO preference, accessible |
| Server State | **TanStack Query** | 5.x | Caching, optimistic updates |
| Forms | **React Hook Form** | Latest | Performance, integration |
| Validation | **Zod** | Latest | Type-safe schema validation |
| Tables | **TanStack Table** | 8.x | Invoice lists, data grids |
| Charts | **Recharts** | 2.x | Analytics dashboards |
| Date Picker | **react-day-picker** | Latest | Invoice date handling |
| Notifications | **Sonner** | Latest | Toast notifications |
| Icons | **Lucide React** | Latest | Consistent iconography |
| Auth | **next-auth** | 4.x | Session management |
| API Client | **Generated** | - | Type-safe from OpenAPI spec |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB support |
| Analytics DB | **DuckDB** | Embedded, fast aggregations |
| Document Storage | **MinIO** | S3-compatible, local-first development |
| Cache / Queue | **Redis 7** | Sessions, rate limiting, job queues |
| Search | **PostgreSQL FTS** | Start simple, add Meilisearch if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost per Page |
|----------|----------|----------|---------------|
| **Tesseract 5** | Primary | Local processing, privacy-first | Free |
| **AWS Textract** | Secondary | Complex layouts, tables | ~$0.01 |
| **Google Vision** | Tertiary | Handwriting, edge cases | ~$0.0015 |

### 2.5 Infrastructure

| Component | Technology | Environment |
|-----------|------------|-------------|
| Containers | Docker | All |
| Dev Orchestration | Docker Compose | Development |
| Prod Orchestration | Kubernetes (EKS) | Production |
| CI/CD | GitHub Actions | All |
| Secrets | HashiCorp Vault | Production |
| Monitoring | Prometheus + Grafana | All |
| Tracing | OpenTelemetry + Jaeger | All |
| Email | AWS SES | Production |
| CDN | CloudFront | Production |

---

## 3. Development Phases

### Timeline Overview

```
Week    1    2    3    4    5    6    7    8    9   10   11   12
        ├────┼────┼────┼────┼────┼────┼────┼────┼────┼────┼────┤
        │    │              │              │              │
        │ P0 │      P1      │      P2      │     P3       │
        │FOUN│   INVOICE    │   INVOICE    │   PILOT      │
        │DATI│   CAPTURE    │  PROCESSING  │   LAUNCH     │
        │ ON │              │              │              │

        M1: Auth       M2: OCR          M3: Workflow    M4: 5 Pilots
            Complete       Working          Working         Live
```

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Project structure, infrastructure, authentication

#### Week 1: Infrastructure Setup

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Create Bill Forge monorepo | DevOps | Cargo.toml workspace + pnpm-workspace.yaml | P0 |
| Docker Compose setup | DevOps | PostgreSQL, Redis, MinIO running locally | P0 |
| CI/CD pipeline | DevOps | GitHub Actions: lint, test, build on PR | P0 |
| Control plane schema | Backend | tenants, users, api_keys tables | P0 |
| Tenant service | Backend | bf-tenant crate: create/list/get tenants | P0 |
| SQLx migrations setup | Backend | Migration infrastructure working | P0 |

#### Week 2: Auth + API Foundation

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| JWT authentication | Backend | bf-auth crate: issue/verify tokens | P0 |
| API gateway | Backend | bf-api crate with health endpoint | P0 |
| Tenant resolution middleware | Backend | Extract tenant from URL path/header | P0 |
| Next.js scaffold | Frontend | App with shadcn/ui, login page | P0 |
| API client generation | Frontend | OpenAPI to TypeScript client | P1 |
| Seed data/fixtures | Full-stack | Development test data | P1 |

**Exit Criteria:**
- [ ] Monorepo with crates/ and apps/web/
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] GET /health returns 200
- [ ] bf-tenant can create tenant databases
- [ ] bf-auth issues and verifies JWTs
- [ ] Next.js app renders login page
- [ ] CI pipeline green on main

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with confidence-based routing

#### Weeks 3-4: OCR Pipeline

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Document upload API | Backend | POST /api/v1/{tenant}/invoices/upload | P0 |
| S3 storage abstraction | Backend | bf-storage crate (MinIO/S3) | P0 |
| Tesseract integration | Backend | bf-ocr crate with local OCR | P0 |
| Field extraction | Backend | vendor, invoice #, amount, date, currency | P0 |
| Confidence scoring | Backend | Per-field and overall confidence | P0 |
| Queue data models | Backend | invoices, invoice_queue tables | P0 |

#### Weeks 5-6: Capture UI + Vendor Matching

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Invoice upload UI | Frontend | Drag-drop, file preview, progress | P0 |
| OCR results display | Frontend | Confidence badges per field | P0 |
| Manual correction UI | Frontend | Inline edit with visual highlighting | P0 |
| Vendor fuzzy matching | Backend | Levenshtein distance matching | P0 |
| Vendor CRUD API | Backend | GET/POST/PATCH /vendors | P0 |
| Queue dashboard | Frontend | AP queue, review queue, error queue | P0 |

**Exit Criteria:**
- [ ] POST /api/v1/{tenant}/invoices/upload accepts PDF/images
- [ ] GET /api/v1/{tenant}/invoices/{id} returns extracted data
- [ ] GET /api/v1/{tenant}/queues/ap lists high-confidence invoices
- [ ] GET /api/v1/{tenant}/queues/errors lists low-confidence invoices
- [ ] OCR extracts: vendor_name, invoice_number, amount, date, currency
- [ ] Confidence routing: ≥85% → AP, 70-84% → review, <70% → error
- [ ] Manual correction updates invoice data
- [ ] Vendor matching suggests existing vendors

**Success Metrics:**
- OCR accuracy: ≥85% on clean PDF test set
- Processing time: <3 seconds per invoice (P95)
- Manual correction: <30 seconds to fix one field

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Objective:** Approval workflows with email actions (key differentiator)

#### Weeks 7-8: Workflow Engine

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Workflow rule engine | Backend | bf-workflow crate with JSON rules | P0 |
| Approval state machine | Backend | States: pending, l1_wait, approved, etc. | P0 |
| Rule configuration API | Backend | GET/POST /workflows | P0 |
| Approval inbox UI | Frontend | Pending items with bulk select | P0 |
| Approve/reject/hold actions | Full-stack | Action buttons + API endpoints | P0 |

#### Weeks 9-10: Email Actions + Audit

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Signed token generation | Backend | HMAC tokens with 72h expiration | P0 |
| Email approval endpoints | Backend | GET /api/v1/actions/{token}/approve | P0 |
| Email service (SES) | Backend | Notifications for pending approvals | P0 |
| Delegation config | Full-stack | Out-of-office routing | P1 |
| SLA tracking | Backend | Time-in-queue calculation | P1 |
| Audit trail | Backend | All actions logged with actor, IP | P0 |
| Bulk operations | Frontend | Batch approve/reject | P1 |

**Exit Criteria:**
- [ ] POST /api/v1/{tenant}/workflows creates approval rules
- [ ] POST /api/v1/{tenant}/invoices/{id}/approve works
- [ ] GET /api/v1/actions/{token}/approve works without authentication
- [ ] Email notifications sent on pending approval
- [ ] Delegation: users can set out-of-office routing
- [ ] SLA dashboard shows queue times
- [ ] Audit log captures: actor, action, timestamp, IP

**Success Metrics:**
- Approval action latency: <5 seconds (P95)
- Email approval success rate: ≥95%
- Audit coverage: 100% of state changes logged

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Production deployment with 5 pilot customers

#### Week 11: Production Readiness

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Production environment | DevOps | Kubernetes deployment on EKS | P0 |
| Security audit | Security | Penetration testing, SAST/DAST | P0 |
| Load testing | QA | 100 invoices/minute sustained | P0 |
| Monitoring + alerting | DevOps | Prometheus/Grafana dashboards | P0 |
| API documentation | Backend | OpenAPI spec published | P1 |
| User guides | Product | Help documentation | P1 |

#### Week 12: Customer Onboarding

| Task | Owner | Deliverable | Priority |
|------|-------|-------------|----------|
| Data migration tooling | Backend | Import from CSV | P0 |
| White-glove onboarding | Product | Personal setup for each pilot | P0 |
| Feedback mechanisms | Product | In-app feedback, weekly calls | P0 |
| Bug triage process | Engineering | P0/P1/P2 classification | P0 |
| Hotfix process | DevOps | Emergency deploy pipeline | P0 |

**Exit Criteria:**
- [ ] Production deployment live
- [ ] Security audit passed (no critical/high vulnerabilities)
- [ ] Load test: 100 invoices/minute for 1 hour
- [ ] 5 pilot customers actively using platform
- [ ] API docs published
- [ ] Support runbook covers top 20 scenarios

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy <85%** | Medium | High | Multi-provider fallback; human review loop; collect training data from corrections |
| **Rust learning curve** | Medium | Medium | Pair programming; code reviews; detailed examples; consider Go for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; RLS as defense-in-depth layer |
| **Email token security** | Medium | High | HMAC with 72h expiration; one-time use; rate limiting; IP audit logging |
| **DuckDB scalability** | Medium | Medium | Partition by month; archive >12 months; evaluate ClickHouse if needed |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant pools with limits; lazy connections; alerting on threshold |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep** | High | High | Strict anti-goals; weekly scope review; "Phase 2" is the answer |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack channel |
| **ERP integration complexity** | High | Medium | Start with QuickBooks (simplest API); use official SDK |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ; health checks; auto-failover; <1h MTTR target |
| **Key person dependency** | High | High | ADRs; pair programming; knowledge sharing sessions |
| **Security incident** | Low | Critical | Penetration testing; incident response plan; consider bug bounty |

### 4.4 Risk Priority Matrix

```
                        IMPACT
                    Low     Medium    High      Critical
              ┌─────────┬─────────┬─────────┬─────────┐
         High │         │ Feature │         │         │
              │         │ creep   │         │         │
              ├─────────┼─────────┼─────────┼─────────┤
  P    Medium │         │ Rust    │ OCR     │         │
  R           │         │ curve   │ accuracy│         │
  O           │         │ DuckDB  │ Churn   │         │
  B           │         │ Pool    │ Email   │         │
              ├─────────┼─────────┼─────────┼─────────┤
  A     Low   │         │         │         │ Data    │
  B           │         │         │         │ loss    │
  I           │         │         │         │ Tenant  │
  L           │         │         │         │ breach  │
  I           │         │         │         │ Security│
  T           │         │         │         │ incident│
  Y           └─────────┴─────────┴─────────┴─────────┘
```

---

## 5. Resource Requirements

### 5.1 Team Structure

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           BILL FORGE TEAM (5.5 FTE)                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ENGINEERING (4.5 FTE)                                                     │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                                                                      │   │
│   │   Backend Engineer (Rust) - 2 FTE                                   │   │
│   │   • bf-api, bf-invoice, bf-workflow, bf-ocr crates                  │   │
│   │   • Database schema design and queries                              │   │
│   │   • OCR pipeline and accuracy optimization                          │   │
│   │   • Approval workflow engine                                        │   │
│   │                                                                      │   │
│   │   Frontend Engineer (Next.js/TypeScript) - 1 FTE                    │   │
│   │   • Invoice capture UI (upload, preview, correction)                │   │
│   │   • Approval inbox and workflow UI                                  │   │
│   │   • Dashboard and analytics views                                   │   │
│   │   • Component library (shadcn/ui customization)                     │   │
│   │                                                                      │   │
│   │   Full-Stack / DevOps Engineer - 1 FTE                              │   │
│   │   • CI/CD pipeline, Docker, Kubernetes                              │   │
│   │   • Monitoring, alerting, observability                             │   │
│   │   • Integration work between frontend and backend                   │   │
│   │   • Security hardening                                              │   │
│   │                                                                      │   │
│   │   ML/AI Engineer (Contract) - 0.5 FTE                               │   │
│   │   • OCR accuracy tuning and provider selection                      │   │
│   │   • Field extraction optimization                                   │   │
│   │   • Winston AI adaptation (Phase 3+)                                │   │
│   │                                                                      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│   PRODUCT (1 FTE)                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │   Product Manager - 1 FTE                                           │   │
│   │   • Pilot customer relationships and onboarding                     │   │
│   │   • Feature prioritization and roadmap                              │   │
│   │   • User research and feedback synthesis                            │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│   TOTAL: 5.5 FTE for 12-week MVP                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Hiring Priorities

| Role | Priority | Start Week | Key Skills |
|------|----------|------------|------------|
| Backend Engineer (Rust) #1 | P0 | Week 1 | Rust, Axum, PostgreSQL, async |
| Backend Engineer (Rust) #2 | P0 | Week 1 | Rust, API design, SQLx |
| DevOps Engineer | P0 | Week 1 | Docker, Kubernetes, GitHub Actions |
| Frontend Engineer | P0 | Week 2 | Next.js 14+, TypeScript, Tailwind |
| Product Manager | P1 | Week 1 | B2B SaaS, customer development |
| ML/AI Contractor | P2 | Week 3 | OCR, document processing |

### 5.3 Infrastructure Costs (Monthly)

| Component | Development | Production (5 Pilots) |
|-----------|------------:|----------------------:|
| Cloud Compute (EKS) | $200 | $800 |
| PostgreSQL (RDS) | $50 | $300 |
| Redis (ElastiCache) | $20 | $100 |
| S3/MinIO Storage | $10 | $50 |
| OCR (Textract backup) | $0 | $200 |
| Email (SES) | $0 | $50 |
| Monitoring (Grafana) | $0 | $100 |
| Domain + SSL | $10 | $10 |
| **TOTAL** | **$290/mo** | **$1,610/mo** |

---

## 6. Monorepo Structure

```
bill-forge/
├── Cargo.toml                      # Rust workspace root
├── Cargo.lock
├── package.json                    # pnpm workspace root
├── pnpm-workspace.yaml
├── docker-compose.yml              # Local development
├── .env.example
├── README.md
│
├── .github/
│   └── workflows/
│       ├── ci.yml                  # PR: lint, test, build
│       ├── deploy-staging.yml
│       └── deploy-prod.yml
│
├── crates/                         # Rust backend crates
│   ├── bf-api/                     # API gateway (Axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── routes/
│   │       ├── middleware/
│   │       └── error.rs
│   │
│   ├── bf-invoice/                 # Invoice capture service
│   ├── bf-workflow/                # Approval workflow engine
│   ├── bf-ocr/                     # OCR provider abstraction
│   ├── bf-vendor/                  # Vendor management
│   ├── bf-storage/                 # S3/MinIO abstraction
│   ├── bf-auth/                    # Authentication
│   ├── bf-tenant/                  # Tenant management
│   ├── bf-analytics/               # DuckDB analytics
│   └── bf-common/                  # Shared types, utilities
│
├── apps/                           # Frontend applications
│   └── web/                        # Next.js main app
│       ├── package.json
│       ├── next.config.mjs
│       ├── tailwind.config.ts
│       └── src/
│           ├── app/                # App Router pages
│           ├── components/         # UI components
│           └── lib/                # Utilities
│
├── packages/                       # Shared JS packages
│   ├── ui/                         # Extended shadcn/ui
│   └── api-client/                 # Generated TypeScript client
│
├── services/                       # Additional services
│   └── winston/                    # AI assistant (Phase 3)
│       ├── pyproject.toml          # Python (adapted from Locust)
│       └── src/winston/
│
├── migrations/                     # Database migrations
│   ├── control-plane/
│   └── tenant/
│
├── infra/                          # Infrastructure as code
│   ├── terraform/
│   └── kubernetes/
│
├── docs/                           # Documentation
│   ├── api/                        # OpenAPI specs
│   ├── architecture/               # ADRs
│   └── runbooks/
│
└── tests/                          # End-to-end tests
    ├── e2e/
    └── load/
```

---

## 7. Success Metrics

### 7.1 Technical KPIs (12-Week Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **OCR Accuracy** | ≥85% | Correct fields / Total fields |
| **OCR Accuracy (clean PDFs)** | ≥90% | Well-formatted digital PDFs |
| **Processing Latency (P95)** | <5 sec | Upload to queue placement |
| **API Response Time (P95)** | <200ms | Non-OCR endpoints |
| **System Uptime** | ≥99.5% | Monthly availability |
| **Test Coverage** | ≥80% | Line coverage on core crates |
| **Critical Bugs** | 0 | Unresolved P0 in production |
| **Security Vulnerabilities** | 0 Critical/High | SAST/DAST results |

### 7.2 Business KPIs (12-Week Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 3,000+ | Total across pilots |
| **Net Promoter Score** | ≥50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | ≥60% | Conversion conversations |
| **Email Approval Adoption** | ≥50% | Email / Total approvals |

### 7.3 Operational KPIs

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deployment Frequency** | Daily | Deploys to staging |
| **Mean Time to Recovery** | <1 hour | Incident to resolution |
| **Change Failure Rate** | <15% | Deploys requiring rollback |

---

## 8. Answers to CEO's Strategic Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**Strengths:**
- 20+ years of ERP integration experience (SAP, Oracle)
- Mature workflow engine for complex scenarios
- Established Nordic/European customer base

**Weaknesses (Our Opportunities):**
- UI described as "slow" and "clunky" in customer reviews
- Limited AI/ML innovation in recent years
- Opaque "call for quote" pricing model
- Poor mobile experience
- Slow, impersonal support

**Bill Forge Differentiation:**

| Dimension | Palette | Bill Forge |
|-----------|---------|------------|
| UI Speed | Multi-second loads | Sub-second |
| Setup Time | Weeks/months | Hours/days |
| Pricing | "Call for quote" | Published |
| OCR | Cloud-only | Local-first option |
| Approvals | Login required | Email (no login) |

### Q2: What's the ideal OCR accuracy threshold?

**Three-tier confidence routing:**

| Confidence | Routing | Action |
|------------|---------|--------|
| ≥85% | AP Queue | Auto-route to workflow |
| 70-84% | Review Queue | Human verifies flagged fields |
| <70% | Error Queue | Full manual entry |

### Q3: Which ERP integration should we prioritize?

**Recommendation: QuickBooks Online (Phase 2)**

| ERP | Priority | Rationale | Timeline |
|-----|----------|-----------|----------|
| **QuickBooks Online** | 1 | Largest mid-market share, simple REST API | 2-3 weeks |
| NetSuite | 2 | Growing companies, SuiteScript | 4-6 weeks |
| Sage Intacct | 3 | Manufacturing vertical | 4-6 weeks |

### Q4: Common approval workflow patterns?

| Pattern | Adoption | MVP Support |
|---------|----------|-------------|
| **Amount-based thresholds** | 85% | Required |
| Exception-only routing | 65% | Partial |
| Department/cost center | 45% | Phase 2 |
| Dual approval | 30% | Phase 3 |

### Q5: How to handle multi-currency?

**MVP Approach:**
- Extract currency from invoice, store as metadata
- Display original currency, convert for totals using daily rates
- Send base currency to ERP; ERP handles GL rates
- Full multi-currency GL posting deferred to Phase 3

### Q6: What pricing model resonates?

**Tiered Usage-Based Pricing (No Seat Tax):**

| Tier | Monthly | Invoices | Overage |
|------|---------|----------|---------|
| Starter | $299 | 500 | $0.75 |
| Growth | $799 | 2,000 | $0.50 |
| Scale | $1,999 | 10,000 | $0.30 |
| Enterprise | Custom | Custom | Custom |

---

## 9. Winston AI Strategy (Leveraging Locust)

The existing Locust framework at `/src/locust/` provides a foundation for Winston AI.

### What to Reuse

| Locust Component | Winston Adaptation |
|------------------|-------------------|
| Agent base classes | Simplify for single-agent |
| LLM backend switching | Keep Claude + Ollama support |
| Workflow state | Adapt for query/action patterns |
| Memory/embeddings | Use for semantic search |
| Checkpoint/resume | Conversation recovery |

### Winston Tools (Phase 3)

```python
@tool
async def search_invoices(query: str, tenant_id: str) -> list[Invoice]:
    """Search invoices by vendor name, amount, or status."""
    pass

@tool
async def list_pending_approvals(user_id: str, tenant_id: str) -> list[PendingApproval]:
    """List all invoices pending the user's approval."""
    pass

@tool
async def vendor_lookup(search: str, tenant_id: str) -> list[Vendor]:
    """Search vendors by name or tax ID."""
    pass

@tool
async def run_report(report_type: str, date_range: DateRange, tenant_id: str) -> Report:
    """Run a spending or processing report."""
    pass
```

### Timeline: ~3 weeks post-MVP

| Week | Focus |
|------|-------|
| 1 | Fork Locust agent core, strip unused modules |
| 2 | Implement Bill Forge tools, API integration |
| 3 | Chat UI, testing, tenant isolation |

**Effort savings:** ~60% reduction vs building from scratch

---

## 10. Immediate Next Steps

### Week 0 (This Week)

**Day 1-2: Repository Setup**
- [ ] Create `bill-forge` repository
- [ ] Initialize Cargo workspace with bf-common, bf-api
- [ ] Initialize pnpm workspace with Next.js app
- [ ] Configure Docker Compose (PostgreSQL, Redis, MinIO)

**Day 3-4: CI/CD Pipeline**
- [ ] GitHub Actions: Rust lint (clippy), test, build
- [ ] GitHub Actions: TypeScript lint, build
- [ ] Pre-commit hooks: format, lint

**Day 5: Foundation Crates**
- [ ] bf-common: UUID types, config, error types
- [ ] bf-api: Axum scaffold with health endpoint
- [ ] bf-tenant: Tenant model, control plane schema

### Week 1 Checklist

| Deliverable | Owner | Done |
|-------------|-------|------|
| Monorepo initialized | DevOps | [ ] |
| Docker Compose running | DevOps | [ ] |
| bf-api health check working | Backend | [ ] |
| bf-tenant creates tenant databases | Backend | [ ] |
| CI pipeline passing on main | DevOps | [ ] |
| Next.js app with shadcn/ui | Frontend | [ ] |

---

## Appendices

### Appendix A: Architecture Decision Records

**ADR-001: Database-per-Tenant**
- Status: Accepted
- Decision: Separate database per tenant (not RLS)
- Trade-offs: Higher connection overhead vs complete isolation

**ADR-002: OCR Provider Strategy**
- Status: Accepted
- Decision: Tesseract default, cloud fallback
- Trade-offs: Lower cost vs slightly lower accuracy

**ADR-003: Email Approval Security**
- Status: Accepted
- Decision: HMAC-signed tokens, 72h expiration, one-time use
- Trade-offs: Frictionless UX vs token forwarding risk (mitigated by audit)

### Appendix B: Technical Debt Prevention

1. **Mandatory code review** for all PRs
2. **80% test coverage** target for core paths
3. **ADRs** for all architectural decisions
4. **Weekly debt review** - 20% time for cleanup
5. **No "temporary" hacks** without tracking ticket

---

**Document Status:**

- [x] CTO Review Complete
- [ ] CEO Alignment Review
- [ ] Engineering Lead Review
- [ ] Product Manager Review

**Next Actions:**
1. Share with engineering team for feedback
2. Create Jira/Linear project with Phase 0 tasks
3. Set up weekly progress review meetings
4. Begin repository setup

---

*This execution plan is derived from the CTO Strategic Plan v3.0 and CPO Product Strategy v4.0. It provides the operational blueprint for Bill Forge implementation.*
