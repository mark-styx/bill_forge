# Bill Forge - CTO Strategic Technical Plan

**Date:** January 31, 2026
**Version:** 1.0
**Author:** CTO
**Status:** Final

---

## Executive Summary

Bill Forge will be a **new greenfield build** using the CEO's preferred stack (Rust/Axum backend, Next.js frontend). The existing Locust codebase is an AI agent orchestration framework that serves a completely different purpose. However, Locust's agent architecture can be adapted later (Phase 3+) for the Winston AI Assistant.

**Key Strategic Decisions:**
1. **Clean start** - Build Bill Forge from scratch in Rust + Next.js
2. **Database-per-tenant** - Complete data isolation for mid-market compliance
3. **Local-first OCR** - Tesseract as default, cloud providers for fallback
4. **Modular monorepo** - Cargo workspace + pnpm for unified development
5. **Winston reuse** - Adapt Locust's LangGraph agents for AI assistant (Phase 3)

---

## 1. Technical Architecture Recommendations

### 1.1 System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           BILL FORGE PLATFORM                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                         PRESENTATION LAYER                             │ │
│  │                                                                        │ │
│  │   Next.js 14+ (App Router)                                            │ │
│  │   ├── Invoice Capture UI (upload, preview, correction)                │ │
│  │   ├── Approval Workflow UI (inbox, actions, delegation)               │ │
│  │   ├── Vendor Management UI (master data, tax docs)                    │ │
│  │   ├── Analytics Dashboard (metrics, spend, reports)                   │ │
│  │   └── Winston Chat Interface (Phase 3)                                │ │
│  │                                                                        │ │
│  │   Stack: TypeScript, Tailwind CSS, shadcn/ui, TanStack Query          │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                         │
│                                    ▼                                         │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                           API GATEWAY                                  │ │
│  │                         (bf-api crate)                                 │ │
│  │                                                                        │ │
│  │   Axum 0.7+ with Tower middleware                                     │ │
│  │   ├── JWT + API Key authentication                                    │ │
│  │   ├── Tenant resolution from URL path (/api/v1/{tenant}/...)          │ │
│  │   ├── Rate limiting (per-tenant, configurable)                        │ │
│  │   ├── Request validation (validator crate)                            │ │
│  │   └── OpenTelemetry tracing                                           │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                         │
│         ┌──────────────────────────┼──────────────────────────┐             │
│         ▼                          ▼                          ▼             │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐         │
│  │   INVOICE SVC   │    │  WORKFLOW SVC   │    │   VENDOR SVC    │         │
│  │  (bf-invoice)   │    │  (bf-workflow)  │    │   (bf-vendor)   │         │
│  ├─────────────────┤    ├─────────────────┤    ├─────────────────┤         │
│  │ • Upload API    │    │ • Rule engine   │    │ • Master data   │         │
│  │ • OCR pipeline  │    │ • State machine │    │ • Tax documents │         │
│  │ • Extraction    │    │ • Email actions │    │ • Fuzzy match   │         │
│  │ • Confidence    │    │ • SLA tracking  │    │ • Spend analysis│         │
│  │ • Queue routing │    │ • Audit trail   │    │ • Onboarding    │         │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘         │
│         │                          │                          │             │
│         └──────────────────────────┼──────────────────────────┘             │
│                                    │                                         │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                           OCR SERVICE                                  │ │
│  │                          (bf-ocr crate)                                │ │
│  │                                                                        │ │
│  │   Provider Abstraction Layer                                          │ │
│  │   ├── Tesseract 5 (local, default, privacy-first)                     │ │
│  │   ├── AWS Textract (cloud, high accuracy)                             │ │
│  │   └── Google Vision (fallback, handwriting)                           │ │
│  │                                                                        │ │
│  │   Pipeline: Ingest → Preprocess → OCR → Extract → Validate → Route    │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                         ANALYTICS SERVICE                              │ │
│  │                       (bf-analytics crate)                             │ │
│  │                                                                        │ │
│  │   DuckDB per-tenant embedded analytics                                 │ │
│  │   ├── Real-time dashboard queries                                      │ │
│  │   ├── Processing metrics (volume, accuracy, cycle time)                │ │
│  │   ├── Spend analysis (vendor, cost center, GL)                        │ │
│  │   └── Export engine (CSV, Excel, API)                                  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                              DATA LAYER                                      │
│                                                                              │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐          │
│  │   PostgreSQL 16  │  │     DuckDB       │  │  S3-Compatible   │          │
│  │  (Per-Tenant)    │  │   (Analytics)    │  │    (MinIO)       │          │
│  │                  │  │                  │  │                  │          │
│  │ • invoices       │  │ • metrics tables │  │ • /tenant/docs/  │          │
│  │ • vendors        │  │ • aggregates     │  │ • /tenant/tax/   │          │
│  │ • workflows      │  │ • time series    │  │ • /tenant/temp/  │          │
│  │ • audit_log      │  │                  │  │                  │          │
│  │ • users          │  │                  │  │                  │          │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘          │
│                                                                              │
│  ┌──────────────────┐  ┌──────────────────┐                                │
│  │  Control Plane   │  │      Redis       │                                │
│  │   PostgreSQL     │  │                  │                                │
│  │                  │  │ • Sessions       │                                │
│  │ • tenants        │  │ • Rate limits    │                                │
│  │ • subscriptions  │  │ • Job queues     │                                │
│  │ • billing        │  │ • Pub/sub        │                                │
│  └──────────────────┘  └──────────────────┘                                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Tenant Isolation Strategy

**Database-per-tenant** is the chosen isolation model:

```
┌─────────────────────────────────────────────────────────────────┐
│                      CONTROL PLANE                               │
│                                                                  │
│   ┌───────────────────────────────────────────────────────────┐ │
│   │              control_plane_db (PostgreSQL)                │ │
│   │                                                           │ │
│   │   tenants: id, slug, name, db_name, modules, settings    │ │
│   │   users: id, tenant_id, email, role                      │ │
│   │   api_keys: id, tenant_id, key_hash, permissions         │ │
│   │   subscriptions: id, tenant_id, tier, modules, billing   │ │
│   └───────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                    Tenant Resolution                             │
│                 (URL path or API key lookup)                     │
│                              │                                   │
└──────────────────────────────┼───────────────────────────────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        ▼                      ▼                      ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐
│   acme_corp   │      │  techstart    │      │  mfg_inc      │
│  (tenant DB)  │      │  (tenant DB)  │      │  (tenant DB)  │
├───────────────┤      ├───────────────┤      ├───────────────┤
│ • invoices    │      │ • invoices    │      │ • invoices    │
│ • vendors     │      │ • vendors     │      │ • vendors     │
│ • workflows   │      │ • workflows   │      │ • workflows   │
│ • audit_log   │      │ • audit_log   │      │ • audit_log   │
└───────────────┘      └───────────────┘      └───────────────┘
        │                      │                      │
        ▼                      ▼                      ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐
│ S3: /acme/    │      │ S3: /tech/    │      │ S3: /mfg/     │
│ DuckDB: acme  │      │ DuckDB: tech  │      │ DuckDB: mfg   │
└───────────────┘      └───────────────┘      └───────────────┘
```

**Rationale:**
- Complete data isolation (regulatory compliance for mid-market)
- Independent backup/restore per tenant
- Easy data portability (tenant can export their database)
- No complex row-level security policies
- Tradeoff: Higher connection overhead (mitigated by connection pooling per tenant)

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         OCR PIPELINE                                 │
│                                                                      │
│   ┌──────────────┐                                                  │
│   │    INGEST    │  Accept: PDF, PNG, JPG, TIFF, email attachments │
│   │              │  Validate: file type, size (<25MB), malware scan │
│   └──────┬───────┘                                                  │
│          ▼                                                          │
│   ┌──────────────┐                                                  │
│   │  PREPROCESS  │  • Deskew (straighten tilted scans)             │
│   │              │  • Enhance (contrast, noise reduction)           │
│   │              │  • Detect type (invoice vs receipt vs other)     │
│   └──────┬───────┘                                                  │
│          ▼                                                          │
│   ┌──────────────────────────────────────────────────────────────┐ │
│   │                    PROVIDER ROUTER                            │ │
│   │                                                               │ │
│   │   Tenant Privacy Mode?                                        │ │
│   │      │                                                        │ │
│   │      ├─ YES ──► Tesseract 5 (local only)                     │ │
│   │      │                                                        │ │
│   │      └─ NO ──► Primary: Tesseract 5                          │ │
│   │                  └─ If confidence < 75% ──► AWS Textract     │ │
│   │                       └─ If still < 75% ──► Google Vision    │ │
│   │                                                               │ │
│   └──────────────────────────────────────────────────────────────┘ │
│          ▼                                                          │
│   ┌──────────────┐                                                  │
│   │   EXTRACT    │  Header fields:                                  │
│   │              │    • vendor_name (+ normalized form)             │
│   │              │    • invoice_number                              │
│   │              │    • invoice_date                                │
│   │              │    • due_date                                    │
│   │              │    • total_amount                                │
│   │              │    • currency                                    │
│   │              │    • tax_amount                                  │
│   │              │                                                  │
│   │              │  Line items (Phase 1.5):                         │
│   │              │    • description, quantity, unit_price, amount  │
│   └──────┬───────┘                                                  │
│          ▼                                                          │
│   ┌──────────────┐                                                  │
│   │   VALIDATE   │  • Required field presence                       │
│   │              │  • Format validation (dates, amounts)            │
│   │              │  • Duplicate detection (invoice # + vendor)      │
│   │              │  • Vendor matching against master list           │
│   └──────┬───────┘                                                  │
│          ▼                                                          │
│   ┌──────────────────────────────────────────────────────────────┐ │
│   │                    CONFIDENCE ROUTER                          │ │
│   │                                                               │ │
│   │   Overall Confidence Score (weighted average of field scores)│ │
│   │                                                               │ │
│   │   >= 85%  ────────────────────────────►  AP QUEUE            │ │
│   │              (auto-route to approval workflow)                │ │
│   │                                                               │ │
│   │   70-84%  ────────────────────────────►  REVIEW QUEUE        │ │
│   │              (human verification of flagged fields)           │ │
│   │                                                               │ │
│   │   < 70%   ────────────────────────────►  ERROR QUEUE         │ │
│   │              (manual data entry required)                     │ │
│   │                                                               │ │
│   └──────────────────────────────────────────────────────────────┘ │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.4 Approval Workflow Engine

```
┌─────────────────────────────────────────────────────────────────────┐
│                    APPROVAL WORKFLOW ENGINE                          │
│                                                                      │
│   ┌──────────────────────────────────────────────────────────────┐  │
│   │                      RULE ENGINE                              │  │
│   │                                                               │  │
│   │   Rules stored as JSON, evaluated by Rust expression engine  │  │
│   │                                                               │  │
│   │   Example rules:                                              │  │
│   │   {                                                           │  │
│   │     "name": "Amount-Based Tiers",                            │  │
│   │     "conditions": [                                          │  │
│   │       { "if": "amount < 5000", "then": "auto_approve" },     │  │
│   │       { "if": "amount >= 5000 && amount < 25000",            │  │
│   │         "then": { "route": "manager", "level": 1 } },        │  │
│   │       { "if": "amount >= 25000 && amount < 50000",           │  │
│   │         "then": { "route": "director", "level": 2 } },       │  │
│   │       { "if": "amount >= 50000",                             │  │
│   │         "then": { "route": "cfo", "level": 3 } }             │  │
│   │     ],                                                        │  │
│   │     "exceptions": [                                           │  │
│   │       { "if": "vendor.is_new", "then": "add_review_step" },  │  │
│   │       { "if": "has_po_mismatch", "then": "exception_queue" } │  │
│   │     ]                                                         │  │
│   │   }                                                           │  │
│   └──────────────────────────────────────────────────────────────┘  │
│                               │                                      │
│                               ▼                                      │
│   ┌──────────────────────────────────────────────────────────────┐  │
│   │                    STATE MACHINE                              │  │
│   │                                                               │  │
│   │                      ┌──────────┐                            │  │
│   │                      │ PENDING  │                            │  │
│   │                      └────┬─────┘                            │  │
│   │                           │                                   │  │
│   │          ┌────────────────┼────────────────┐                 │  │
│   │          ▼                ▼                ▼                 │  │
│   │   ┌──────────┐     ┌──────────┐     ┌──────────┐            │  │
│   │   │ L1_PEND  │────►│ L2_PEND  │────►│ L3_PEND  │            │  │
│   │   └────┬─────┘     └────┬─────┘     └────┬─────┘            │  │
│   │        │                │                │                   │  │
│   │        ▼                ▼                ▼                   │  │
│   │   ┌─────────────────────────────────────────────────────┐   │  │
│   │   │                    TERMINAL STATES                   │   │  │
│   │   │  ┌──────────┐  ┌──────────┐  ┌──────────┐           │   │  │
│   │   │  │ APPROVED │  │ REJECTED │  │ ON_HOLD  │           │   │  │
│   │   │  └──────────┘  └──────────┘  └──────────┘           │   │  │
│   │   └─────────────────────────────────────────────────────┘   │  │
│   └──────────────────────────────────────────────────────────────┘  │
│                               │                                      │
│                               ▼                                      │
│   ┌──────────────────────────────────────────────────────────────┐  │
│   │                 NOTIFICATION SERVICE                          │  │
│   │                                                               │  │
│   │   Email Approvals (Key Differentiator):                      │  │
│   │   ┌─────────────────────────────────────────────────────┐   │  │
│   │   │  Subject: [Bill Forge] Invoice #INV-001 needs approval  │  │
│   │   │                                                         │  │
│   │   │  Vendor: Acme Corp                                      │  │
│   │   │  Amount: $12,500.00                                     │  │
│   │   │  Due: Feb 15, 2026                                      │  │
│   │   │                                                         │  │
│   │   │  [APPROVE]  [REJECT]  [VIEW DETAILS]                   │  │
│   │   │                                                         │  │
│   │   │  Links are signed tokens, expire in 72 hours           │  │
│   │   │  No login required to approve/reject                    │  │
│   │   └─────────────────────────────────────────────────────┘   │  │
│   │                                                               │  │
│   │   Other notifications:                                        │  │
│   │   • In-app notification bell                                 │  │
│   │   • SLA escalation alerts (approaching deadline)             │  │
│   │   • Delegation auto-routing (out-of-office)                  │  │
│   └──────────────────────────────────────────────────────────────┘  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | Axum | 0.7+ | CEO preference, async-first, tower ecosystem |
| Async Runtime | Tokio | 1.x | Industry standard, required by Axum |
| Serialization | Serde + serde_json | Latest | De facto Rust standard |
| Database | SQLx | 0.7+ | Compile-time checked queries, async-native |
| Migrations | sqlx-cli | 0.7+ | Integrated with SQLx |
| Validation | validator | Latest | Derive macros for request validation |
| Error Handling | thiserror + anyhow | Latest | Structured API errors + flexible internal |
| Logging | tracing + tracing-subscriber | Latest | Structured logging, OpenTelemetry export |
| Config | config-rs | Latest | Multi-source (env, files, defaults) |
| HTTP Client | reqwest | Latest | For OCR API calls, integrations |
| Testing | tokio-test + wiremock | Latest | Async test support, HTTP mocking |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | Next.js | 14+ | CEO preference, App Router, RSC |
| Language | TypeScript | 5.x | Strict mode enabled |
| Styling | Tailwind CSS | 3.x | CEO preference |
| Components | shadcn/ui | Latest | CEO preference, consistent design |
| State | TanStack Query | 5.x | Server state caching, optimistic updates |
| Forms | React Hook Form + Zod | Latest | Type-safe validation |
| Tables | TanStack Table | 8.x | Invoice list data grids |
| Charts | Recharts | 2.x | Analytics dashboards |
| Auth | NextAuth.js | 4.x | Session management |
| API Client | Generated from OpenAPI | - | Type-safe API calls |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | PostgreSQL 16+ | CEO preference, per-tenant isolation, JSONB |
| Analytics | DuckDB | CEO preference, embedded, fast aggregations |
| Document Storage | MinIO (S3-compatible) | CEO preference, local-first dev |
| Cache/Queue | Redis 7+ | Sessions, rate limiting, job queues |
| Search | PostgreSQL Full-Text + pg_trgm | Start simple, add Elasticsearch later if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost |
|----------|----------|----------|------|
| Tesseract 5 | Primary (default) | Local/privacy-first, standard invoices | Free |
| AWS Textract | Secondary | High-accuracy for complex documents | ~$0.01/page |
| Google Vision | Tertiary | Handwritten notes, fallback | ~$0.0015/page |

### 2.5 Infrastructure

| Component | Technology | Environment |
|-----------|------------|-------------|
| Containers | Docker | All environments |
| Dev Orchestration | Docker Compose | Development |
| Prod Orchestration | Kubernetes | Production |
| CI/CD | GitHub Actions | All environments |
| Secrets | HashiCorp Vault | Production |
| Monitoring | Prometheus + Grafana | Production |
| Tracing | OpenTelemetry + Jaeger | All environments |
| Email | AWS SES or Resend | Production |

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Establish project structure and development environment

```
Week 1: Infrastructure Setup
├─ Create monorepo (Cargo workspace + pnpm workspace)
├─ Set up Docker Compose (PostgreSQL, Redis, MinIO)
├─ Configure GitHub Actions CI (lint, test, build)
├─ Implement control plane database schema
├─ Create tenant provisioning service (bf-tenant crate)
└─ Set up SQLx migrations infrastructure

Week 2: Auth + API Foundation
├─ Implement JWT authentication (bf-auth crate)
├─ Create API gateway with tenant resolution (bf-api crate)
├─ Scaffold Next.js app with shadcn/ui
├─ Configure TanStack Query + API client generation
├─ Create development seed data and fixtures
└─ Document local development setup
```

**Deliverables:**
- [ ] Monorepo with `crates/` and `apps/web`
- [ ] Docker Compose with Postgres, Redis, MinIO running
- [ ] `bf-api` service with health check endpoint
- [ ] `bf-tenant` service can create/list tenants
- [ ] `bf-auth` service handles JWT issue/verify
- [ ] Next.js app displays login page with shadcn/ui
- [ ] CI pipeline runs on every PR

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with manual review capability

```
Week 3-4: OCR Pipeline
├─ Implement document upload API (bf-invoice crate)
├─ Integrate Tesseract 5 for local OCR (bf-ocr crate)
├─ Build field extraction (vendor, invoice #, amount, date)
├─ Implement confidence scoring system
├─ Create S3 storage abstraction (bf-storage crate)
├─ Design AP Queue and Error Queue data model
└─ Unit tests for extraction accuracy

Week 5-6: Capture UI + Vendor Matching
├─ Build invoice upload UI (drag-drop, preview)
├─ Create OCR results review interface
├─ Implement manual correction with field highlighting
├─ Add vendor fuzzy matching logic
├─ Build basic vendor CRUD API
├─ Create queue dashboard (AP queue, error queue)
└─ Integration tests for full pipeline
```

**Deliverables:**
- [ ] Upload API: `POST /api/v1/{tenant}/invoices/upload`
- [ ] Get invoice: `GET /api/v1/{tenant}/invoices/{id}`
- [ ] List queues: `GET /api/v1/{tenant}/queues/ap`, `/queues/errors`
- [ ] Tesseract OCR extracts: vendor, invoice #, amount, date
- [ ] Confidence scores displayed with visual indicators
- [ ] Manual correction UI updates invoice data
- [ ] Vendor matching suggests existing vendors
- [ ] 85%+ accuracy on clean PDF test set

**Success Metrics:**
- OCR accuracy: ≥85% on standard invoices
- Processing time: <3 seconds per invoice
- Manual correction reduces errors: ≥95%

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Objective:** Approval workflows with email actions

```
Week 7-8: Workflow Engine
├─ Design workflow rule engine (bf-workflow crate)
├─ Implement approval state machine
├─ Build rule configuration API
├─ Create approval inbox UI
├─ Implement approve/reject/hold actions
├─ Add workflow to queue routing integration
└─ Unit tests for rule evaluation

Week 9-10: Email Actions + Audit
├─ Implement signed token generation
├─ Build email approval endpoints (no auth required)
├─ Integrate email sending (SES/Resend)
├─ Create delegation configuration UI
├─ Build SLA tracking with escalation alerts
├─ Implement complete audit trail logging
├─ Create bulk operations (batch approve)
└─ End-to-end tests for approval flow
```

**Deliverables:**
- [ ] Workflow API: Create/update approval rules
- [ ] Approval endpoints: `POST /api/v1/{tenant}/invoices/{id}/approve`
- [ ] Email actions: `GET /api/v1/actions/{token}/approve` (no auth)
- [ ] Delegation: Set out-of-office routing
- [ ] SLA dashboard: Time in queue, approaching deadlines
- [ ] Audit log: Every action recorded with actor, timestamp, IP

**Success Metrics:**
- Approval action latency: <5 seconds
- Email approval success rate: ≥95%
- Audit coverage: 100% of actions logged

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Deploy to 5 pilot customers

```
Week 11: Production Readiness
├─ Production environment setup (Kubernetes)
├─ Security audit and penetration testing
├─ Load testing (target: 100 invoices/minute)
├─ Configure monitoring and alerting
├─ Create API documentation (OpenAPI)
├─ Write user guides and help content
└─ Establish support runbook

Week 12: Customer Onboarding
├─ Pilot customer data migration tooling
├─ White-glove onboarding support
├─ Feedback collection mechanisms
├─ Bug triage and hotfix process
├─ Weekly check-in schedule
└─ Success metrics tracking
```

**Deliverables:**
- [ ] Production deployment on cloud infrastructure
- [ ] Security audit passed
- [ ] Load test: 100 invoices/minute sustained
- [ ] 5 pilot customers onboarded
- [ ] API docs published
- [ ] Support runbook documented

### Phase Timeline Summary

```
┌────────────────────────────────────────────────────────────────────────────┐
│                      12-WEEK DEVELOPMENT TIMELINE                           │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Week:  1   2   3   4   5   6   7   8   9   10  11  12                   │
│          ├───┴───┼───┴───┴───┴───┼───┴───┴───┴───┼───┴───┤                │
│          │       │               │               │       │                │
│          │ P0    │     P1        │      P2       │  P3   │                │
│          │Found- │  Invoice      │   Invoice     │Pilot  │                │
│          │ation  │  Capture      │  Processing   │Launch │                │
│          │       │               │               │       │                │
│          ▼       ▼               ▼               ▼       ▼                │
│                                                                             │
│   Milestones:                                                              │
│   • Week 2:  Auth + API scaffolding complete                              │
│   • Week 4:  OCR pipeline functional                                       │
│   • Week 6:  Invoice Capture MVP complete                                  │
│   • Week 8:  Workflow engine functional                                    │
│   • Week 10: Invoice Processing MVP complete                               │
│   • Week 12: 5 pilot customers live                                        │
│                                                                             │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **OCR accuracy below 90%** | Medium | High | Multi-provider fallback chain; human-in-loop for low confidence; collect training data from corrections |
| **Rust learning curve slows development** | Medium | Medium | Pair programming on complex features; comprehensive code reviews; consider Go for non-critical services if needed |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant eliminates cross-tenant queries; penetration testing before launch; add RLS as defense-in-depth |
| **Email approval token security** | Medium | High | HMAC-signed tokens with 72-hour expiration; one-time use; rate limiting on action endpoints; IP logging |
| **DuckDB scalability limits** | Medium | Medium | Partition by month; archive data >12 months; evaluate ClickHouse/TimescaleDB if needed |
| **Connection pool exhaustion (multi-tenant)** | Medium | Medium | Per-tenant connection pools with limits; lazy tenant DB connections; monitoring alerts |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **Feature creep delays MVP** | High | High | Strict adherence to anti-goals; weekly scope reviews with stakeholders; say "Phase 2" often |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24 hour bug response; dedicated Slack channel; white-glove support |
| **ERP integration complexity** | High | Medium | Start with QuickBooks Online (simplest API); use official SDKs; defer NetSuite/Sage to Phase 2 |
| **Competitor response (BILL/Tipalti)** | Medium | Medium | Move fast; focus on UX differentiation; build switching costs through data/workflow customization |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **Data loss** | Low | Critical | Daily automated backups; point-in-time recovery enabled; cross-region replication for production |
| **Service outage** | Medium | High | Multi-AZ deployment; health checks; automatic failover; <1 hour MTTR target |
| **Key person dependency** | High | High | Document all architectural decisions; pair programming; knowledge sharing sessions; runbooks |
| **Security incident** | Low | Critical | Penetration testing before launch; security audit; incident response plan; bug bounty consideration |

---

## 5. Resource Requirements

### 5.1 Team Structure

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        BILL FORGE TEAM                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ENGINEERING (4.5 FTE)                                                 │
│   ┌─────────────────────────────────────────────────────────────────┐  │
│   │                                                                  │  │
│   │   Backend Engineer (Rust) - 2 FTE                               │  │
│   │   • bf-api, bf-invoice, bf-workflow, bf-ocr crates             │  │
│   │   • Database schema, migrations, queries                        │  │
│   │   • OCR pipeline, approval engine                               │  │
│   │                                                                  │  │
│   │   Frontend Engineer (Next.js) - 1 FTE                           │  │
│   │   • Invoice capture UI, approval inbox                          │  │
│   │   • Dashboards, analytics views                                 │  │
│   │   • Component library (shadcn/ui customization)                 │  │
│   │                                                                  │  │
│   │   Full-Stack / DevOps Engineer - 1 FTE                          │  │
│   │   • CI/CD pipeline, Docker, Kubernetes                          │  │
│   │   • Monitoring, alerting, runbooks                              │  │
│   │   • Integration work, gap filling                               │  │
│   │                                                                  │  │
│   │   ML/AI Engineer (Contract) - 0.5 FTE                           │  │
│   │   • OCR optimization, accuracy tuning                           │  │
│   │   • Winston AI adaptation (Phase 3)                             │  │
│   │                                                                  │  │
│   └─────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│   PRODUCT (1 FTE)                                                       │
│   ┌─────────────────────────────────────────────────────────────────┐  │
│   │   Product Manager - 1 FTE                                        │  │
│   │   • Pilot customer relationships                                 │  │
│   │   • Feature prioritization, roadmap                              │  │
│   │   • Feedback synthesis                                           │  │
│   └─────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│   TOTAL: 5.5 FTE for 3-month MVP                                        │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Infrastructure Costs (Monthly)

| Component | Development | Production (5 Pilots) |
|-----------|------------:|----------------------:|
| Cloud Compute (ECS/EKS) | $200 | $800 |
| PostgreSQL (RDS) | $50 | $300 |
| Redis (ElastiCache) | $20 | $100 |
| S3/MinIO Storage | $10 | $50 |
| OCR API (Textract backup) | $0 | $200 |
| Email (SES/Resend) | $0 | $50 |
| Monitoring (Grafana Cloud) | $0 | $100 |
| Domain + SSL | $10 | $10 |
| **Total** | **$290/mo** | **$1,610/mo** |

### 5.3 Development Tools

| Tool | Cost | Purpose |
|------|-----:|---------|
| GitHub Team | $4/user/mo | Source control, CI/CD |
| Linear | $8/user/mo | Issue tracking |
| Figma | $15/user/mo | Design |
| Vercel | $20/mo | Frontend hosting (dev) |
| Posthog | Free tier | Product analytics |
| Sentry | Free tier | Error tracking |

---

## 6. Monorepo Structure

```
bill-forge/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── package.json                  # pnpm workspace root
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── docker-compose.yml            # Local development
├── docker-compose.prod.yml       # Production template
├── .github/
│   └── workflows/
│       ├── ci.yml                # Lint, test, build
│       ├── deploy-staging.yml
│       └── deploy-prod.yml
│
├── crates/                       # Rust backend crates
│   ├── bf-api/                   # API gateway (Axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── routes/
│   │       ├── middleware/
│   │       └── error.rs
│   │
│   ├── bf-invoice/               # Invoice capture service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs
│   │       ├── handlers.rs
│   │       └── extraction.rs
│   │
│   ├── bf-workflow/              # Approval workflow engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rules.rs
│   │       ├── state_machine.rs
│   │       └── notifications.rs
│   │
│   ├── bf-vendor/                # Vendor management
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   ├── bf-ocr/                   # OCR provider abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tesseract.rs
│   │       ├── textract.rs
│   │       └── vision.rs
│   │
│   ├── bf-storage/               # S3/MinIO abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   ├── bf-auth/                  # Authentication/authorization
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   ├── bf-tenant/                # Tenant management
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   ├── bf-analytics/             # DuckDB analytics
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   └── bf-common/                # Shared types, utilities
│       ├── Cargo.toml
│       └── src/
│
├── apps/                         # Frontend applications
│   └── web/                      # Next.js main app
│       ├── package.json
│       ├── next.config.js
│       ├── tailwind.config.js
│       ├── tsconfig.json
│       └── src/
│           ├── app/              # App Router pages
│           │   ├── layout.tsx
│           │   ├── page.tsx
│           │   ├── (auth)/
│           │   ├── (dashboard)/
│           │   │   ├── invoices/
│           │   │   ├── approvals/
│           │   │   ├── vendors/
│           │   │   └── reports/
│           │   └── api/          # Next.js API routes (if needed)
│           ├── components/       # React components
│           │   ├── ui/           # shadcn/ui components
│           │   ├── invoices/
│           │   ├── approvals/
│           │   └── layout/
│           └── lib/              # Utilities, API client
│
├── packages/                     # Shared JS packages
│   ├── ui/                       # Extended shadcn/ui components
│   │   ├── package.json
│   │   └── src/
│   └── api-client/               # Generated TypeScript API client
│       ├── package.json
│       └── src/
│
├── services/                     # Additional services
│   └── winston/                  # AI assistant (Phase 3)
│       ├── pyproject.toml        # Python project (adapted from Locust)
│       └── src/
│
├── migrations/                   # Database migrations
│   ├── control-plane/            # Control plane schema
│   │   ├── 001_tenants.sql
│   │   └── 002_users.sql
│   └── tenant/                   # Per-tenant schema
│       ├── 001_vendors.sql
│       ├── 002_invoices.sql
│       ├── 003_workflows.sql
│       └── 004_audit_log.sql
│
├── infra/                        # Infrastructure as code
│   ├── terraform/
│   └── kubernetes/
│
└── docs/                         # Documentation
    ├── api/                      # OpenAPI specs
    ├── architecture/             # ADRs, diagrams
    └── runbooks/                 # Operational guides
```

---

## 7. Success Criteria (3-Month Horizon)

### 7.1 Technical Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| OCR Accuracy | ≥90% | (Correct fields / Total fields) on test set |
| Processing Latency (P95) | <5 seconds | From upload to queue placement |
| API Response Time (P95) | <200ms | Non-OCR endpoints |
| System Uptime | ≥99.5% | Monthly availability |
| Test Coverage | ≥80% | Line coverage on core crates |
| Critical Bugs | 0 | Unresolved P0 issues |

### 7.2 Business Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Pilot Customers | 5 | Actively using platform |
| Invoices Processed | 1,000+ | Total across all pilots |
| Customer NPS | ≥50 | Weekly survey |
| Pilot-to-Paid Intent | ≥60% | "Would you pay for this?" |

### 7.3 Operational Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Deployment Frequency | Daily | Successful deploys to staging |
| Mean Time to Recovery | <1 hour | Incident detection to resolution |
| Security Vulnerabilities | 0 Critical/High | SAST/DAST scan results |

---

## 8. Answers to CEO Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette Strengths:**
- Established presence in Nordics/Europe
- Deep SAP/Oracle integrations
- Mature workflow engine handling complex scenarios

**Palette Weaknesses (Our Opportunities):**
- UI described as "slow" and "clunky" in reviews
- Limited AI/ML innovation
- Opaque, expensive pricing
- Poor mobile experience

**Differentiation Strategy:**
1. **Speed:** Sub-second UI vs their multi-second loads
2. **Modern UX:** React/Next.js vs legacy web tech
3. **Transparent pricing:** Published rates vs "call for quote"
4. **Local OCR option:** Privacy differentiator for sensitive industries
5. **Email approvals:** No login required (unique feature)

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence | Routing | Rationale |
|------------|---------|-----------|
| ≥85% | AP Queue (auto-flow) | High confidence, proceed to approval |
| 70-84% | Review Queue | Human verifies flagged fields only |
| <70% | Error Queue | Full manual entry required |

This balances automation rate with error cost. The 85% threshold is industry-aligned; lower thresholds increase manual work, higher thresholds reject good invoices.

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Rationale |
|-----|----------|-----------|
| QuickBooks Online | 1 | Largest mid-market share, simple REST API, OAuth 2.0, excellent documentation |
| NetSuite | 2 | Common in growing companies, more complex but well-documented |
| Sage Intacct | 3 | Strong in manufacturing, defer to Phase 2 |

QuickBooks has the simplest API and largest addressable market for 10-1000 employee companies. We can have a working integration in 2-3 weeks.

### Q4: What approval workflow patterns are most common in mid-market companies?

**Top patterns from research:**

1. **Amount-Based Tiers (80% of companies)**
   - <$5K: Auto-approve or manager
   - $5K-$25K: Department head
   - $25K-$50K: Finance director
   - >$50K: CFO/Controller

2. **Exception-Only Review (60%)**
   - Auto-approve if PO matches and vendor is known
   - Route for review only on mismatch

3. **Department Routing (40%)**
   - Route to cost center owner
   - Finance approval on all > threshold

4. **Dual Approval (30%)**
   - Two approvers required above certain thresholds
   - Common in regulated industries

**MVP must support:** Amount-based tiers + exception routing. Department routing and dual approval are Phase 2.

### Q5: How do competitors handle multi-currency and international invoices?

**Common approaches:**
- Store original currency + converted base currency amount
- Daily exchange rate sync (ECB, Open Exchange Rates API)
- Allow manual rate override
- Display both currencies in UI

**Recommendation for MVP:**
- Support `currency` field in extraction
- Convert to tenant's base currency for totals/reporting
- Use Open Exchange Rates API (free tier: 1,000 requests/month)
- Defer full multi-currency GL posting to Phase 2

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommendation: Tiered Usage-Based**

| Tier | Monthly Base | Invoices Included | Overage |
|------|-------------|-------------------|---------|
| Starter | $299 | 500 | $0.75/invoice |
| Growth | $799 | 2,000 | $0.50/invoice |
| Scale | $1,999 | 10,000 | $0.30/invoice |

**Why this works:**
- **No per-seat pricing** (AP teams hate paying per approver)
- **Predictable base** (finance can budget)
- **Scales with business** (aligned with value)
- **Transparent** (builds trust vs "call for quote")

---

## 9. Winston AI Strategy (Leveraging Locust)

### What to Reuse from Locust

The existing Locust codebase contains a sophisticated LangGraph-based agent framework that can be adapted for Winston:

**Keep:**
- `src/locust/agents/` - Agent base classes and abstractions
- `src/locust/llm/` - LLM backend switching (Claude/Ollama)
- `src/locust/workflows/` - LangGraph state machine patterns
- `src/locust/memory/` - Embeddings and vector store integration

**Modify:**
- Remove software development agents (CTO, CPO, etc.)
- Add Bill Forge domain tools:
  - `invoice_search` - "Show me invoices from Acme Corp"
  - `approval_status` - "What invoices are pending my approval?"
  - `vendor_lookup` - "Find vendor with tax ID starting with 12-"
  - `report_query` - "Total spend by department last month"
- Integrate with Bill Forge APIs (authenticated as system)
- Add tenant context to all queries

### Timeline

Winston is Phase 3 (post-MVP). Estimated 2-3 weeks to adapt Locust architecture.

---

## 10. Next Steps

### Immediate Actions (This Week)

1. **Create bill-forge repository**
   - Initialize Cargo workspace
   - Initialize pnpm workspace
   - Set up Docker Compose

2. **Scaffold core crates**
   - `bf-common` - Shared types
   - `bf-tenant` - Tenant management
   - `bf-auth` - JWT handling
   - `bf-api` - Axum gateway

3. **Set up CI/CD**
   - GitHub Actions for Rust (clippy, test, build)
   - GitHub Actions for Next.js (lint, build)

4. **Design database schema**
   - Control plane tables
   - Tenant schema migrations

### Week 1 Deliverables

- [ ] Monorepo initialized with Cargo + pnpm
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] `bf-api` crate with health check endpoint
- [ ] `bf-tenant` crate can create tenant databases
- [ ] CI pipeline passing on main branch
- [ ] Next.js app with shadcn/ui displaying placeholder page

---

*This strategic plan is a living document. Updates will be made based on pilot customer feedback and technical learnings.*

**Document History:**
| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-31 | CTO | Initial strategic plan |
