# Bill Forge: CTO Strategic Technical Plan

**Version:** 3.0
**Date:** February 1, 2026
**Author:** Chief Technology Officer
**Status:** Final - Ready for Execution
**Planning Horizon:** 12 Weeks (Q1 2026)

---

## Executive Summary

Bill Forge is a modular B2B SaaS platform for mid-market companies (50-500 employees) frustrated with either overbuilt enterprise AP tools or underpowered SMB solutions. This plan provides the technical blueprint for our 12-week MVP sprint.

### Current State Assessment

The existing `locust` codebase is a Python-based multi-agent AI orchestration framework. While sophisticated, it serves a different purpose than Bill Forge's core invoice processing needs. The relationship between the two:

| Aspect | Current Locust | Bill Forge Target | Decision |
|--------|---------------|-------------------|----------|
| **Purpose** | AI Agent Orchestration | Invoice Processing | Separate codebases |
| **Backend Language** | Python (LangGraph) | Rust (Axum) | New codebase for Bill Forge |
| **Frontend** | None | Next.js 14+ | Build from scratch |
| **OLTP Database** | SQLite | PostgreSQL | Per-tenant PostgreSQL |
| **AI Component** | Full agent system | Winston AI Assistant | Adapt Locust for Winston (Phase 3) |

### Strategic Technical Decision: Dual Codebase Approach

```
bill-forge/                    # NEW: Core product (Rust + Next.js)
├── crates/                    # Rust backend modules
├── apps/web/                  # Next.js frontend
└── services/winston/          # Adapted from Locust (Phase 3)

locust/                        # EXISTING: AI framework (Python)
└── src/locust/               # Will be adapted for Winston AI
```

**Rationale:**
1. **Performance**: Rust provides the sub-second response times our UX demands
2. **Type Safety**: Compile-time guarantees for financial data integrity
3. **CEO Preference**: Aligns with stated technology choices
4. **AI Reuse**: Locust's agent architecture will power Winston (Phase 3)

---

## 1. Technical Architecture Recommendations

### 1.1 High-Level System Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                           BILL FORGE PLATFORM                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                        NEXT.JS 14+ FRONTEND                              │ │
│  │                                                                          │ │
│  │  /invoices          /approvals        /vendors          /reports        │ │
│  │  ┌──────────┐      ┌──────────┐      ┌──────────┐      ┌──────────┐    │ │
│  │  │ Upload   │      │  Inbox   │      │  Master  │      │Dashboard │    │ │
│  │  │ Preview  │      │ Actions  │      │   Data   │      │ Charts   │    │ │
│  │  │ Correct  │      │   SLA    │      │ Tax Docs │      │ Export   │    │ │
│  │  └──────────┘      └──────────┘      └──────────┘      └──────────┘    │ │
│  │                                                                          │ │
│  │  Tech: shadcn/ui • Tailwind CSS • TanStack Query • React Hook Form      │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                      │
│                                        ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         RUST API GATEWAY (bf-api)                        │ │
│  │                                                                          │ │
│  │  Axum 0.7+ │ JWT Auth │ Tenant Resolution │ Rate Limiting │ CORS        │ │
│  │                                                                          │ │
│  │  Middleware Stack: Request → Log → Auth → Tenant → RateLimit → Handler  │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                      │
│            ┌───────────────────────────┼───────────────────────────┐         │
│            ▼                           ▼                           ▼         │
│     ┌──────────────┐          ┌──────────────┐          ┌──────────────┐    │
│     │  bf-invoice  │          │ bf-workflow  │          │  bf-vendor   │    │
│     ├──────────────┤          ├──────────────┤          ├──────────────┤    │
│     │ Upload API   │          │ Rule Engine  │          │ Master Data  │    │
│     │ OCR Pipeline │          │ State Machine│          │ Tax Storage  │    │
│     │ Extraction   │          │ Notifications│          │ Fuzzy Match  │    │
│     │ Queue Route  │          │ Email Actions│          │ Spend View   │    │
│     └──────────────┘          └──────────────┘          └──────────────┘    │
│            │                           │                           │         │
│            └───────────────────────────┼───────────────────────────┘         │
│                                        ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                     bf-ocr (Provider Abstraction)                        │ │
│  │                                                                          │ │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐            │ │
│  │  │  Tesseract 5   │  │  AWS Textract  │  │ Google Vision  │            │ │
│  │  │  (Local/Free)  │  │  (Cloud/$0.01) │  │  (Fallback)    │            │ │
│  │  │   DEFAULT      │  │   ESCALATION   │  │  HANDWRITING   │            │ │
│  │  └────────────────┘  └────────────────┘  └────────────────┘            │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
├──────────────────────────────────────────────────────────────────────────────┤
│                              DATA LAYER                                       │
│                                                                               │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐           │
│  │  Control Plane   │  │   Tenant DBs     │  │  MinIO (S3)      │           │
│  │   PostgreSQL     │  │   PostgreSQL     │  │                  │           │
│  │                  │  │                  │  │  /tenant-a/      │           │
│  │ • tenants        │  │  bf_tenant_acme: │  │    /invoices/    │           │
│  │ • users          │  │  • invoices      │  │    /tax-docs/    │           │
│  │ • api_keys       │  │  • vendors       │  │                  │           │
│  │ • subscriptions  │  │  • workflows     │  │  /tenant-b/      │           │
│  │                  │  │  • audit_log     │  │    /invoices/    │           │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘           │
│                                                                               │
│  ┌──────────────────┐  ┌──────────────────┐                                 │
│  │  DuckDB          │  │  Redis           │                                 │
│  │  (Per-Tenant)    │  │                  │                                 │
│  │                  │  │ • Sessions       │                                 │
│  │ • metrics        │  │ • Rate limits    │                                 │
│  │ • aggregates     │  │ • Job queues     │                                 │
│  │ • reports        │  │ • Pub/Sub        │                                 │
│  └──────────────────┘  └──────────────────┘                                 │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Database-per-Tenant Architecture

**Decision:** Database-per-tenant (not row-level security)

```
                        ┌────────────────────────────────────┐
                        │          CONTROL PLANE             │
                        │                                    │
                        │  control_plane_db (PostgreSQL)     │
                        │  ┌──────────────────────────────┐  │
                        │  │ tenants                       │  │
                        │  │  • id: uuid                   │  │
                        │  │  • slug: "acme-corp"          │  │
                        │  │  • db_name: "bf_tenant_acme"  │  │
                        │  │  • modules: ["capture","proc"]│  │
                        │  │  • settings: {...}            │  │
                        │  └──────────────────────────────┘  │
                        └────────────────────────────────────┘
                                         │
                    ┌────────────────────┼────────────────────┐
                    ▼                    ▼                    ▼
            ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
            │bf_tenant_acme│     │bf_tenant_tech│     │bf_tenant_mfg │
            ├──────────────┤     ├──────────────┤     ├──────────────┤
            │ • invoices   │     │ • invoices   │     │ • invoices   │
            │ • vendors    │     │ • vendors    │     │ • vendors    │
            │ • workflows  │     │ • workflows  │     │ • workflows  │
            │ • audit_log  │     │ • audit_log  │     │ • audit_log  │
            └──────────────┘     └──────────────┘     └──────────────┘
```

**Rationale:**
- Complete data isolation (regulatory compliance for healthcare, legal, finance)
- Per-tenant backup/restore capability
- Easy data portability (customer can export their database)
- No cross-tenant query risk
- **Trade-off:** Higher connection overhead (mitigated by per-tenant connection pools)

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
│   │           │  • Document classification (invoice vs. other)              │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────┐           │
│   │                    PROVIDER ROUTER                           │           │
│   │                                                              │           │
│   │   Tenant Privacy Mode = TRUE?                                │           │
│   │      → Tesseract 5 ONLY (no cloud)                          │           │
│   │                                                              │           │
│   │   Tenant Privacy Mode = FALSE (default)?                     │           │
│   │      → Tesseract 5 (primary)                                 │           │
│   │          → If confidence < 75% → AWS Textract                │           │
│   │              → If still < 75% → Google Vision                │           │
│   │                  → If still < 70% → Error Queue              │           │
│   └─────────────────────────────────────────────────────────────┘           │
│         │                                                                    │
│         ▼                                                                    │
│   ┌───────────┐                                                              │
│   │  EXTRACT  │  Header Fields:                                             │
│   │           │   • vendor_name (+ normalized form)                          │
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
│   │ VALIDATE  │  • Required fields present?                                 │
│   │           │  • Date/amount format valid?                                 │
│   │           │  • Duplicate check (invoice# + vendor hash)                 │
│   │           │  • Vendor fuzzy match against master list                   │
│   └─────┬─────┘                                                              │
│         │                                                                    │
│         ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────┐           │
│   │                  CONFIDENCE ROUTER                           │           │
│   │                                                              │           │
│   │   ≥85% Confidence ─────────────────────→ AP QUEUE           │           │
│   │                                          (auto-route)        │           │
│   │                                                              │           │
│   │   70-84% Confidence ───────────────────→ REVIEW QUEUE       │           │
│   │                                          (human verifies)    │           │
│   │                                                              │           │
│   │   <70% Confidence ─────────────────────→ ERROR QUEUE        │           │
│   │                                          (manual entry)      │           │
│   └─────────────────────────────────────────────────────────────┘           │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 1.4 Approval Workflow Engine

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        APPROVAL WORKFLOW ENGINE                               │
│                                                                               │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         RULE ENGINE                                  │   │
│   │                                                                      │   │
│   │   Rules stored as JSON, evaluated by Rust expression engine          │   │
│   │                                                                      │   │
│   │   {                                                                  │   │
│   │     "name": "Standard Amount Tiers",                                 │   │
│   │     "priority": 1,                                                   │   │
│   │     "conditions": [                                                  │   │
│   │       { "if": "amount < 5000", "action": "auto_approve" },           │   │
│   │       { "if": "amount >= 5000 && amount < 25000",                    │   │
│   │         "action": { "route_to": "manager", "level": 1 } },           │   │
│   │       { "if": "amount >= 25000 && amount < 50000",                   │   │
│   │         "action": { "route_to": "director", "level": 2 } },          │   │
│   │       { "if": "amount >= 50000",                                     │   │
│   │         "action": { "route_to": "cfo", "level": 3 } }                │   │
│   │     ],                                                               │   │
│   │     "exceptions": [                                                  │   │
│   │       { "if": "vendor.is_new", "action": "add_review" },             │   │
│   │       { "if": "po_mismatch", "action": "exception_queue" }           │   │
│   │     ]                                                                │   │
│   │   }                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                       │                                      │
│                                       ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                        STATE MACHINE                                 │   │
│   │                                                                      │   │
│   │                        ┌───────────┐                                 │   │
│   │                        │  PENDING  │                                 │   │
│   │                        └─────┬─────┘                                 │   │
│   │                              │                                       │   │
│   │           ┌──────────────────┼──────────────────┐                   │   │
│   │           ▼                  ▼                  ▼                   │   │
│   │     ┌───────────┐     ┌───────────┐     ┌───────────┐              │   │
│   │     │ L1_WAIT   │────→│ L2_WAIT   │────→│ L3_WAIT   │              │   │
│   │     │ (Manager) │     │ (Director)│     │   (CFO)   │              │   │
│   │     └─────┬─────┘     └─────┬─────┘     └─────┬─────┘              │   │
│   │           │                 │                 │                     │   │
│   │           ▼                 ▼                 ▼                     │   │
│   │     ┌─────────────────────────────────────────────────┐            │   │
│   │     │               TERMINAL STATES                    │            │   │
│   │     │  ┌──────────┐ ┌──────────┐ ┌──────────┐        │            │   │
│   │     │  │ APPROVED │ │ REJECTED │ │ ON_HOLD  │        │            │   │
│   │     │  └──────────┘ └──────────┘ └──────────┘        │            │   │
│   │     └─────────────────────────────────────────────────┘            │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                       │                                      │
│                                       ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │            EMAIL APPROVAL (Key Differentiator)                       │   │
│   │                                                                      │   │
│   │   From: notifications@billforge.io                                   │   │
│   │   Subject: [Action Required] Invoice #INV-2024-001                   │   │
│   │                                                                      │   │
│   │   Vendor: Acme Corporation                                           │   │
│   │   Amount: $12,500.00 USD                                             │   │
│   │   Due Date: February 15, 2026                                        │   │
│   │                                                                      │   │
│   │   [  APPROVE  ]    [  REJECT  ]    [  VIEW  ]                       │   │
│   │                                                                      │   │
│   │   Security:                                                          │   │
│   │   • Links are HMAC-signed (SHA-256)                                  │   │
│   │   • Tokens expire in 72 hours                                        │   │
│   │   • No login required to approve or reject                           │   │
│   │   • One-time use (invalidated after action)                          │   │
│   │   • IP logging for complete audit trail                              │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | CEO preference, async-first, Tower middleware |
| Async Runtime | **Tokio** | 1.x | Industry standard, required by Axum |
| Serialization | **Serde** | Latest | De facto Rust standard |
| Database | **SQLx** | 0.7+ | Compile-time query checking, async-native |
| Migrations | **sqlx-cli** | 0.7+ | Integrated with SQLx |
| Validation | **validator** | Latest | Derive macros for request validation |
| Error Handling | **thiserror** | Latest | Typed error definitions |
| Logging | **tracing** | Latest | Structured logging with spans |
| Config | **config-rs** | Latest | Multi-source configuration |
| HTTP Client | **reqwest** | Latest | OCR provider calls |
| UUID | **uuid** | Latest | Entity identifiers |
| Date/Time | **chrono** | Latest | Timestamps |
| Password | **argon2** | Latest | Secure password hashing |
| JWT | **jsonwebtoken** | Latest | Token authentication |
| Testing | **tokio-test, wiremock** | Latest | Async tests, HTTP mocking |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | CEO preference, App Router, RSC |
| Language | **TypeScript** | 5.x | Strict mode enabled |
| Styling | **Tailwind CSS** | 3.x | CEO preference, utility-first |
| Components | **shadcn/ui** | Latest | CEO preference, accessible |
| Server State | **TanStack Query** | 5.x | Caching, optimistic updates |
| Forms | **React Hook Form + Zod** | Latest | Type-safe validation |
| Tables | **TanStack Table** | 8.x | Invoice lists, data grids |
| Charts | **Recharts** | 2.x | Analytics dashboards |
| Date Picker | **date-fns + react-day-picker** | Latest | Invoice date handling |
| Notifications | **Sonner** | Latest | Toast notifications |
| Icons | **Lucide React** | Latest | Consistent iconography |
| Auth Client | **next-auth** | 4.x | Session management |
| API Client | Generated from **OpenAPI** | - | Type-safe API calls |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB support |
| Analytics DB | **DuckDB** | Embedded, fast aggregations |
| Document Storage | **MinIO** | S3-compatible, local-first development |
| Cache/Queue | **Redis 7** | Sessions, rate limiting, job queues |
| Search | **PostgreSQL FTS + pg_trgm** | Start simple, add Meilisearch if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost per Page |
|----------|----------|----------|---------------|
| **Tesseract 5** | Primary | Local, privacy-first, standard invoices | Free |
| **AWS Textract** | Secondary | Complex layouts, tables | ~$0.01 |
| **Google Vision** | Tertiary | Fallback, handwritten content | ~$0.0015 |

**Strategy:** Default to Tesseract. Escalate to cloud only when confidence < 75% and tenant allows cloud OCR.

### 2.5 Infrastructure

| Component | Technology | Environment |
|-----------|------------|-------------|
| Containers | **Docker** | All |
| Dev Orchestration | **Docker Compose** | Development |
| Prod Orchestration | **Kubernetes (EKS)** | Production |
| CI/CD | **GitHub Actions** | All |
| Secrets | **HashiCorp Vault** | Production |
| Monitoring | **Prometheus + Grafana** | All |
| Tracing | **OpenTelemetry + Jaeger** | All |
| Email | **AWS SES** | Production |
| CDN | **CloudFront** | Production |

---

## 3. Development Priorities and Phases

### 3.1 Timeline Overview

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        12-WEEK MVP TIMELINE                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  Week:   1    2    3    4    5    6    7    8    9   10   11   12            │
│          ├────┼────┼────┼────┼────┼────┼────┼────┼────┼────┼────┤            │
│          │    │         │              │              │         │            │
│          │ P0 │       P1              │      P2      │   P3    │            │
│          │FOUN│   INVOICE CAPTURE     │ INVOICE PROC │  PILOT  │            │
│          │DATI│                       │              │  LAUNCH │            │
│          │ON  │                       │              │         │            │
│                                                                               │
│  Milestones:                                                                  │
│  M1 (W2): Auth + Tenant Complete    M3 (W10): Workflow Functional            │
│  M2 (W6): OCR Pipeline Ready        M4 (W12): 5 Pilots Live                  │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Project structure, infrastructure, authentication

#### Week 1: Infrastructure

| Task | Owner | Deliverable |
|------|-------|-------------|
| Create monorepo structure | DevOps | Cargo.toml workspace + pnpm-workspace.yaml |
| Docker Compose setup | DevOps | PostgreSQL, Redis, MinIO running locally |
| CI/CD pipeline | DevOps | GitHub Actions: lint, test, build on PR |
| Control plane schema | Backend | tenants, users, api_keys tables |
| Tenant service | Backend | bf-tenant crate: create/list tenants |
| SQLx migrations | Backend | Migration infrastructure |

#### Week 2: Auth + API Foundation

| Task | Owner | Deliverable |
|------|-------|-------------|
| JWT authentication | Backend | bf-auth crate: issue/verify tokens |
| API gateway | Backend | bf-api crate with health check |
| Tenant resolution middleware | Backend | Extract tenant from URL path |
| Next.js scaffold | Frontend | App with shadcn/ui, login page |
| API client generation | Frontend | OpenAPI to TypeScript client |
| Seed data/fixtures | Full-stack | Development test data |

**Phase 0 Exit Criteria:**
- [ ] Monorepo with crates/ and apps/web/
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] bf-api health endpoint: GET /health → 200 OK
- [ ] bf-tenant can create tenant databases dynamically
- [ ] bf-auth issues and verifies JWTs
- [ ] Next.js app renders login page
- [ ] CI pipeline passes on every PR

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with confidence-based routing

#### Weeks 3-4: OCR Pipeline

| Task | Owner | Deliverable |
|------|-------|-------------|
| Document upload API | Backend | POST /api/v1/{tenant}/invoices/upload |
| S3 storage abstraction | Backend | bf-storage crate |
| Tesseract integration | Backend | bf-ocr crate with local OCR |
| Field extraction | Backend | Vendor, invoice #, amount, date, currency |
| Confidence scoring | Backend | Per-field and overall confidence |
| Queue data models | Backend | invoices, invoice_queue tables |

#### Weeks 5-6: Capture UI + Vendor Matching

| Task | Owner | Deliverable |
|------|-------|-------------|
| Invoice upload UI | Frontend | Drag-drop, file preview |
| OCR results display | Frontend | Confidence badges per field |
| Manual correction UI | Frontend | Inline edit with visual highlighting |
| Vendor fuzzy matching | Backend | Levenshtein distance matching |
| Vendor CRUD API | Backend | GET/POST/PATCH /vendors |
| Queue dashboard | Frontend | AP queue, review queue, error queue views |

**Phase 1 Exit Criteria:**
- [ ] POST /api/v1/{tenant}/invoices/upload accepts PDF/images
- [ ] GET /api/v1/{tenant}/invoices/{id} returns extracted data
- [ ] GET /api/v1/{tenant}/queues/ap lists high-confidence invoices
- [ ] GET /api/v1/{tenant}/queues/errors lists low-confidence invoices
- [ ] OCR extracts: vendor_name, invoice_number, amount, date, currency
- [ ] Confidence routing: ≥85% → AP, 70-84% → review, <70% → error
- [ ] Manual correction updates invoice data
- [ ] Vendor matching suggests existing vendors

**Success Metrics:**

| Metric | Target |
|--------|--------|
| OCR accuracy (clean PDFs) | ≥85% |
| Processing time | <3 seconds (P95) |
| Manual correction time | <30 seconds per field |

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Objective:** Approval workflows with email actions

#### Weeks 7-8: Workflow Engine

| Task | Owner | Deliverable |
|------|-------|-------------|
| Workflow rule engine | Backend | bf-workflow crate with JSON rules |
| Approval state machine | Backend | States: pending, l1_wait, approved, etc. |
| Rule configuration API | Backend | GET/POST /workflows |
| Approval inbox UI | Frontend | Pending items with bulk select |
| Approve/reject/hold actions | Full-stack | Action buttons + API endpoints |

#### Weeks 9-10: Email Actions + Audit

| Task | Owner | Deliverable |
|------|-------|-------------|
| Signed token generation | Backend | HMAC tokens with 72h expiration |
| Email approval endpoints | Backend | GET /api/v1/actions/{token}/approve |
| Email service integration | Backend | AWS SES for notifications |
| Delegation config | Full-stack | Out-of-office routing |
| SLA tracking | Backend | Time-in-queue calculation |
| Audit trail logging | Backend | All actions logged with IP |
| Bulk operations | Frontend | Batch approve/reject |

**Phase 2 Exit Criteria:**
- [ ] POST /api/v1/{tenant}/workflows creates approval rules
- [ ] POST /api/v1/{tenant}/invoices/{id}/approve works
- [ ] GET /api/v1/actions/{token}/approve works without authentication
- [ ] Email notifications sent on pending approval
- [ ] Delegation: users can set out-of-office routing
- [ ] SLA dashboard shows queue times
- [ ] Audit log captures: actor, action, timestamp, IP

**Success Metrics:**

| Metric | Target |
|--------|--------|
| Approval action latency | <5 seconds (P95) |
| Email approval success rate | ≥95% |
| Audit coverage | 100% of state changes |

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Production deployment and 5 pilot customers

#### Week 11: Production Readiness

| Task | Owner | Deliverable |
|------|-------|-------------|
| Production environment | DevOps | Kubernetes deployment on AWS |
| Security audit | Security | Penetration testing, SAST/DAST |
| Load testing | QA | 100 invoices/minute sustained |
| Monitoring + alerting | DevOps | Prometheus/Grafana dashboards |
| API documentation | Backend | OpenAPI spec published |
| User guides | Product | Help documentation |

#### Week 12: Customer Onboarding

| Task | Owner | Deliverable |
|------|-------|-------------|
| Data migration tooling | Backend | Import from CSV |
| White-glove onboarding | Product | Personal setup for each pilot |
| Feedback mechanisms | Product | In-app feedback, weekly calls |
| Bug triage process | Engineering | P0/P1/P2 classification |
| Hotfix process | DevOps | Emergency deploy pipeline |

**Phase 3 Exit Criteria:**
- [ ] Production deployment live on AWS
- [ ] Security audit passed (no critical/high vulnerabilities)
- [ ] Load test: 100 invoices/minute for 1 hour
- [ ] 5 pilot customers actively using platform
- [ ] API documentation published
- [ ] Support runbook covers top 20 scenarios

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy below 85%** | Medium | High | Multi-provider fallback; human review loop; collect training data |
| **Rust learning curve** | Medium | Medium | Pair programming; code reviews; consider Go for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; RLS as defense-in-depth |
| **Email approval token security** | Medium | High | HMAC with 72h expiration; one-time use; rate limiting; IP audit |
| **DuckDB scalability** | Medium | Medium | Partition by month; archive >12 months; evaluate ClickHouse |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant pools with limits; lazy connections; alerting |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep delays MVP** | High | High | Strict anti-goals; weekly scope review; "Phase 2" answer |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack |
| **ERP integration complexity** | High | Medium | Start with QuickBooks; use official SDK; defer others |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ; health checks; auto-failover |
| **Key person dependency** | High | High | ADRs; pair programming; knowledge sharing sessions |
| **Security incident** | Low | Critical | Pen testing; incident response plan; bug bounty (later) |

### 4.4 Risk Priority Matrix

```
                        IMPACT
                    Low       Medium      High        Critical
              ┌──────────┬──────────┬──────────┬──────────┐
         High │          │ Feature  │          │          │
              │          │ creep    │          │          │
              ├──────────┼──────────┼──────────┼──────────┤
PROBABILITY   │          │ Rust     │ OCR      │          │
       Medium │          │ learning │ accuracy │          │
              │          │ DuckDB   │ Pilot    │          │
              │          │ Connpool │ churn    │          │
              │          │          │ Email sec│          │
              ├──────────┼──────────┼──────────┼──────────┤
         Low  │          │          │          │ Data loss│
              │          │          │          │ Tenant   │
              │          │          │          │ isolation│
              │          │          │          │ Security │
              └──────────┴──────────┴──────────┴──────────┘
```

---

## 5. Resource Requirements

### 5.1 Team Structure

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                            BILL FORGE TEAM                                    │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ENGINEERING (4.5 FTE)                                                       │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                                                                          │ │
│  │  Backend Engineer (Rust) - 2 FTE                                        │ │
│  │  • bf-api, bf-invoice, bf-workflow, bf-ocr crates                       │ │
│  │  • Database schema design and queries                                    │ │
│  │  • OCR pipeline and accuracy optimization                                │ │
│  │  • Approval workflow engine                                              │ │
│  │                                                                          │ │
│  │  Frontend Engineer (Next.js/TypeScript) - 1 FTE                         │ │
│  │  • Invoice capture UI (upload, preview, correction)                      │ │
│  │  • Approval inbox and workflow UI                                        │ │
│  │  • Dashboard and analytics views                                         │ │
│  │  • Component library (shadcn/ui customization)                           │ │
│  │                                                                          │ │
│  │  Full-Stack / DevOps Engineer - 1 FTE                                   │ │
│  │  • CI/CD pipeline, Docker, Kubernetes                                    │ │
│  │  • Monitoring, alerting, observability                                   │ │
│  │  • Integration work between frontend and backend                         │ │
│  │  • Security hardening                                                    │ │
│  │                                                                          │ │
│  │  ML/AI Engineer (Contract) - 0.5 FTE                                    │ │
│  │  • OCR accuracy tuning and provider selection                            │ │
│  │  • Field extraction optimization                                         │ │
│  │  • Winston AI adaptation (Phase 3+)                                      │ │
│  │                                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│  PRODUCT (1 FTE)                                                             │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Product Manager - 1 FTE                                                 │ │
│  │  • Pilot customer relationships and onboarding                           │ │
│  │  • Feature prioritization and roadmap                                    │ │
│  │  • User research and feedback synthesis                                  │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│  TOTAL: 5.5 FTE for 12-week MVP                                              │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Hiring Priorities

| Role | Priority | Start By | Key Skills |
|------|----------|----------|------------|
| Backend Engineer (Rust) #1 | P0 | Week 1 | Rust, Axum, PostgreSQL, async |
| Backend Engineer (Rust) #2 | P0 | Week 1 | Rust, API design, SQLx |
| Frontend Engineer | P0 | Week 2 | Next.js 14+, TypeScript, Tailwind |
| DevOps Engineer | P1 | Week 1 | Docker, Kubernetes, GitHub Actions |
| ML/AI Contractor | P2 | Week 3 | OCR, document processing, Python |
| Product Manager | P1 | Week 1 | B2B SaaS, customer development |

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

### 5.4 Development Tools (Per User/Month)

| Tool | Cost | Purpose |
|------|-----:|---------|
| GitHub Team | $4 | Source control, CI/CD |
| Linear | $8 | Issue tracking |
| Figma | $15 | Design |
| Vercel | $20 | Frontend preview deploys |
| Posthog | Free | Product analytics |
| Sentry | Free | Error tracking |

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

## 7. Core Database Schema

### Control Plane (control_plane_db)

```sql
-- Tenant management
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    db_name VARCHAR(100) NOT NULL,
    modules JSONB DEFAULT '["invoice_capture", "invoice_processing"]',
    settings JSONB DEFAULT '{}',
    privacy_mode BOOLEAN DEFAULT FALSE,
    status VARCHAR(20) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Global users (can belong to multiple tenants)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    status VARCHAR(20) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- User-tenant membership
CREATE TABLE tenant_users (
    tenant_id UUID REFERENCES tenants(id),
    user_id UUID REFERENCES users(id),
    role VARCHAR(50) DEFAULT 'member',
    PRIMARY KEY (tenant_id, user_id)
);

-- API keys for programmatic access
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(255) NOT NULL,
    permissions JSONB DEFAULT '[]',
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Per-Tenant Schema (bf_tenant_{slug})

```sql
-- Vendors
CREATE TABLE vendors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    normalized_name VARCHAR(255) NOT NULL,
    tax_id VARCHAR(50),
    payment_terms INTEGER DEFAULT 30,
    default_gl_code VARCHAR(50),
    status VARCHAR(20) DEFAULT 'active',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_vendors_normalized_name ON vendors USING gin(normalized_name gin_trgm_ops);

-- Invoices
CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vendor_id UUID REFERENCES vendors(id),
    invoice_number VARCHAR(100),
    invoice_date DATE,
    due_date DATE,
    amount DECIMAL(15, 2),
    currency VARCHAR(3) DEFAULT 'USD',
    status VARCHAR(20) DEFAULT 'pending',
    ocr_confidence DECIMAL(5, 2),
    ocr_provider VARCHAR(50),
    document_path VARCHAR(500),
    extracted_data JSONB,
    queue VARCHAR(20) DEFAULT 'pending', -- pending, ap, review, error
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_invoices_status ON invoices(status);
CREATE INDEX idx_invoices_queue ON invoices(queue);
CREATE INDEX idx_invoices_vendor_id ON invoices(vendor_id);

-- Invoice line items
CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id) ON DELETE CASCADE,
    description TEXT,
    quantity DECIMAL(15, 4),
    unit_price DECIMAL(15, 4),
    amount DECIMAL(15, 2),
    gl_code VARCHAR(50),
    cost_center VARCHAR(50),
    sort_order INTEGER
);

-- Approval workflows
CREATE TABLE approval_workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    rules JSONB NOT NULL,
    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Approval steps
CREATE TABLE approval_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id),
    workflow_id UUID REFERENCES approval_workflows(id),
    step_number INTEGER,
    approver_id UUID,
    status VARCHAR(20) DEFAULT 'pending',
    action_at TIMESTAMPTZ,
    action_method VARCHAR(20), -- 'web', 'email', 'api'
    comments TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_approval_steps_approver_status ON approval_steps(approver_id, status);

-- Email action tokens
CREATE TABLE email_action_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id),
    action VARCHAR(20) NOT NULL, -- 'approve', 'reject'
    token_hash VARCHAR(255) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    used_from_ip INET,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_email_tokens_hash ON email_action_tokens(token_hash);

-- Audit log
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type VARCHAR(50) NOT NULL,
    entity_id UUID NOT NULL,
    action VARCHAR(50) NOT NULL,
    actor_id UUID,
    actor_type VARCHAR(20), -- 'user', 'system', 'api', 'email_token'
    old_values JSONB,
    new_values JSONB,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_created_at ON audit_log(created_at);
```

---

## 8. API Design

### 8.1 URL Pattern

```
/api/v1/{tenant}/resource/{id}
```

Example: `/api/v1/acme-corp/invoices/inv_123abc`

### 8.2 Response Format

**Success:**
```json
{
  "data": { ... },
  "meta": {
    "page": 1,
    "per_page": 50,
    "total": 1234
  }
}
```

**Error:**
```json
{
  "error": {
    "code": "INVOICE_NOT_FOUND",
    "message": "Invoice inv_123abc not found",
    "field": null
  }
}
```

### 8.3 Key Endpoints (MVP)

```
# Authentication
POST   /api/v1/auth/login
POST   /api/v1/auth/refresh
POST   /api/v1/auth/logout

# Invoice Capture
POST   /api/v1/{tenant}/invoices/upload
GET    /api/v1/{tenant}/invoices
GET    /api/v1/{tenant}/invoices/{id}
PATCH  /api/v1/{tenant}/invoices/{id}
POST   /api/v1/{tenant}/invoices/{id}/reprocess

# Queues
GET    /api/v1/{tenant}/queues/ap
GET    /api/v1/{tenant}/queues/review
GET    /api/v1/{tenant}/queues/errors
POST   /api/v1/{tenant}/queues/errors/{id}/resolve

# Approvals
GET    /api/v1/{tenant}/approvals/pending
POST   /api/v1/{tenant}/invoices/{id}/approve
POST   /api/v1/{tenant}/invoices/{id}/reject
POST   /api/v1/{tenant}/invoices/{id}/hold

# Email Actions (signed tokens, no auth required)
GET    /api/v1/actions/{signed_token}/approve
GET    /api/v1/actions/{signed_token}/reject

# Vendors
GET    /api/v1/{tenant}/vendors
POST   /api/v1/{tenant}/vendors
GET    /api/v1/{tenant}/vendors/{id}
PATCH  /api/v1/{tenant}/vendors/{id}

# Workflows
GET    /api/v1/{tenant}/workflows
POST   /api/v1/{tenant}/workflows
PATCH  /api/v1/{tenant}/workflows/{id}
```

---

## 9. Success Metrics

### 9.1 Technical Metrics (3-Month Horizon)

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

### 9.2 Business Metrics (3-Month Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 3,000+ | Total across pilots |
| **Customer NPS** | ≥50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | ≥60% | Conversion conversations |
| **Email Approval Success** | ≥95% | Successful / Total |

### 9.3 Operational Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deployment Frequency** | Daily | Deploys to staging |
| **Mean Time to Recovery** | <1 hour | Incident to resolution |
| **Change Failure Rate** | <15% | Deploys requiring rollback |

---

## 10. Answers to CEO's Strategic Questions

### Q1: Palette/Rillion Strengths and Weaknesses?

**Strengths:**
- 20+ years in Nordic/European markets with deep SAP/Oracle integrations
- Mature workflow engine for complex multinational scenarios
- Established customer base provides stability proof

**Weaknesses (Our Opportunities):**
- UI described as "slow" and "clunky" in customer reviews
- Limited AI/ML innovation in recent years
- Opaque "call for quote" pricing model
- Poor mobile experience
- Slow customer support

**Bill Forge Differentiation:**

| Dimension | Palette | Bill Forge | Advantage |
|-----------|---------|------------|-----------|
| UI Speed | Multi-second loads | Sub-second | 10x faster |
| Setup Time | Weeks/months | Hours/days | Self-service |
| Pricing | "Call for quote" | Published | Trust |
| OCR | Cloud-only | Local-first | Privacy |
| Approvals | Login required | Email (no login) | Frictionless |

### Q2: OCR Accuracy Threshold?

**Recommendation: Three-tier confidence routing**

| Confidence | Queue | Action |
|------------|-------|--------|
| **≥85%** | AP Queue | Auto-route to workflow |
| **70-84%** | Review Queue | Human verifies flagged fields |
| **<70%** | Error Queue | Full manual entry required |

**Implementation Notes:**
- Calculate overall confidence as weighted average of field confidences
- Weight amount and vendor_name higher (critical fields)
- Store per-field confidence for granular review UI
- Collect corrections as training data for future optimization

### Q3: Which ERP Integration First?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Complexity | Timeline |
|-----|----------|------------|----------|
| **QuickBooks Online** | 1 | Low | 2-3 weeks |
| NetSuite | 2 | Medium | 4-6 weeks |
| Sage Intacct | 3 | Medium | 4-6 weeks |
| Dynamics 365 | 4 | High | 6-8 weeks |

**Why QuickBooks First:**
- Largest addressable market for 10-1000 employee companies
- Simplest REST API with excellent documentation
- Enables ProAdvisor partnership channel (75K+ referral partners)

### Q4: Common Approval Workflow Patterns?

**Research-Based Patterns:**

1. **Amount-Based Tiers (85% of companies)** - MVP Priority
   ```
   < $5,000:      Auto-approve (if vendor known)
   $5K - $25K:    Manager approval
   $25K - $50K:   Director/VP approval
   > $50K:        CFO/Controller approval
   ```

2. **Exception-Only Review (65%)** - MVP Priority
   - Clean invoices (match PO, known vendor) → auto-approve
   - Exceptions (no PO, new vendor) → review queue

3. **Department/Cost Center (45%)** - Phase 2
4. **Dual Approval (30%)** - Phase 2

**MVP Implementation:** Amount-based tiers + exception routing

### Q5: Multi-Currency Handling?

**MVP Approach:**
- Capture currency from invoice as metadata
- Support: USD, EUR, GBP, CAD
- Convert for display totals using daily rates (Open Exchange Rates API)
- Store both original and converted amounts
- Send base currency amount to ERP
- **Defer full multi-currency GL posting to Phase 3**

### Q6: Pricing Model?

**Recommendation: Tiered Usage-Based Pricing**

| Tier | Monthly Base | Invoices | Overage | Target |
|------|--------------|----------|---------|--------|
| **Starter** | $299 | 500 | $0.75 | Testing |
| **Growth** | $799 | 2,000 | $0.50 | Primary ICP |
| **Scale** | $1,999 | 10,000 | $0.30 | Larger mid-market |
| **Enterprise** | Custom | Custom | Custom | 10K+ invoices |

**Why This Model:**
- **No per-seat pricing:** AP teams hate paying for each approver
- **Predictable base:** Finance can budget effectively
- **Scales with business:** Aligned with value delivered
- **Transparent:** Published pricing builds trust

---

## 11. Winston AI Strategy (Leveraging Locust)

### 11.1 What to Reuse from Locust

The existing `locust` codebase provides a sophisticated multi-agent AI framework. For Winston, we'll adapt:

| Locust Component | Adaptation for Winston |
|------------------|------------------------|
| Agent base classes (agents/base.py) | Simplify for single-agent use |
| LLM backend switching (llm/) | Keep Claude + Ollama support |
| Workflow state (workflows/) | Adapt for query/action patterns |
| Memory/embeddings (memory/) | Use for semantic search over tenant data |
| Checkpoint/resume | Conversation recovery |

**Remove from Winston:**
- Software development agents (CTO, CPO, etc.)
- Code generation modules
- Research workflows
- Git integration

### 11.2 Winston Tool Design

```python
@tool
async def search_invoices(query: str, tenant_id: str, limit: int = 10) -> list[Invoice]:
    """Search invoices by vendor name, amount, or status.

    Examples:
    - "invoices from Acme Corp"
    - "pending invoices over $10,000"
    """
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

### 11.3 Winston Timeline

**Phase 3+ (Post-MVP):** ~3 weeks to adapt Locust architecture

| Week | Focus |
|------|-------|
| 1 | Fork Locust agent core, strip unused code |
| 2 | Implement Bill Forge tools, API integration |
| 3 | Chat UI, testing, tenant isolation |

**Effort Savings:** 60% reduction vs. building from scratch

---

## 12. Immediate Next Steps

### This Week (Week 0)

**Day 1-2: Repository Setup**
- [ ] Create `bill-forge` repository (separate from locust)
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

### Week 1 Deliverables

| Deliverable | Owner | Status |
|-------------|-------|--------|
| Monorepo initialized | DevOps | [ ] |
| Docker Compose running | DevOps | [ ] |
| bf-api health check working | Backend | [ ] |
| bf-tenant creates tenant databases | Backend | [ ] |
| CI pipeline passing on main | DevOps | [ ] |
| Next.js app with shadcn/ui | Frontend | [ ] |

---

## Appendix A: Architecture Decision Records

### ADR-001: Database-per-Tenant Isolation

**Status:** Accepted
**Decision:** Use database-per-tenant model (not RLS)
**Consequences:**
- (+) Complete data isolation for compliance
- (+) Per-tenant backup/restore
- (+) Easy data portability
- (-) Higher connection overhead
- (-) More complex migrations

### ADR-002: OCR Provider Strategy

**Status:** Accepted
**Decision:** Tesseract 5 default, cloud for escalation
**Consequences:**
- (+) Privacy-first positioning
- (+) Low cost for high-confidence invoices
- (-) Slightly lower accuracy than cloud-only

### ADR-003: Email Approval Security

**Status:** Accepted
**Decision:** HMAC-signed tokens, 72h expiration, one-time use
**Consequences:**
- (+) Frictionless approver experience
- (+) Works on mobile without app
- (-) Tokens can be forwarded (mitigated by audit logging)

### ADR-004: Dual Codebase Strategy

**Status:** Accepted
**Decision:** Separate Bill Forge (Rust) from Locust (Python)
**Consequences:**
- (+) Clean separation of concerns
- (+) Optimal language for each purpose
- (+) Locust agent architecture reusable for Winston
- (-) Two codebases to maintain
- (-) Locust → Winston adaptation effort

---

## Appendix B: Local Development Setup

```bash
# Prerequisites
# - Rust 1.75+
# - Node.js 20+
# - pnpm 8+
# - Docker & Docker Compose

# Clone and setup
git clone https://github.com/billforge/bill-forge.git
cd bill-forge

# Start infrastructure
docker compose up -d postgres redis minio

# Install dependencies
pnpm install
cargo build

# Run migrations
cargo run -p bf-tenant -- migrate

# Start services (separate terminals)
cargo run -p bf-api
pnpm --filter web dev

# Access
# API: http://localhost:8080
# Web: http://localhost:3000
# MinIO: http://localhost:9001
```

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-31 | CTO | Initial technical plan |
| 2.0 | 2026-02-01 | CTO | Consolidated execution-ready version |
| 3.0 | 2026-02-01 | CTO | Added dual-codebase strategy, Locust assessment |

**Sign-offs:**
- [ ] CEO Approval
- [ ] CPO Alignment Confirmation
- [ ] Engineering Lead Review

---

*This technical plan is the strategic execution document for Bill Forge. It supersedes all previous versions and will be updated as decisions evolve based on pilot feedback.*
