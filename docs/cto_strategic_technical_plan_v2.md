# Bill Forge - CTO Strategic Technical Plan

**Date:** January 31, 2026
**Version:** 2.0
**Author:** CTO
**Status:** Final for Approval

---

## Executive Summary

Bill Forge is a greenfield B2B SaaS platform for mid-market invoice processing. Based on the CEO's vision and prior research, this plan provides specific, actionable technical guidance for the 12-week MVP development.

**Strategic Technical Decisions:**

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Architecture** | Modular monolith → services | Start simple, split later |
| **Backend** | Rust + Axum | Performance, safety, CEO preference |
| **Frontend** | Next.js 14+ App Router | Modern React, RSC, CEO preference |
| **Database** | PostgreSQL (per-tenant) + DuckDB | Isolation + analytics |
| **OCR** | Tesseract 5 primary, cloud fallback | Privacy-first, cost-effective |
| **AI Assistant** | Adapted from Locust (Phase 3) | Leverage existing LangGraph framework |

**Key Insight:** The existing Locust codebase is an AI agent orchestration framework—not an invoice platform. However, its LangGraph-based agent architecture can be repurposed for Winston AI Assistant in Phase 3, reducing that feature's development time by approximately 60%.

---

## 1. Technical Architecture Recommendations

### 1.1 System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              BILL FORGE PLATFORM                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│   ┌──────────────────────────────────────────────────────────────────────────┐  │
│   │                          NEXT.JS FRONTEND                                 │  │
│   │                                                                           │  │
│   │   /invoices          /approvals         /vendors        /reports         │  │
│   │   ┌─────────┐       ┌─────────┐        ┌─────────┐     ┌─────────┐      │  │
│   │   │ Upload  │       │  Inbox  │        │  Master │     │Dashboard│      │  │
│   │   │ Preview │       │ Actions │        │  Data   │     │ Charts  │      │  │
│   │   │ Correct │       │  SLA    │        │ Tax Doc │     │ Export  │      │  │
│   │   └─────────┘       └─────────┘        └─────────┘     └─────────┘      │  │
│   │                                                                           │  │
│   │   shadcn/ui + Tailwind CSS + TanStack Query + React Hook Form            │  │
│   └──────────────────────────────────────────────────────────────────────────┘  │
│                                        │                                         │
│                                        ▼                                         │
│   ┌──────────────────────────────────────────────────────────────────────────┐  │
│   │                           RUST API GATEWAY                                │  │
│   │                             (bf-api crate)                                │  │
│   │                                                                           │  │
│   │   Axum 0.7+ │ JWT Auth │ Tenant Resolution │ Rate Limiting │ CORS        │  │
│   │                                                                           │  │
│   │   Middleware Chain:                                                       │  │
│   │   Request → Logging → Auth → Tenant → Rate Limit → Handler → Response    │  │
│   └──────────────────────────────────────────────────────────────────────────┘  │
│                                        │                                         │
│          ┌─────────────────────────────┼─────────────────────────────┐          │
│          ▼                             ▼                             ▼          │
│   ┌──────────────┐            ┌──────────────┐            ┌──────────────┐     │
│   │  bf-invoice  │            │ bf-workflow  │            │  bf-vendor   │     │
│   ├──────────────┤            ├──────────────┤            ├──────────────┤     │
│   │ Upload API   │            │ Rule Engine  │            │ Master Data  │     │
│   │ OCR Pipeline │            │ State Machine│            │ Tax Storage  │     │
│   │ Extraction   │            │ Notifications│            │ Fuzzy Match  │     │
│   │ Confidence   │            │ Email Actions│            │ Spend View   │     │
│   │ Queue Route  │            │ SLA Tracking │            │              │     │
│   └──────────────┘            └──────────────┘            └──────────────┘     │
│          │                             │                             │          │
│          └─────────────────────────────┼─────────────────────────────┘          │
│                                        ▼                                         │
│   ┌──────────────────────────────────────────────────────────────────────────┐  │
│   │                           bf-ocr (Provider Abstraction)                   │  │
│   │                                                                           │  │
│   │   ┌───────────────┐   ┌───────────────┐   ┌───────────────┐              │  │
│   │   │ Tesseract 5   │   │ AWS Textract  │   │ Google Vision │              │  │
│   │   │ (Local/Free)  │   │ (Cloud/$0.01) │   │ (Fallback)    │              │  │
│   │   │ DEFAULT       │   │ ESCALATION    │   │ HANDWRITING   │              │  │
│   │   └───────────────┘   └───────────────┘   └───────────────┘              │  │
│   └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                   DATA LAYER                                     │
│                                                                                  │
│   ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐                │
│   │  Control Plane  │  │  Tenant DBs     │  │  MinIO (S3)     │                │
│   │   PostgreSQL    │  │   PostgreSQL    │  │                 │                │
│   │                 │  │                 │  │  /tenant-a/     │                │
│   │ • tenants       │  │ acme_corp_db:   │  │   /invoices/    │                │
│   │ • users         │  │  • invoices     │  │   /tax-docs/    │                │
│   │ • api_keys      │  │  • vendors      │  │                 │                │
│   │ • subscriptions │  │  • workflows    │  │  /tenant-b/     │                │
│   │ • billing       │  │  • audit_log    │  │   /invoices/    │                │
│   └─────────────────┘  └─────────────────┘  └─────────────────┘                │
│                                                                                  │
│   ┌─────────────────┐  ┌─────────────────┐                                     │
│   │  DuckDB         │  │  Redis          │                                     │
│   │  (Per-Tenant)   │  │                 │                                     │
│   │                 │  │ • Sessions      │                                     │
│   │ • metrics       │  │ • Rate limits   │                                     │
│   │ • aggregates    │  │ • Job queues    │                                     │
│   │ • reports       │  │ • Pub/Sub       │                                     │
│   └─────────────────┘  └─────────────────┘                                     │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Tenant Isolation Architecture

**Decision: Database-per-tenant** (not row-level security)

```
                    ┌───────────────────────────────────────┐
                    │           CONTROL PLANE               │
                    │                                       │
                    │  control_plane_db (PostgreSQL)        │
                    │  ┌─────────────────────────────────┐  │
                    │  │ tenants                          │  │
                    │  │  • id: uuid                      │  │
                    │  │  • slug: "acme-corp"             │  │
                    │  │  • db_name: "bf_tenant_acme"     │  │
                    │  │  • modules: ["capture","process"]│  │
                    │  │  • settings: {...}               │  │
                    │  └─────────────────────────────────┘  │
                    └───────────────────────────────────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    ▼                  ▼                  ▼
            ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
            │bf_tenant_acme│   │bf_tenant_tech│   │bf_tenant_mfg │
            ├──────────────┤   ├──────────────┤   ├──────────────┤
            │• invoices    │   │• invoices    │   │• invoices    │
            │• vendors     │   │• vendors     │   │• vendors     │
            │• workflows   │   │• workflows   │   │• workflows   │
            │• audit_log   │   │• audit_log   │   │• audit_log   │
            └──────────────┘   └──────────────┘   └──────────────┘
```

**Rationale:**
- **Complete isolation**: No accidental cross-tenant data exposure
- **Per-tenant backup/restore**: Critical for compliance and disaster recovery
- **Data portability**: Tenant can export their database easily
- **Regulatory compliance**: Healthcare, legal, and finance customers require this
- **Trade-off**: Higher connection overhead (mitigated by per-tenant connection pools)

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            OCR PROCESSING PIPELINE                           │
│                                                                              │
│   ┌─────────────┐                                                           │
│   │   INGEST    │  Supported formats: PDF, PNG, JPG, TIFF                  │
│   │             │  Max size: 25MB                                           │
│   │             │  Validation: file type, malware scan (ClamAV)            │
│   └──────┬──────┘                                                           │
│          │                                                                   │
│          ▼                                                                   │
│   ┌─────────────┐                                                           │
│   │ PREPROCESS  │  • Deskew (correct rotation)                             │
│   │             │  • Contrast enhancement                                   │
│   │             │  • Noise reduction                                        │
│   │             │  • Document classification (invoice vs other)            │
│   └──────┬──────┘                                                           │
│          │                                                                   │
│          ▼                                                                   │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                        PROVIDER ROUTER                               │   │
│   │                                                                      │   │
│   │   Tenant Privacy Mode = TRUE?                                        │   │
│   │      └─► Tesseract 5 ONLY (no cloud)                                │   │
│   │                                                                      │   │
│   │   Tenant Privacy Mode = FALSE (default)?                            │   │
│   │      └─► Tesseract 5 (primary)                                      │   │
│   │           └─► If confidence < 75% → AWS Textract                    │   │
│   │                └─► If still < 75% → Google Vision                   │   │
│   │                     └─► If still < 70% → Error Queue                │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│          │                                                                   │
│          ▼                                                                   │
│   ┌─────────────┐                                                           │
│   │   EXTRACT   │  Header Fields:                                          │
│   │             │   • vendor_name (+ normalized)     • invoice_number      │
│   │             │   • invoice_date                   • due_date            │
│   │             │   • total_amount                   • currency            │
│   │             │   • tax_amount                     • subtotal            │
│   │             │                                                          │
│   │             │  Line Items (Phase 1.5):                                 │
│   │             │   • description, quantity, unit_price, amount, gl_code   │
│   └──────┬──────┘                                                           │
│          │                                                                   │
│          ▼                                                                   │
│   ┌─────────────┐                                                           │
│   │  VALIDATE   │  • Required fields present?                              │
│   │             │  • Date/amount format valid?                              │
│   │             │  • Duplicate check (invoice# + vendor hash)              │
│   │             │  • Vendor fuzzy match against master list               │
│   └──────┬──────┘                                                           │
│          │                                                                   │
│          ▼                                                                   │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                      CONFIDENCE ROUTER                               │   │
│   │                                                                      │   │
│   │   >= 85% Confidence ────────────────────► AP QUEUE                  │   │
│   │                                            (auto-route to workflow)  │   │
│   │                                                                      │   │
│   │   70-84% Confidence ────────────────────► REVIEW QUEUE              │   │
│   │                                            (human verifies flagged)  │   │
│   │                                                                      │   │
│   │   < 70% Confidence ─────────────────────► ERROR QUEUE               │   │
│   │                                            (manual entry required)   │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.4 Approval Workflow Engine

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        APPROVAL WORKFLOW ENGINE                              │
│                                                                              │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                          RULE ENGINE                                 │   │
│   │                                                                      │   │
│   │   Rules stored as JSON, evaluated by Rust expression engine         │   │
│   │                                                                      │   │
│   │   {                                                                  │   │
│   │     "name": "Standard Amount Tiers",                                │   │
│   │     "priority": 1,                                                   │   │
│   │     "conditions": [                                                  │   │
│   │       { "if": "amount < 5000", "action": "auto_approve" },          │   │
│   │       { "if": "amount >= 5000 && amount < 25000",                   │   │
│   │         "action": { "route_to": "manager", "level": 1 } },          │   │
│   │       { "if": "amount >= 25000 && amount < 50000",                  │   │
│   │         "action": { "route_to": "director", "level": 2 } },         │   │
│   │       { "if": "amount >= 50000",                                    │   │
│   │         "action": { "route_to": "cfo", "level": 3 } }               │   │
│   │     ],                                                               │   │
│   │     "exceptions": [                                                  │   │
│   │       { "if": "vendor.is_new == true", "action": "add_review" },    │   │
│   │       { "if": "po_mismatch == true", "action": "exception_queue" }  │   │
│   │     ]                                                                │   │
│   │   }                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                     │                                        │
│                                     ▼                                        │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                        STATE MACHINE                                 │   │
│   │                                                                      │   │
│   │                        ┌──────────┐                                 │   │
│   │                        │ PENDING  │                                 │   │
│   │                        └────┬─────┘                                 │   │
│   │                             │                                        │   │
│   │         ┌───────────────────┼───────────────────┐                   │   │
│   │         ▼                   ▼                   ▼                   │   │
│   │   ┌──────────┐       ┌──────────┐       ┌──────────┐               │   │
│   │   │ L1_WAIT  │──────►│ L2_WAIT  │──────►│ L3_WAIT  │               │   │
│   │   │(Manager) │       │(Director)│       │  (CFO)   │               │   │
│   │   └────┬─────┘       └────┬─────┘       └────┬─────┘               │   │
│   │        │                  │                  │                      │   │
│   │        ▼                  ▼                  ▼                      │   │
│   │   ┌─────────────────────────────────────────────────────────┐      │   │
│   │   │                 TERMINAL STATES                          │      │   │
│   │   │  ┌──────────┐   ┌──────────┐   ┌──────────┐             │      │   │
│   │   │  │ APPROVED │   │ REJECTED │   │ ON_HOLD  │             │      │   │
│   │   │  └──────────┘   └──────────┘   └──────────┘             │      │   │
│   │   └─────────────────────────────────────────────────────────┘      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                     │                                        │
│                                     ▼                                        │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                    EMAIL APPROVAL (Key Differentiator)               │   │
│   │                                                                      │   │
│   │   Email Template:                                                    │   │
│   │   ┌──────────────────────────────────────────────────────────────┐  │   │
│   │   │  From: notifications@billforge.io                            │  │   │
│   │   │  Subject: [Action Required] Invoice #INV-2024-001            │  │   │
│   │   │                                                               │  │   │
│   │   │  Vendor: Acme Corporation                                     │  │   │
│   │   │  Amount: $12,500.00 USD                                       │  │   │
│   │   │  Due Date: February 15, 2026                                  │  │   │
│   │   │                                                               │  │   │
│   │   │  [  APPROVE  ]    [  REJECT  ]    [  VIEW  ]                 │  │   │
│   │   │                                                               │  │   │
│   │   │  Links are HMAC-signed, expire in 72 hours                   │  │   │
│   │   │  No login required to approve or reject                       │  │   │
│   │   └──────────────────────────────────────────────────────────────┘  │   │
│   │                                                                      │   │
│   │   Action Endpoint: GET /api/v1/actions/{signed_token}/approve       │   │
│   │   - One-time use (invalidated after action)                         │   │
│   │   - IP logging for audit trail                                      │   │
│   │   - Rate limited (5 attempts per token)                             │   │
│   │                                                                      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | CEO preference, async-first, Tower middleware |
| Async Runtime | **Tokio** | 1.x | Industry standard, required by Axum |
| Serialization | **Serde** | Latest | De facto Rust standard |
| Database | **SQLx** | 0.7+ | Compile-time query checking, async |
| Migrations | **sqlx-cli** | 0.7+ | Integrated migration tooling |
| Validation | **validator** | Latest | Derive macros for requests |
| Errors | **thiserror** | Latest | Typed error definitions |
| Logging | **tracing** | Latest | Structured logging, spans |
| Config | **config-rs** | Latest | Multi-source configuration |
| HTTP Client | **reqwest** | Latest | OCR provider calls |
| UUID | **uuid** | Latest | Entity identifiers |
| Date/Time | **chrono** | Latest | Timestamps |
| Password | **argon2** | Latest | Secure password hashing |
| JWT | **jsonwebtoken** | Latest | Token auth |
| Testing | **tokio-test**, **wiremock** | Latest | Async tests, HTTP mocking |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | CEO preference, App Router, RSC |
| Language | **TypeScript** | 5.x | Strict mode enabled |
| Styling | **Tailwind CSS** | 3.x | CEO preference |
| Components | **shadcn/ui** | Latest | CEO preference, accessible |
| Server State | **TanStack Query** | 5.x | Caching, optimistic updates |
| Forms | **React Hook Form** + **Zod** | Latest | Type-safe validation |
| Tables | **TanStack Table** | 8.x | Invoice lists, data grids |
| Charts | **Recharts** | 2.x | Analytics dashboards |
| Date Picker | **date-fns** + **react-day-picker** | Latest | Invoice date handling |
| Notifications | **Sonner** | Latest | Toast notifications |
| Icons | **Lucide React** | Latest | Consistent iconography |
| Auth Client | **next-auth** | 4.x | Session management |
| API Client | Generated from **OpenAPI** | - | Type-safe API calls |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB, CEO preference |
| Analytics DB | **DuckDB** | Embedded, fast aggregations, CEO preference |
| Document Storage | **MinIO** | S3-compatible, local-first development |
| Cache / Queue | **Redis 7** | Sessions, rate limiting, job queues |
| Search | **PostgreSQL FTS + pg_trgm** | Start simple, add Meilisearch if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost per Page |
|----------|----------|----------|---------------|
| **Tesseract 5** | Primary | Local, privacy-first, standard invoices | Free |
| **AWS Textract** | Secondary | Complex layouts, handwriting | ~$0.01 |
| **Google Vision** | Tertiary | Fallback, handwriting | ~$0.0015 |

**Decision:** Default to Tesseract. Escalate to cloud only when confidence < 75% and tenant allows cloud OCR.

### 2.5 Infrastructure

| Component | Technology | Environment |
|-----------|------------|-------------|
| Containers | **Docker** | All |
| Dev Orchestration | **Docker Compose** | Development |
| Prod Orchestration | **Kubernetes (EKS/GKE)** | Production |
| CI/CD | **GitHub Actions** | All |
| Secrets | **HashiCorp Vault** | Production |
| Monitoring | **Prometheus + Grafana** | All |
| Tracing | **OpenTelemetry + Jaeger** | All |
| Email | **AWS SES** or **Resend** | Production |
| CDN | **CloudFront** or **Cloudflare** | Production |

---

## 3. Development Priorities and Phases

### Phase Timeline Overview

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           12-WEEK MVP TIMELINE                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Week:   1    2    3    4    5    6    7    8    9   10   11   12              │
│          ├────┴────┼────┴────┴────┴────┼────┴────┴────┴────┼────┴────┤         │
│          │         │                    │                    │         │         │
│          │  P0     │        P1          │         P2         │   P3    │         │
│          │ Found-  │  Invoice Capture   │  Invoice Processing│  Pilot  │         │
│          │ ation   │                    │                    │  Launch │         │
│          │         │                    │                    │         │         │
│          ▼         ▼                    ▼                    ▼         ▼         │
│                                                                                  │
│  M1: Auth+API    M2: OCR Pipeline    M3: Workflow Engine   M4: 5 Pilots Live   │
│      Complete        Functional          Functional                              │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Establish project structure, infrastructure, and authentication

#### Week 1: Infrastructure

| Task | Owner | Deliverable |
|------|-------|-------------|
| Create monorepo structure | DevOps | `Cargo.toml` workspace + `pnpm-workspace.yaml` |
| Docker Compose setup | DevOps | PostgreSQL, Redis, MinIO running locally |
| CI/CD pipeline | DevOps | GitHub Actions: lint, test, build on PR |
| Control plane schema | Backend | `tenants`, `users`, `api_keys` tables |
| Tenant service | Backend | `bf-tenant` crate: create/list tenants |
| SQLx migrations | Backend | Migration infrastructure |

#### Week 2: Auth + API Foundation

| Task | Owner | Deliverable |
|------|-------|-------------|
| JWT authentication | Backend | `bf-auth` crate: issue/verify tokens |
| API gateway | Backend | `bf-api` crate with health check |
| Tenant resolution middleware | Backend | Extract tenant from URL path |
| Next.js scaffold | Frontend | App with shadcn/ui, login page |
| API client generation | Frontend | OpenAPI → TypeScript client |
| Seed data/fixtures | Full-stack | Development test data |

**Phase 0 Deliverables Checklist:**
- [ ] Monorepo with `crates/` and `apps/web/`
- [ ] Docker Compose running Postgres, Redis, MinIO
- [ ] `bf-api` health endpoint: `GET /health`
- [ ] `bf-tenant` can create tenant databases
- [ ] `bf-auth` issues and verifies JWTs
- [ ] Next.js app renders login page
- [ ] CI pipeline passes on every PR

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with confidence-based routing and manual review

#### Weeks 3-4: OCR Pipeline

| Task | Owner | Deliverable |
|------|-------|-------------|
| Document upload API | Backend | `POST /api/v1/{tenant}/invoices/upload` |
| S3 storage abstraction | Backend | `bf-storage` crate |
| Tesseract integration | Backend | `bf-ocr` crate with local OCR |
| Field extraction | Backend | Vendor, invoice #, amount, date, currency |
| Confidence scoring | Backend | Per-field and overall confidence |
| Queue data models | Backend | `invoices`, `invoice_queue` tables |

#### Weeks 5-6: Capture UI + Vendor Matching

| Task | Owner | Deliverable |
|------|-------|-------------|
| Invoice upload UI | Frontend | Drag-drop, file preview |
| OCR results display | Frontend | Confidence badges per field |
| Manual correction UI | Frontend | Inline edit with visual highlighting |
| Vendor fuzzy matching | Backend | Levenshtein distance matching |
| Vendor CRUD API | Backend | `GET/POST/PATCH /vendors` |
| Queue dashboard | Frontend | AP queue, review queue, error queue views |

**Phase 1 Deliverables Checklist:**
- [ ] `POST /api/v1/{tenant}/invoices/upload` accepts PDF/images
- [ ] `GET /api/v1/{tenant}/invoices/{id}` returns extracted data
- [ ] `GET /api/v1/{tenant}/queues/ap` lists high-confidence invoices
- [ ] `GET /api/v1/{tenant}/queues/errors` lists low-confidence invoices
- [ ] OCR extracts: vendor_name, invoice_number, amount, date, currency
- [ ] Confidence threshold routing: ≥85% → AP, 70-84% → review, <70% → error
- [ ] Manual correction updates invoice data
- [ ] Vendor matching suggests existing vendors

**Success Metrics:**
- OCR accuracy: ≥85% on clean PDF test set
- Processing time: <3 seconds per invoice (P95)
- Manual correction flow: <30 seconds to fix one field

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Objective:** Approval workflows with email actions

#### Weeks 7-8: Workflow Engine

| Task | Owner | Deliverable |
|------|-------|-------------|
| Workflow rule engine | Backend | `bf-workflow` crate with JSON rules |
| Approval state machine | Backend | States: pending, l1_wait, l2_wait, approved, rejected |
| Rule configuration API | Backend | `GET/POST /workflows` |
| Approval inbox UI | Frontend | Pending items with bulk select |
| Approve/reject/hold actions | Frontend + Backend | Action buttons + API endpoints |

#### Weeks 9-10: Email Actions + Audit

| Task | Owner | Deliverable |
|------|-------|-------------|
| Signed token generation | Backend | HMAC tokens with expiration |
| Email approval endpoints | Backend | `GET /api/v1/actions/{token}/approve` (no auth) |
| Email service integration | Backend | SES/Resend for notifications |
| Delegation config | Backend + Frontend | Out-of-office routing |
| SLA tracking | Backend | Time-in-queue calculation |
| Escalation alerts | Backend | Email/notification on SLA breach |
| Audit trail logging | Backend | All actions logged with actor, timestamp, IP |
| Bulk operations | Frontend | Batch approve/reject |

**Phase 2 Deliverables Checklist:**
- [ ] `POST /api/v1/{tenant}/workflows` creates approval rules
- [ ] `POST /api/v1/{tenant}/invoices/{id}/approve` approves invoice
- [ ] `POST /api/v1/{tenant}/invoices/{id}/reject` rejects invoice
- [ ] `GET /api/v1/actions/{token}/approve` works without authentication
- [ ] Email notifications sent on pending approval
- [ ] Delegation: users can set out-of-office routing
- [ ] SLA dashboard shows queue times, escalation status
- [ ] Audit log captures: actor, action, timestamp, IP, old/new values

**Success Metrics:**
- Approval action latency: <5 seconds (P95)
- Email approval success rate: ≥95%
- Audit coverage: 100% of state changes logged

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Deploy to production and onboard 5 pilot customers

#### Week 11: Production Readiness

| Task | Owner | Deliverable |
|------|-------|-------------|
| Production environment | DevOps | Kubernetes deployment (EKS/GKE) |
| Security audit | Security | Penetration testing, SAST/DAST |
| Load testing | QA | 100 invoices/minute sustained |
| Monitoring + alerting | DevOps | Prometheus/Grafana dashboards |
| API documentation | Backend | OpenAPI spec published |
| User guides | Product | Help documentation |
| Support runbook | DevOps | Incident response procedures |

#### Week 12: Customer Onboarding

| Task | Owner | Deliverable |
|------|-------|-------------|
| Data migration tooling | Backend | Import from CSV/existing systems |
| White-glove onboarding | Product | Personal setup for each pilot |
| Feedback mechanisms | Product | In-app feedback, weekly calls |
| Bug triage process | Engineering | P0/P1/P2 classification |
| Hotfix process | DevOps | Emergency deploy pipeline |

**Phase 3 Deliverables Checklist:**
- [ ] Production deployment live on cloud infrastructure
- [ ] Security audit passed (no critical/high vulnerabilities)
- [ ] Load test: 100 invoices/minute for 1 hour
- [ ] 5 pilot customers actively using the platform
- [ ] API documentation published at `docs.billforge.io`
- [ ] Support runbook covers top 20 scenarios

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **OCR accuracy below 85%** | Medium | High | Multi-provider fallback; human review for low confidence; collect training data from corrections |
| **Rust learning curve slows velocity** | Medium | Medium | Pair programming; comprehensive code reviews; reserve Go as fallback for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant eliminates cross-tenant queries; penetration testing; defense-in-depth with RLS |
| **Email action token security** | Medium | High | HMAC-signed with 72h expiration; one-time use; rate limiting; IP logging in audit trail |
| **DuckDB scalability limits** | Medium | Medium | Partition by month; archive data >12 months; evaluate ClickHouse if needed |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant connection pools with hard limits; lazy tenant DB connections; alerting |
| **S3/MinIO file handling issues** | Low | Medium | Retry logic with exponential backoff; async upload with progress tracking |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **Feature creep delays MVP** | High | High | Strict adherence to anti-goals; weekly scope review; "Phase 2" is the answer |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack channel; white-glove support |
| **ERP integration complexity** | High | Medium | Start with QuickBooks Online (simplest); use official SDK; defer NetSuite/Sage |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs through workflow customization |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **Data loss** | Low | Critical | Daily automated backups; point-in-time recovery; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ deployment; health checks; automatic failover; <1h MTTR target |
| **Key person dependency** | High | High | Document all decisions (ADRs); pair programming; weekly knowledge sharing |
| **Security incident** | Low | Critical | Penetration testing pre-launch; incident response plan; consider bug bounty |

### 4.4 Risk Priority Matrix

```
                    IMPACT
                    Low        Medium       High        Critical
              ┌─────────────┬────────────┬────────────┬────────────┐
         High │             │ Feature    │            │            │
              │             │ creep      │            │            │
              ├─────────────┼────────────┼────────────┼────────────┤
  P    Medium │             │ Rust       │ OCR        │            │
  R           │             │ learning   │ accuracy   │            │
  O           │             │ DuckDB     │ Pilot churn│            │
  B           │             │ scale      │ Email sec  │            │
  A           │             │ Connpool   │            │            │
  B    ├─────────────┼────────────┼────────────┼────────────┤
  I     Low   │             │ S3 issues  │            │ Data loss  │
  L           │             │            │            │ Tenant     │
  I           │             │            │            │ isolation  │
  T           │             │            │            │ Security   │
  Y           │             │            │            │ incident   │
              └─────────────┴────────────┴────────────┴────────────┘
```

---

## 5. Resource Requirements

### 5.1 Team Structure

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                             BILL FORGE TEAM                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│   ENGINEERING (4.5 FTE)                                                         │
│   ┌─────────────────────────────────────────────────────────────────────────┐  │
│   │                                                                          │  │
│   │   Backend Engineer (Rust) - 2 FTE                                       │  │
│   │   ├── bf-api, bf-invoice, bf-workflow, bf-ocr crates                   │  │
│   │   ├── Database schema design and queries                                │  │
│   │   ├── OCR pipeline and accuracy optimization                            │  │
│   │   └── Approval workflow engine                                          │  │
│   │                                                                          │  │
│   │   Frontend Engineer (Next.js/TypeScript) - 1 FTE                        │  │
│   │   ├── Invoice capture UI (upload, preview, correction)                  │  │
│   │   ├── Approval inbox and workflow UI                                    │  │
│   │   ├── Dashboard and analytics views                                     │  │
│   │   └── Component library (shadcn/ui customization)                       │  │
│   │                                                                          │  │
│   │   Full-Stack / DevOps Engineer - 1 FTE                                  │  │
│   │   ├── CI/CD pipeline, Docker, Kubernetes                                │  │
│   │   ├── Monitoring, alerting, observability                               │  │
│   │   ├── Integration work between frontend and backend                     │  │
│   │   └── Security hardening, penetration testing prep                      │  │
│   │                                                                          │  │
│   │   ML/AI Engineer (Contract) - 0.5 FTE                                   │  │
│   │   ├── OCR accuracy tuning and provider selection                        │  │
│   │   ├── Field extraction optimization                                     │  │
│   │   └── Winston AI adaptation (Phase 3+)                                  │  │
│   │                                                                          │  │
│   └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
│   PRODUCT (1 FTE)                                                               │
│   ┌─────────────────────────────────────────────────────────────────────────┐  │
│   │                                                                          │  │
│   │   Product Manager - 1 FTE                                               │  │
│   │   ├── Pilot customer relationships and onboarding                       │  │
│   │   ├── Feature prioritization and roadmap                                │  │
│   │   ├── User research and feedback synthesis                              │  │
│   │   └── Documentation and help content                                    │  │
│   │                                                                          │  │
│   └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
│   TOTAL: 5.5 FTE for 12-week MVP                                               │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Hiring Priorities

| Role | Priority | When Needed | Key Skills |
|------|----------|-------------|------------|
| **Backend Engineer (Rust)** | P0 | Week 1 | Rust, Axum/Tokio, PostgreSQL, async programming |
| **Backend Engineer (Rust)** | P0 | Week 1 | Rust, API design, testing, SQLx |
| **Frontend Engineer** | P0 | Week 2 | Next.js 14+, TypeScript, Tailwind, TanStack |
| **DevOps Engineer** | P1 | Week 1 | Docker, Kubernetes, GitHub Actions, monitoring |
| **ML/AI Contractor** | P2 | Week 3 | OCR, document processing, Python/Rust |
| **Product Manager** | P1 | Week 1 | B2B SaaS, customer development, AP domain |

### 5.3 Infrastructure Costs (Monthly)

| Component | Development | Production (5 Pilots) | Notes |
|-----------|------------:|----------------------:|-------|
| Cloud Compute (EKS/GKE) | $200 | $800 | 2x t3.medium dev, 3x t3.large prod |
| PostgreSQL (RDS) | $50 | $300 | db.t3.small dev, db.t3.medium prod |
| Redis (ElastiCache) | $20 | $100 | cache.t3.micro dev, cache.t3.small prod |
| S3/MinIO Storage | $10 | $50 | ~100GB initial |
| OCR (Textract backup) | $0 | $200 | ~20K pages/month at $0.01 |
| Email (SES/Resend) | $0 | $50 | ~10K emails/month |
| Monitoring (Grafana Cloud) | $0 | $100 | Free tier → paid |
| Domain + SSL | $10 | $10 | billforge.io |
| **TOTAL** | **$290/mo** | **$1,610/mo** | |

### 5.4 Development Tools (Per User/Month)

| Tool | Cost | Purpose |
|------|-----:|---------|
| GitHub Team | $4 | Source control, CI/CD |
| Linear | $8 | Issue tracking |
| Figma | $15 | Design |
| Vercel | $20 | Frontend preview deploys |
| Posthog | Free | Product analytics |
| Sentry | Free | Error tracking |
| Slack | $8.75 | Team communication |

---

## 6. Monorepo Structure

```
bill-forge/
├── Cargo.toml                      # Rust workspace root
├── Cargo.lock
├── package.json                    # pnpm workspace root
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── docker-compose.yml              # Local development
├── docker-compose.prod.yml         # Production template
├── .env.example                    # Environment template
├── README.md
│
├── .github/
│   └── workflows/
│       ├── ci.yml                  # PR: lint, test, build
│       ├── deploy-staging.yml      # Auto-deploy to staging
│       └── deploy-prod.yml         # Manual prod deploy
│
├── crates/                         # Rust backend crates
│   ├── bf-api/                     # API gateway (Axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── health.rs
│   │       │   ├── invoices.rs
│   │       │   ├── workflows.rs
│   │       │   ├── vendors.rs
│   │       │   └── actions.rs      # Email approval actions
│   │       ├── middleware/
│   │       │   ├── mod.rs
│   │       │   ├── auth.rs
│   │       │   ├── tenant.rs
│   │       │   └── rate_limit.rs
│   │       └── error.rs
│   │
│   ├── bf-invoice/                 # Invoice capture service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs
│   │       ├── handlers.rs
│   │       ├── extraction.rs
│   │       └── queue.rs
│   │
│   ├── bf-workflow/                # Approval workflow engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rules.rs            # Rule engine
│   │       ├── state_machine.rs    # Approval states
│   │       ├── notifications.rs    # Email sending
│   │       └── audit.rs
│   │
│   ├── bf-ocr/                     # OCR provider abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── provider.rs         # Trait definition
│   │       ├── tesseract.rs
│   │       ├── textract.rs
│   │       ├── vision.rs
│   │       └── router.rs           # Provider selection logic
│   │
│   ├── bf-vendor/                  # Vendor management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs
│   │       ├── handlers.rs
│   │       └── matching.rs         # Fuzzy matching
│   │
│   ├── bf-storage/                 # S3/MinIO abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── s3.rs
│   │
│   ├── bf-auth/                    # Authentication
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── jwt.rs
│   │       ├── api_key.rs
│   │       └── password.rs
│   │
│   ├── bf-tenant/                  # Tenant management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs
│   │       ├── handlers.rs
│   │       └── provisioning.rs     # DB creation
│   │
│   ├── bf-analytics/               # DuckDB analytics
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── queries.rs
│   │
│   └── bf-common/                  # Shared types, utilities
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── types.rs            # UUID, Money, etc.
│           ├── config.rs
│           └── error.rs
│
├── apps/                           # Frontend applications
│   └── web/                        # Next.js main app
│       ├── package.json
│       ├── next.config.mjs
│       ├── tailwind.config.ts
│       ├── tsconfig.json
│       ├── components.json         # shadcn/ui config
│       └── src/
│           ├── app/                # App Router pages
│           │   ├── layout.tsx
│           │   ├── page.tsx        # Landing/login
│           │   ├── (auth)/
│           │   │   ├── login/
│           │   │   └── signup/
│           │   ├── (dashboard)/
│           │   │   ├── layout.tsx
│           │   │   ├── invoices/
│           │   │   │   ├── page.tsx        # Invoice list
│           │   │   │   ├── upload/
│           │   │   │   └── [id]/           # Invoice detail
│           │   │   ├── approvals/
│           │   │   │   └── page.tsx        # Approval inbox
│           │   │   ├── vendors/
│           │   │   │   ├── page.tsx
│           │   │   │   └── [id]/
│           │   │   ├── workflows/
│           │   │   │   └── page.tsx        # Rule config
│           │   │   └── reports/
│           │   │       └── page.tsx
│           │   └── api/            # Next.js API routes (if needed)
│           ├── components/
│           │   ├── ui/             # shadcn/ui components
│           │   ├── invoices/
│           │   │   ├── upload-zone.tsx
│           │   │   ├── ocr-results.tsx
│           │   │   └── correction-form.tsx
│           │   ├── approvals/
│           │   │   ├── inbox-table.tsx
│           │   │   └── action-buttons.tsx
│           │   ├── layout/
│           │   │   ├── sidebar.tsx
│           │   │   └── header.tsx
│           │   └── shared/
│           │       ├── data-table.tsx
│           │       └── confidence-badge.tsx
│           └── lib/
│               ├── api-client.ts   # Generated from OpenAPI
│               ├── auth.ts
│               └── utils.ts
│
├── packages/                       # Shared JS packages
│   ├── ui/                         # Extended shadcn/ui
│   │   ├── package.json
│   │   └── src/
│   └── api-client/                 # Generated TypeScript client
│       ├── package.json
│       └── src/
│
├── services/                       # Additional services
│   └── winston/                    # AI assistant (Phase 3)
│       ├── pyproject.toml          # Python (adapted from Locust)
│       └── src/
│           └── winston/
│               ├── agents/         # Adapted from Locust
│               ├── tools/          # Bill Forge domain tools
│               └── api.py
│
├── migrations/                     # Database migrations
│   ├── control-plane/              # Control plane schema
│   │   ├── 001_create_tenants.sql
│   │   ├── 002_create_users.sql
│   │   ├── 003_create_api_keys.sql
│   │   └── 004_create_subscriptions.sql
│   └── tenant/                     # Per-tenant schema
│       ├── 001_create_vendors.sql
│       ├── 002_create_invoices.sql
│       ├── 003_create_line_items.sql
│       ├── 004_create_workflows.sql
│       ├── 005_create_approval_steps.sql
│       └── 006_create_audit_log.sql
│
├── infra/                          # Infrastructure as code
│   ├── terraform/
│   │   ├── environments/
│   │   │   ├── staging/
│   │   │   └── production/
│   │   └── modules/
│   │       ├── eks/
│   │       ├── rds/
│   │       └── s3/
│   └── kubernetes/
│       ├── base/
│       ├── staging/
│       └── production/
│
├── docs/                           # Documentation
│   ├── api/                        # OpenAPI specs
│   │   └── openapi.yaml
│   ├── architecture/               # ADRs, diagrams
│   │   ├── 001-database-per-tenant.md
│   │   ├── 002-ocr-provider-strategy.md
│   │   └── 003-email-approval-security.md
│   └── runbooks/                   # Operational guides
│       ├── incident-response.md
│       ├── tenant-provisioning.md
│       └── backup-restore.md
│
└── tests/                          # End-to-end tests
    ├── e2e/
    │   ├── invoice-capture.spec.ts
    │   └── approval-workflow.spec.ts
    └── load/
        └── locust/                 # Load testing (different from AI Locust)
            └── locustfile.py
```

---

## 7. Success Criteria

### 7.1 Technical Metrics (3-Month Horizon)

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| **OCR Accuracy** | ≥85% | Correct fields / Total fields on test set |
| **OCR Accuracy (clean PDFs)** | ≥90% | Subset of well-formatted digital PDFs |
| **Processing Latency (P95)** | <5 seconds | Upload to queue placement |
| **API Response Time (P95)** | <200ms | Non-OCR endpoints |
| **System Uptime** | ≥99.5% | Monthly availability |
| **Test Coverage** | ≥80% | Line coverage on core crates |
| **Critical Bugs** | 0 | Unresolved P0 issues in production |
| **Security Vulnerabilities** | 0 Critical/High | SAST/DAST scan results |

### 7.2 Business Metrics (3-Month Horizon)

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 1,000+ | Total across all pilots |
| **Customer NPS** | ≥50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | ≥60% | "Would you pay?" conversion |
| **Email Approval Success** | ≥95% | Successful actions / Total emails |

### 7.3 Operational Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| **Deployment Frequency** | Daily | Successful deploys to staging |
| **Mean Time to Recovery** | <1 hour | Incident detection to resolution |
| **Change Failure Rate** | <15% | Deploys requiring rollback |

---

## 8. Answers to CEO Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette/Rillion Strengths:**
- Established presence in Nordics/Europe with 20+ years in market
- Deep SAP and Oracle integrations built over time
- Mature workflow engine handling complex multi-entity scenarios
- Existing customer base provides stability proof

**Palette/Rillion Weaknesses (Our Opportunities):**
- **UI/UX**: Described as "slow" and "clunky" in customer reviews
- **Innovation**: Limited AI/ML advancement in recent years
- **Pricing**: Opaque "call for quote" model, expensive licensing
- **Mobile**: Poor to non-existent mobile experience
- **Support**: Slow, impersonal support processes

**Bill Forge Differentiation Strategy:**

| Dimension | Palette | Bill Forge | Our Advantage |
|-----------|---------|------------|---------------|
| **UI Speed** | Multi-second loads | Sub-second | 10x faster experience |
| **Setup Time** | Weeks/months | Hours/days | Self-service onboarding |
| **Pricing** | "Call for quote" | Published, transparent | Trust and predictability |
| **OCR** | Cloud-only | Local-first option | Privacy for sensitive industries |
| **Approvals** | Login required | Email actions (no login) | Frictionless for approvers |
| **AI** | Limited/none | Winston assistant | Natural language queries |

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence Score | Queue | Action | Rationale |
|------------------|-------|--------|-----------|
| **≥85%** | AP Queue | Auto-route to approval workflow | High confidence, proceed without human review |
| **70-84%** | Review Queue | Human verifies flagged fields only | Medium confidence, targeted intervention |
| **<70%** | Error Queue | Full manual data entry required | Low confidence, OCR unreliable |

**Implementation Notes:**
- Calculate overall confidence as weighted average of field confidences
- Weight amount and vendor_name higher (critical for routing/matching)
- Store per-field confidence for granular review UI
- Collect corrections as training data for future accuracy improvements

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Rationale | Complexity | Timeline |
|-----|----------|-----------|------------|----------|
| **QuickBooks Online** | 1 | Largest mid-market share, REST API, OAuth 2.0, excellent docs | Low | 2-3 weeks |
| **NetSuite** | 2 | Common in growing companies, SuiteScript API | Medium | 4-6 weeks |
| **Sage Intacct** | 3 | Strong in manufacturing, REST API | Medium | 4-6 weeks |
| **Dynamics 365** | 4 | Microsoft ecosystem | High | 6-8 weeks |

**Why QuickBooks First:**
- Largest addressable market for 10-1000 employee companies
- Simplest API with best documentation
- Fast time-to-integration (2-3 weeks)
- Enables QuickBooks ProAdvisor partnership channel

### Q4: What approval workflow patterns are most common in mid-market companies?

**Research-Based Patterns (from competitor analysis and customer interviews):**

1. **Amount-Based Tiers (85% of companies)** - **MVP Priority**
   ```
   < $5,000:      Auto-approve (if vendor known)
   $5K - $25K:    Manager approval
   $25K - $50K:   Director/VP approval
   > $50K:        CFO/Controller approval
   ```

2. **Exception-Only Review (65%)** - **MVP Priority**
   - Clean invoices (match PO, known vendor) → auto-approve
   - Exceptions (no PO, new vendor, variance) → review queue

3. **Department/Cost Center Routing (45%)** - **Phase 2**
   - Route to cost center owner regardless of amount
   - Finance has override/visibility on all

4. **Dual Approval (30%)** - **Phase 2**
   - Two approvers required above threshold
   - Common in regulated industries (healthcare, finance)

**MVP Implementation:** Amount-based tiers + exception routing

### Q5: How do competitors handle multi-currency and international invoices?

**Common Competitor Approaches:**
- Store original currency alongside converted base currency amount
- Daily exchange rate sync from ECB or Open Exchange Rates API
- Allow manual rate override for specific invoices
- Display both currencies in UI

**Recommendation for MVP:**
- Support `currency` field in extraction (USD, EUR, GBP, CAD)
- Convert to tenant's base currency for totals and reporting
- Use Open Exchange Rates API (free tier: 1,000 requests/month)
- Store both original and converted amounts
- **Defer full multi-currency GL posting to Phase 2**

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommendation: Tiered Usage-Based Pricing**

| Tier | Monthly Base | Invoices Included | Overage | Target Customer |
|------|--------------|-------------------|---------|-----------------|
| **Starter** | $299 | 500 | $0.75/invoice | Small teams, testing |
| **Growth** | $799 | 2,000 | $0.50/invoice | Growing mid-market |
| **Scale** | $1,999 | 10,000 | $0.30/invoice | Larger mid-market |
| **Enterprise** | Custom | Custom | Custom | 10K+ invoices |

**Why This Model:**
- **No per-seat pricing**: AP teams hate paying for each approver
- **Predictable base**: Finance can budget effectively
- **Scales with business**: Aligned with value delivered
- **Transparent**: Published pricing builds trust vs "call for quote"

**Module Add-Ons (Phase 2+):**
- Vendor Management: +$199/month
- Advanced Reporting: +$299/month
- Winston AI: +$299/month
- NetSuite Integration: +$199/month

---

## 9. Winston AI Strategy (Leveraging Locust)

### 9.1 What to Reuse from Locust

The existing Locust codebase (`/Users/mark/sentinel/locust`) contains a sophisticated LangGraph-based agent framework. Key components to adapt:

**Keep and Adapt:**
| Locust Component | Path | Adaptation for Winston |
|------------------|------|------------------------|
| Agent base classes | `src/locust/agents/base.py` | Simplify for single-agent, remove tiers |
| LLM backend switching | `src/locust/llm/` | Keep Claude + Ollama support |
| Workflow state machine | `src/locust/workflows/state.py` | Adapt for query/action pattern |
| Memory/embeddings | `src/locust/memory/` | Use for semantic invoice search |
| Checkpoint/resume | `src/locust/ceo/checkpoint.py` | Adapt for conversation recovery |

**Remove:**
- Software development agents (CTO, CPO, Engineering Manager, etc.)
- Code generation modules
- Research workflows (web search)
- Git integration

### 9.2 Winston Tool Design

Winston will have access to Bill Forge domain tools:

```python
# Example Winston tools (adapted from Locust MCP pattern)

@tool
async def search_invoices(
    query: str,
    tenant_id: str,
    limit: int = 10
) -> list[Invoice]:
    """Search invoices by vendor name, amount, or status.

    Examples:
    - "invoices from Acme Corp"
    - "pending invoices over $10,000"
    - "invoices due this week"
    """
    pass

@tool
async def get_approval_status(
    invoice_id: str,
    tenant_id: str
) -> ApprovalStatus:
    """Get the current approval status of an invoice."""
    pass

@tool
async def list_pending_approvals(
    user_id: str,
    tenant_id: str
) -> list[PendingApproval]:
    """List all invoices pending the user's approval."""
    pass

@tool
async def vendor_lookup(
    search: str,
    tenant_id: str
) -> list[Vendor]:
    """Search vendors by name or tax ID."""
    pass

@tool
async def run_report(
    report_type: str,
    date_range: DateRange,
    tenant_id: str
) -> Report:
    """Run a spending or processing report.

    Report types: spend_by_vendor, spend_by_department, processing_metrics
    """
    pass
```

### 9.3 Winston Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           WINSTON AI ASSISTANT                               │
│                                                                              │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         CHAT INTERFACE                               │   │
│   │                                                                      │   │
│   │   User: "Show me all pending invoices from Acme"                    │   │
│   │                                                                      │   │
│   └───────────────────────────────────┬─────────────────────────────────┘   │
│                                       │                                      │
│                                       ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                      LANGGRAPH AGENT                                 │   │
│   │                  (Adapted from Locust)                               │   │
│   │                                                                      │   │
│   │   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐              │   │
│   │   │   Parse     │──►│   Plan      │──►│   Execute   │              │   │
│   │   │   Intent    │   │   Tools     │   │   Actions   │              │   │
│   │   └─────────────┘   └─────────────┘   └─────────────┘              │   │
│   │                                                                      │   │
│   └───────────────────────────────────┬─────────────────────────────────┘   │
│                                       │                                      │
│                                       ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                     BILL FORGE TOOLS                                 │   │
│   │                                                                      │   │
│   │   search_invoices │ get_approval_status │ list_pending_approvals   │   │
│   │   vendor_lookup   │ run_report          │ approve_invoice           │   │
│   │                                                                      │   │
│   └───────────────────────────────────┬─────────────────────────────────┘   │
│                                       │                                      │
│                                       ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                     BILL FORGE API                                   │   │
│   │              (Authenticated as system user)                          │   │
│   │                                                                      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 9.4 Winston Timeline

**Phase 3+ (Post-MVP):** ~3 weeks to adapt Locust architecture

| Week | Focus |
|------|-------|
| 1 | Fork Locust agent core, strip unused components |
| 2 | Implement Bill Forge tools, integrate with API |
| 3 | Chat UI, testing, tenant isolation |

**Estimated Effort Savings:** 60% reduction vs building from scratch (Locust provides LLM abstraction, state management, error handling)

---

## 10. Immediate Next Steps

### This Week (Week 0)

**Day 1-2: Repository Setup**
- [ ] Create `bill-forge` repository
- [ ] Initialize Cargo workspace with `bf-common`, `bf-api`
- [ ] Initialize pnpm workspace with Next.js app
- [ ] Configure Docker Compose (PostgreSQL, Redis, MinIO)

**Day 3-4: CI/CD Pipeline**
- [ ] GitHub Actions: Rust lint (clippy), test, build
- [ ] GitHub Actions: TypeScript lint, build
- [ ] Pre-commit hooks: format, lint

**Day 5: Foundation Crates**
- [ ] `bf-common`: UUID types, config, error types
- [ ] `bf-api`: Axum scaffold with health endpoint
- [ ] `bf-tenant`: Tenant model, control plane schema

### Week 1 Deliverables

| Deliverable | Owner | Status |
|-------------|-------|--------|
| Monorepo initialized | DevOps | [ ] |
| Docker Compose running | DevOps | [ ] |
| `bf-api` health check working | Backend | [ ] |
| `bf-tenant` creates tenant databases | Backend | [ ] |
| CI pipeline passing on main | DevOps | [ ] |
| Next.js app with shadcn/ui placeholder | Frontend | [ ] |

### Hiring Actions (This Week)

- [ ] Post job descriptions for 2x Rust Backend Engineers
- [ ] Schedule interviews for Next.js Frontend Engineer
- [ ] Identify DevOps contractor for infrastructure setup
- [ ] Reach out to ML/AI contractors for OCR optimization

---

## Appendix A: Architecture Decision Records (ADRs)

### ADR-001: Database-per-Tenant Isolation

**Status:** Accepted
**Context:** Multi-tenant data isolation strategy
**Decision:** Use database-per-tenant model (not row-level security)
**Consequences:**
- (+) Complete data isolation for compliance
- (+) Per-tenant backup/restore
- (+) Easy data portability
- (-) Higher connection overhead
- (-) More complex migrations

### ADR-002: OCR Provider Strategy

**Status:** Accepted
**Context:** Balance privacy, cost, and accuracy
**Decision:** Tesseract 5 as default, cloud providers for escalation
**Consequences:**
- (+) Privacy-first positioning
- (+) Low cost for high-confidence invoices
- (-) Slightly lower accuracy than cloud-only

### ADR-003: Email Approval Security

**Status:** Accepted
**Context:** One-click approval without login
**Decision:** HMAC-signed tokens with 72h expiration, one-time use
**Consequences:**
- (+) Frictionless approver experience
- (+) Works on mobile without app
- (-) Tokens can be forwarded (mitigated by audit logging)

---

**Document History:**

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-31 | CTO | Initial strategic plan |
| 2.0 | 2026-01-31 | CTO | Comprehensive revision with detailed architecture |

---

*This strategic plan is a living document. Updates will be made based on pilot customer feedback, technical learnings, and market conditions.*
