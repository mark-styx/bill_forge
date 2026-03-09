# Bill Forge - CTO Strategic Technical Plan

**Version:** 3.0 (Final)
**Date:** February 1, 2026
**Author:** CTO, Bill Forge
**Status:** Approved for Execution
**Planning Horizon:** 12 Weeks (Q1 2026)

---

## Executive Summary

Bill Forge is a modular B2B SaaS platform for mid-market invoice processing automation, targeting companies with 50-500 employees who are underserved by both enterprise solutions (Coupa, SAP) and SMB tools (BILL.com).

### Current State

The repository at `/Users/mark/sentinel/locust` contains **Locust**, a fully functional multi-agent AI development framework (~30-40K lines of Python). Bill Forge itself is in the planning phase with no production code yet written.

| Aspect | CEO Vision | Locust Reality | Decision |
|--------|-----------|----------------|----------|
| Language | Rust (Axum) | Python (LangChain) | Build Bill Forge in Rust as envisioned |
| Frontend | Next.js 14+ | None (CLI only) | Build from scratch in Next.js |
| Database | PostgreSQL + DuckDB | SQLite + DuckDB | Use PostgreSQL per-tenant |
| Purpose | Invoice Processing | AI Agent Orchestration | Repurpose Locust for Winston AI |

### Strategic Recommendation: Hybrid Approach

1. **Build Bill Forge core** in Rust/Axum + Next.js 14+ (as per CEO vision)
2. **Adapt Locust's LangGraph architecture** for Winston AI Assistant (Phase 3)
3. **Database-per-tenant** architecture for complete data isolation
4. **Local-first OCR** (Tesseract 5) with cloud fallback for privacy-conscious positioning

### Key Technical Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Architecture** | Modular monolith | Start simple, split if needed |
| **Backend** | Rust + Axum 0.7+ | Performance, safety, CEO preference |
| **Frontend** | Next.js 14 App Router | Modern React, RSC, CEO preference |
| **Database** | PostgreSQL 16 (per-tenant) | Isolation + compliance |
| **Analytics** | DuckDB | Embedded, fast aggregations |
| **OCR Primary** | Tesseract 5 | Local-first, privacy |
| **OCR Fallback** | AWS Textract | Accuracy for complex docs |
| **AI Assistant** | Adapted Locust/LangGraph | Leverage existing framework |

---

## 1. Technical Architecture Recommendations

### 1.1 High-Level System Architecture

```
+--------------------------------------------------------------------------------+
|                              BILL FORGE PLATFORM                                |
+--------------------------------------------------------------------------------+
|                                                                                 |
|   +-------------------------------------------------------------------------+  |
|   |                         NEXT.JS 14+ FRONTEND                             |  |
|   |                                                                          |  |
|   |   /invoices        /approvals        /vendors         /reports          |  |
|   |   +----------+     +----------+     +----------+     +----------+       |  |
|   |   | Upload   |     |  Inbox   |     |  Master  |     |Dashboard |       |  |
|   |   | Preview  |     | Actions  |     |   Data   |     | Charts   |       |  |
|   |   | Correct  |     |   SLA    |     | Tax Docs |     | Export   |       |  |
|   |   +----------+     +----------+     +----------+     +----------+       |  |
|   |                                                                          |  |
|   |   shadcn/ui + Tailwind CSS + TanStack Query + React Hook Form           |  |
|   +-------------------------------------------------------------------------+  |
|                                      |                                          |
|                                      v                                          |
|   +-------------------------------------------------------------------------+  |
|   |                          RUST API GATEWAY                                |  |
|   |                           (bf-api crate)                                 |  |
|   |                                                                          |  |
|   |   Axum 0.7+ | JWT Auth | Tenant Resolution | Rate Limiting | CORS       |  |
|   |                                                                          |  |
|   |   Middleware: Request -> Log -> Auth -> Tenant -> RateLimit -> Handler  |  |
|   +-------------------------------------------------------------------------+  |
|                                      |                                          |
|          +---------------------------+---------------------------+              |
|          v                           v                           v              |
|   +--------------+          +--------------+          +--------------+         |
|   | bf-invoice   |          | bf-workflow  |          | bf-vendor    |         |
|   +--------------+          +--------------+          +--------------+         |
|   | Upload API   |          | Rule Engine  |          | Master Data  |         |
|   | OCR Pipeline |          | State Machine|          | Tax Storage  |         |
|   | Extraction   |          | Notifications|          | Fuzzy Match  |         |
|   | Queue Route  |          | Email Actions|          | Spend View   |         |
|   +--------------+          +--------------+          +--------------+         |
|          |                           |                           |              |
|          +---------------------------+---------------------------+              |
|                                      v                                          |
|   +-------------------------------------------------------------------------+  |
|   |                    bf-ocr (Provider Abstraction)                         |  |
|   |                                                                          |  |
|   |   +---------------+   +---------------+   +---------------+             |  |
|   |   | Tesseract 5   |   | AWS Textract  |   | Google Vision |             |  |
|   |   | (Local/Free)  |   | (Cloud/$0.01) |   | (Fallback)    |             |  |
|   |   | DEFAULT       |   | ESCALATION    |   | HANDWRITING   |             |  |
|   |   +---------------+   +---------------+   +---------------+             |  |
|   +-------------------------------------------------------------------------+  |
|                                                                                 |
+---------------------------------------------------------------------------------+
|                                 DATA LAYER                                      |
|                                                                                 |
|   +------------------+  +------------------+  +------------------+             |
|   | Control Plane    |  |  Tenant DBs      |  |  MinIO (S3)      |             |
|   |  PostgreSQL      |  |   PostgreSQL     |  |                  |             |
|   |                  |  |                  |  |  /tenant-a/      |             |
|   | - tenants        |  | acme_corp_db:    |  |    /invoices/    |             |
|   | - users          |  |  - invoices      |  |    /tax-docs/    |             |
|   | - api_keys       |  |  - vendors       |  |                  |             |
|   | - subscriptions  |  |  - workflows     |  |  /tenant-b/      |             |
|   |                  |  |  - audit_log     |  |    /invoices/    |             |
|   +------------------+  +------------------+  +------------------+             |
|                                                                                 |
|   +------------------+  +------------------+                                   |
|   |  DuckDB          |  |  Redis           |                                   |
|   |  (Per-Tenant)    |  |                  |                                   |
|   |                  |  | - Sessions       |                                   |
|   | - metrics        |  | - Rate limits    |                                   |
|   | - aggregates     |  | - Job queues     |                                   |
|   | - reports        |  | - Pub/Sub        |                                   |
|   +------------------+  +------------------+                                   |
|                                                                                 |
+---------------------------------------------------------------------------------+
```

### 1.2 Database-per-Tenant Architecture

**Decision: Database-per-tenant** (not row-level security)

```
                    +---------------------------------------+
                    |           CONTROL PLANE               |
                    |                                       |
                    |  control_plane_db (PostgreSQL)        |
                    |  +-------------------------------+    |
                    |  | tenants                        |    |
                    |  |  - id: uuid                    |    |
                    |  |  - slug: "acme-corp"           |    |
                    |  |  - db_name: "bf_tenant_acme"   |    |
                    |  |  - modules: ["capture","proc"] |    |
                    |  |  - settings: {...}             |    |
                    |  +-------------------------------+    |
                    +---------------------------------------+
                                       |
                    +------------------+------------------+
                    v                  v                  v
            +--------------+   +--------------+   +--------------+
            |bf_tenant_acme|   |bf_tenant_tech|   |bf_tenant_mfg |
            +--------------+   +--------------+   +--------------+
            | - invoices   |   | - invoices   |   | - invoices   |
            | - vendors    |   | - vendors    |   | - vendors    |
            | - workflows  |   | - workflows  |   | - workflows  |
            | - audit_log  |   | - audit_log  |   | - audit_log  |
            +--------------+   +--------------+   +--------------+
```

**Rationale:**
- Complete data isolation (regulatory compliance for healthcare, legal, finance)
- Per-tenant backup/restore capability
- Easy data portability (customer can export their database)
- No cross-tenant query risk
- **Trade-off**: Higher connection overhead (mitigated by per-tenant connection pools)

### 1.3 OCR Pipeline Architecture

```
+-------------------------------------------------------------------------+
|                        OCR PROCESSING PIPELINE                           |
|                                                                          |
|   +-----------+                                                          |
|   |  INGEST   |  Supported: PDF, PNG, JPG, TIFF                         |
|   |           |  Max size: 25MB                                          |
|   |           |  Validation: file type, ClamAV malware scan             |
|   +-----+-----+                                                          |
|         |                                                                |
|         v                                                                |
|   +-----------+                                                          |
|   |PREPROCESS |  - Deskew (correct rotation)                            |
|   |           |  - Contrast enhancement                                  |
|   |           |  - Noise reduction                                       |
|   |           |  - Document classification                               |
|   +-----+-----+                                                          |
|         |                                                                |
|         v                                                                |
|   +-----------------------------------------------------+               |
|   |                  PROVIDER ROUTER                     |               |
|   |                                                      |               |
|   |   Tenant Privacy Mode = TRUE?                        |               |
|   |      -> Tesseract 5 ONLY (no cloud)                  |               |
|   |                                                      |               |
|   |   Tenant Privacy Mode = FALSE (default)?             |               |
|   |      -> Tesseract 5 (primary)                        |               |
|   |          -> If confidence < 75% -> AWS Textract      |               |
|   |              -> If still < 75% -> Google Vision      |               |
|   |                  -> If still < 70% -> Error Queue    |               |
|   +-----------------------------------------------------+               |
|         |                                                                |
|         v                                                                |
|   +-----------+                                                          |
|   |  EXTRACT  |  Header Fields:                                         |
|   |           |   - vendor_name (+ normalized)                           |
|   |           |   - invoice_number                                       |
|   |           |   - invoice_date, due_date                               |
|   |           |   - total_amount, currency                               |
|   |           |   - tax_amount, subtotal                                 |
|   |           |                                                          |
|   |           |  Line Items (Phase 2):                                   |
|   |           |   - description, quantity, unit_price, amount            |
|   +-----+-----+                                                          |
|         |                                                                |
|         v                                                                |
|   +-----------+                                                          |
|   | VALIDATE  |  - Required fields present?                              |
|   |           |  - Date/amount format valid?                             |
|   |           |  - Duplicate check (invoice# + vendor hash)              |
|   |           |  - Vendor fuzzy match against master list               |
|   +-----+-----+                                                          |
|         |                                                                |
|         v                                                                |
|   +-----------------------------------------------------+               |
|   |                CONFIDENCE ROUTER                     |               |
|   |                                                      |               |
|   |   >= 85% Confidence -----------> AP QUEUE            |               |
|   |                                  (auto-route)        |               |
|   |                                                      |               |
|   |   70-84% Confidence -----------> REVIEW QUEUE        |               |
|   |                                  (human verifies)    |               |
|   |                                                      |               |
|   |   < 70% Confidence ------------> ERROR QUEUE         |               |
|   |                                  (manual entry)      |               |
|   +-----------------------------------------------------+               |
|                                                                          |
+--------------------------------------------------------------------------+
```

### 1.4 Approval Workflow Engine

```
+-------------------------------------------------------------------------+
|                      APPROVAL WORKFLOW ENGINE                            |
|                                                                          |
|   +-------------------------------------------------------------+       |
|   |                        RULE ENGINE                           |       |
|   |                                                              |       |
|   |   Rules stored as JSON, evaluated by Rust expression engine  |       |
|   |                                                              |       |
|   |   {                                                          |       |
|   |     "name": "Standard Amount Tiers",                         |       |
|   |     "priority": 1,                                           |       |
|   |     "conditions": [                                          |       |
|   |       { "if": "amount < 5000", "action": "auto_approve" },   |       |
|   |       { "if": "amount >= 5000 && amount < 25000",            |       |
|   |         "action": { "route_to": "manager", "level": 1 } },   |       |
|   |       { "if": "amount >= 25000 && amount < 50000",           |       |
|   |         "action": { "route_to": "director", "level": 2 } },  |       |
|   |       { "if": "amount >= 50000",                             |       |
|   |         "action": { "route_to": "cfo", "level": 3 } }        |       |
|   |     ],                                                       |       |
|   |     "exceptions": [                                          |       |
|   |       { "if": "vendor.is_new", "action": "add_review" },     |       |
|   |       { "if": "po_mismatch", "action": "exception_queue" }   |       |
|   |     ]                                                        |       |
|   |   }                                                          |       |
|   +-------------------------------------------------------------+       |
|                                   |                                      |
|                                   v                                      |
|   +-------------------------------------------------------------+       |
|   |                      STATE MACHINE                           |       |
|   |                                                              |       |
|   |                      +-----------+                           |       |
|   |                      |  PENDING  |                           |       |
|   |                      +-----+-----+                           |       |
|   |                            |                                 |       |
|   |         +------------------+------------------+              |       |
|   |         v                  v                  v              |       |
|   |   +-----------+     +-----------+     +-----------+         |       |
|   |   | L1_WAIT   |---->| L2_WAIT   |---->| L3_WAIT   |         |       |
|   |   | (Manager) |     | (Director)|     |   (CFO)   |         |       |
|   |   +-----+-----+     +-----+-----+     +-----+-----+         |       |
|   |         |                 |                 |                |       |
|   |         v                 v                 v                |       |
|   |   +-----------------------------------------------+         |       |
|   |   |              TERMINAL STATES                   |         |       |
|   |   |  +--------+   +--------+   +--------+         |         |       |
|   |   |  |APPROVED|   |REJECTED|   | ON_HOLD|         |         |       |
|   |   |  +--------+   +--------+   +--------+         |         |       |
|   |   +-----------------------------------------------+         |       |
|   +-------------------------------------------------------------+       |
|                                   |                                      |
|                                   v                                      |
|   +-------------------------------------------------------------+       |
|   |              EMAIL APPROVAL (Key Differentiator)             |       |
|   |                                                              |       |
|   |   From: notifications@billforge.io                           |       |
|   |   Subject: [Action Required] Invoice #INV-2024-001           |       |
|   |                                                              |       |
|   |   Vendor: Acme Corporation                                   |       |
|   |   Amount: $12,500.00 USD                                     |       |
|   |   Due Date: February 15, 2026                                |       |
|   |                                                              |       |
|   |   [  APPROVE  ]    [  REJECT  ]    [  VIEW  ]               |       |
|   |                                                              |       |
|   |   - Links are HMAC-signed, expire in 72 hours               |       |
|   |   - No login required to approve or reject                   |       |
|   |   - One-time use (invalidated after action)                  |       |
|   |   - IP logging for audit trail                               |       |
|   +-------------------------------------------------------------+       |
|                                                                          |
+--------------------------------------------------------------------------+
```

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | CEO preference, async-first, Tower ecosystem |
| Async Runtime | **Tokio** | 1.x | Industry standard |
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
| Testing | **tokio-test, wiremock** | Latest | Async tests, HTTP mocking |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | CEO preference, App Router, RSC |
| Language | **TypeScript** | 5.x | Strict mode enabled |
| Styling | **Tailwind CSS** | 3.x | CEO preference |
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
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB, CEO preference |
| Analytics DB | **DuckDB** | Embedded, fast aggregations, CEO preference |
| Document Storage | **MinIO** | S3-compatible, local-first development |
| Cache / Queue | **Redis 7** | Sessions, rate limiting, job queues |
| Search | **PostgreSQL FTS + pg_trgm** | Start simple, add Meilisearch if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost per Page |
|----------|----------|----------|---------------|
| **Tesseract 5** | Primary | Local, privacy-first, standard invoices | Free |
| **AWS Textract** | Secondary | Complex layouts, tables | ~$0.01 |
| **Google Vision** | Tertiary | Fallback, handwriting | ~$0.0015 |

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

### Timeline Overview

```
+-------------------------------------------------------------------------+
|                       12-WEEK MVP TIMELINE                               |
+-------------------------------------------------------------------------+
|                                                                          |
|  Week:   1    2    3    4    5    6    7    8    9   10   11   12       |
|          +----+----+----+----+----+----+----+----+----+----+----+       |
|          |    |         |              |              |         |       |
|          | P0 |       P1              |      P2      |   P3    |       |
|          |FOUN|   INVOICE CAPTURE     | INVOICE PROC |  PILOT  |       |
|          |DATI|                       |              |  LAUNCH |       |
|          |ON  |                       |              |         |       |
|                                                                          |
|  M1: Auth    M2: OCR Pipeline      M3: Workflow       M4: 5 Pilots     |
|      Complete    Functional           Functional          Live          |
|                                                                          |
+-------------------------------------------------------------------------+
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
- [ ] Docker Compose running Postgres, Redis, MinIO
- [ ] bf-api health endpoint: GET /health
- [ ] bf-tenant can create tenant databases
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
- [ ] Confidence routing: >=85% -> AP, 70-84% -> review, <70% -> error
- [ ] Manual correction updates invoice data
- [ ] Vendor matching suggests existing vendors

**Success Metrics:**
- OCR accuracy: >=85% on clean PDF test set
- Processing time: <3 seconds per invoice (P95)
- Manual correction: <30 seconds to fix one field

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
| Signed token generation | Backend | HMAC tokens with expiration |
| Email approval endpoints | Backend | GET /api/v1/actions/{token}/approve |
| Email service integration | Backend | SES for notifications |
| Delegation config | Full-stack | Out-of-office routing |
| SLA tracking | Backend | Time-in-queue calculation |
| Audit trail logging | Backend | All actions logged |
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
- Approval action latency: <5 seconds (P95)
- Email approval success rate: >=95%
- Audit coverage: 100% of state changes logged

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Production deployment and 5 pilot customers

#### Week 11: Production Readiness

| Task | Owner | Deliverable |
|------|-------|-------------|
| Production environment | DevOps | Kubernetes deployment |
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
| **OCR accuracy below 85%** | Medium | High | Multi-provider fallback; human review loop; collect training data |
| **Rust learning curve** | Medium | Medium | Pair programming; code reviews; Go as fallback for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; RLS as defense-in-depth |
| **Email action token security** | Medium | High | HMAC with 72h expiration; one-time use; rate limiting; IP audit |
| **DuckDB scalability** | Medium | Medium | Partition by month; archive >12 months; evaluate ClickHouse |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant pools with limits; lazy connections; alerting |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep delays MVP** | High | High | Strict anti-goals; weekly scope review; "Phase 2" is the answer |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack |
| **ERP integration complexity** | High | Medium | Start with QuickBooks; use official SDK; defer others |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ; health checks; auto-failover; <1h MTTR |
| **Key person dependency** | High | High | ADRs; pair programming; knowledge sharing |
| **Security incident** | Low | Critical | Pen testing; incident response plan; bug bounty |

### 4.4 Risk Priority Matrix

```
                    IMPACT
                    Low        Medium       High        Critical
              +------------+------------+------------+------------+
         High |            | Feature    |            |            |
              |            | creep      |            |            |
              +------------+------------+------------+------------+
  P    Medium |            | Rust       | OCR        |            |
  R           |            | learning   | accuracy   |            |
  O           |            | DuckDB     | Pilot churn|            |
  B           |            | scale      | Email sec  |            |
  A           |            | Connpool   |            |            |
  B    +------------+------------+------------+------------+
  I     Low   |            | S3 issues  |            | Data loss  |
  L           |            |            |            | Tenant     |
  I           |            |            |            | isolation  |
  T           |            |            |            | Security   |
  Y           |            |            |            | incident   |
              +------------+------------+------------+------------+
```

---

## 5. Resource Requirements

### 5.1 Team Structure

```
+-------------------------------------------------------------------------+
|                           BILL FORGE TEAM                                |
+-------------------------------------------------------------------------+
|                                                                          |
|   ENGINEERING (4.5 FTE)                                                 |
|   +---------------------------------------------------------------------+
|   |                                                                      |
|   |   Backend Engineer (Rust) - 2 FTE                                   |
|   |   - bf-api, bf-invoice, bf-workflow, bf-ocr crates                  |
|   |   - Database schema design and queries                              |
|   |   - OCR pipeline and accuracy optimization                          |
|   |   - Approval workflow engine                                        |
|   |                                                                      |
|   |   Frontend Engineer (Next.js/TypeScript) - 1 FTE                    |
|   |   - Invoice capture UI (upload, preview, correction)                |
|   |   - Approval inbox and workflow UI                                  |
|   |   - Dashboard and analytics views                                   |
|   |   - Component library (shadcn/ui customization)                     |
|   |                                                                      |
|   |   Full-Stack / DevOps Engineer - 1 FTE                              |
|   |   - CI/CD pipeline, Docker, Kubernetes                              |
|   |   - Monitoring, alerting, observability                             |
|   |   - Integration work between frontend and backend                   |
|   |   - Security hardening                                              |
|   |                                                                      |
|   |   ML/AI Engineer (Contract) - 0.5 FTE                               |
|   |   - OCR accuracy tuning and provider selection                      |
|   |   - Field extraction optimization                                   |
|   |   - Winston AI adaptation (Phase 3+)                                |
|   |                                                                      |
|   +---------------------------------------------------------------------+
|                                                                          |
|   PRODUCT (1 FTE)                                                       |
|   +---------------------------------------------------------------------+
|   |   Product Manager - 1 FTE                                           |
|   |   - Pilot customer relationships and onboarding                     |
|   |   - Feature prioritization and roadmap                              |
|   |   - User research and feedback synthesis                            |
|   +---------------------------------------------------------------------+
|                                                                          |
|   TOTAL: 5.5 FTE for 12-week MVP                                        |
+-------------------------------------------------------------------------+
```

### 5.2 Hiring Priorities

| Role | Priority | When | Key Skills |
|------|----------|------|------------|
| Backend Engineer (Rust) | P0 | Week 1 | Rust, Axum, PostgreSQL, async |
| Backend Engineer (Rust) | P0 | Week 1 | Rust, API design, SQLx |
| Frontend Engineer | P0 | Week 2 | Next.js 14+, TypeScript, Tailwind |
| DevOps Engineer | P1 | Week 1 | Docker, Kubernetes, GitHub Actions |
| ML/AI Contractor | P2 | Week 3 | OCR, document processing |
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
|-- Cargo.toml                      # Rust workspace root
|-- Cargo.lock
|-- package.json                    # pnpm workspace root
|-- pnpm-workspace.yaml
|-- docker-compose.yml              # Local development
|-- .env.example
|-- README.md
|
|-- .github/
|   +-- workflows/
|       |-- ci.yml                  # PR: lint, test, build
|       |-- deploy-staging.yml
|       +-- deploy-prod.yml
|
|-- crates/                         # Rust backend crates
|   |-- bf-api/                     # API gateway (Axum)
|   |   |-- Cargo.toml
|   |   +-- src/
|   |       |-- main.rs
|   |       |-- routes/
|   |       |-- middleware/
|   |       +-- error.rs
|   |
|   |-- bf-invoice/                 # Invoice capture service
|   |-- bf-workflow/                # Approval workflow engine
|   |-- bf-ocr/                     # OCR provider abstraction
|   |-- bf-vendor/                  # Vendor management
|   |-- bf-storage/                 # S3/MinIO abstraction
|   |-- bf-auth/                    # Authentication
|   |-- bf-tenant/                  # Tenant management
|   |-- bf-analytics/               # DuckDB analytics
|   +-- bf-common/                  # Shared types, utilities
|
|-- apps/                           # Frontend applications
|   +-- web/                        # Next.js main app
|       |-- package.json
|       |-- next.config.mjs
|       |-- tailwind.config.ts
|       +-- src/
|           |-- app/                # App Router pages
|           |-- components/         # UI components
|           +-- lib/                # Utilities
|
|-- packages/                       # Shared JS packages
|   |-- ui/                         # Extended shadcn/ui
|   +-- api-client/                 # Generated TypeScript client
|
|-- services/                       # Additional services
|   +-- winston/                    # AI assistant (Phase 3)
|       |-- pyproject.toml          # Python (adapted from Locust)
|       +-- src/winston/
|
|-- migrations/                     # Database migrations
|   |-- control-plane/
|   +-- tenant/
|
|-- infra/                          # Infrastructure as code
|   |-- terraform/
|   +-- kubernetes/
|
|-- docs/                           # Documentation
|   |-- api/                        # OpenAPI specs
|   |-- architecture/               # ADRs
|   +-- runbooks/
|
+-- tests/                          # End-to-end tests
    |-- e2e/
    +-- load/
```

---

## 7. Success Metrics

### 7.1 Technical Metrics (3-Month Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **OCR Accuracy** | >=85% | Correct fields / Total fields |
| **OCR Accuracy (clean PDFs)** | >=90% | Well-formatted digital PDFs |
| **Processing Latency (P95)** | <5 sec | Upload to queue placement |
| **API Response Time (P95)** | <200ms | Non-OCR endpoints |
| **System Uptime** | >=99.5% | Monthly availability |
| **Test Coverage** | >=80% | Line coverage on core crates |
| **Critical Bugs** | 0 | Unresolved P0 in production |
| **Security Vulnerabilities** | 0 Critical/High | SAST/DAST results |

### 7.2 Business Metrics (3-Month Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 1,000+ | Total across pilots |
| **Customer NPS** | >=50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | >=60% | Conversion conversations |
| **Email Approval Success** | >=95% | Successful / Total |

### 7.3 Operational Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deployment Frequency** | Daily | Deploys to staging |
| **Mean Time to Recovery** | <1 hour | Incident to resolution |
| **Change Failure Rate** | <15% | Deploys requiring rollback |

---

## 8. Answers to CEO's Strategic Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette/Rillion Strengths:**
- Established presence in Nordics/Europe with 20+ years in market
- Deep SAP and Oracle integrations built over time
- Mature workflow engine handling complex scenarios
- Existing customer base provides stability proof

**Palette/Rillion Weaknesses (Our Opportunities):**
- **UI/UX**: Described as "slow" and "clunky" in customer reviews
- **Innovation**: Limited AI/ML advancement in recent years
- **Pricing**: Opaque "call for quote" model
- **Mobile**: Poor to non-existent mobile experience
- **Support**: Slow, impersonal support processes

**Bill Forge Differentiation Strategy:**

| Dimension | Palette | Bill Forge | Advantage |
|-----------|---------|------------|-----------|
| UI Speed | Multi-second loads | Sub-second | 10x faster |
| Setup Time | Weeks/months | Hours/days | Self-service |
| Pricing | "Call for quote" | Published | Trust |
| OCR | Cloud-only | Local-first | Privacy |
| Approvals | Login required | Email (no login) | Frictionless |
| AI | Limited/none | Winston assistant | Intelligence |

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence | Queue | Action |
|------------|-------|--------|
| **>=85%** | AP Queue | Auto-route to workflow |
| **70-84%** | Review Queue | Human verifies flagged fields |
| **<70%** | Error Queue | Full manual entry |

**Implementation Notes:**
- Calculate overall confidence as weighted average
- Weight amount and vendor_name higher (critical)
- Store per-field confidence for granular review UI
- Collect corrections as training data

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Rationale | Complexity | Timeline |
|-----|----------|-----------|------------|----------|
| **QuickBooks Online** | 1 | Largest mid-market share, REST API | Low | 2-3 weeks |
| NetSuite | 2 | Growing companies, SuiteScript | Medium | 4-6 weeks |
| Sage Intacct | 3 | Manufacturing, REST API | Medium | 4-6 weeks |
| Dynamics 365 | 4 | Microsoft ecosystem | High | 6-8 weeks |

**Why QuickBooks First:**
- Largest addressable market for 10-1000 employee companies
- Simplest API with best documentation
- Enables ProAdvisor partnership channel

### Q4: What approval workflow patterns are most common in mid-market companies?

**Research-Based Patterns:**

1. **Amount-Based Tiers (85%)** - MVP Priority
   ```
   < $5,000:      Auto-approve (if vendor known)
   $5K - $25K:    Manager approval
   $25K - $50K:   Director/VP approval
   > $50K:        CFO/Controller approval
   ```

2. **Exception-Only Review (65%)** - MVP Priority
   - Clean invoices (match PO, known vendor) -> auto-approve
   - Exceptions (no PO, new vendor) -> review queue

3. **Department/Cost Center (45%)** - Phase 2
4. **Dual Approval (30%)** - Phase 2

**MVP Implementation:** Amount-based tiers + exception routing

### Q5: How do competitors handle multi-currency and international invoices?

**Common Approaches:**
- Store original currency alongside converted base currency
- Daily exchange rate sync from ECB or Open Exchange Rates
- Allow manual rate override
- Display both currencies in UI

**Recommendation for MVP:**
- Support currency field extraction (USD, EUR, GBP, CAD)
- Convert to tenant's base currency for totals
- Use Open Exchange Rates API (free tier: 1,000/month)
- Store both original and converted amounts
- **Defer full multi-currency GL posting to Phase 2**

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommendation: Tiered Usage-Based Pricing**

| Tier | Monthly Base | Invoices | Overage | Target |
|------|--------------|----------|---------|--------|
| **Starter** | $299 | 500 | $0.75 | Testing |
| **Growth** | $799 | 2,000 | $0.50 | Primary ICP |
| **Scale** | $1,999 | 10,000 | $0.30 | Larger mid-market |
| **Enterprise** | Custom | Custom | Custom | 10K+ invoices |

**Why This Model:**
- **No per-seat pricing**: AP teams hate paying for each approver
- **Predictable base**: Finance can budget effectively
- **Scales with business**: Aligned with value delivered
- **Transparent**: Published pricing builds trust

**Module Add-Ons (Phase 2+):**
- Vendor Management: +$199/month
- Advanced Reporting: +$299/month
- Winston AI: +$299/month
- NetSuite Integration: +$199/month

---

## 9. Winston AI Strategy (Leveraging Locust)

### 9.1 What to Reuse from Locust

The existing Locust codebase contains a sophisticated LangGraph-based agent framework.

**Keep and Adapt:**

| Locust Component | Adaptation for Winston |
|------------------|------------------------|
| Agent base classes (agents/base.py) | Simplify for single-agent |
| LLM backend switching (llm/) | Keep Claude + Ollama |
| Workflow state (workflows/state.py) | Adapt for query/action |
| Memory/embeddings (memory/) | Use for semantic search |
| Checkpoint/resume (ceo/checkpoint.py) | Conversation recovery |

**Remove:**
- Software development agents (CTO, CPO, etc.)
- Code generation modules
- Research workflows
- Git integration

### 9.2 Winston Tool Design

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
    """
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
    """Run a spending or processing report."""
    pass
```

### 9.3 Winston Timeline

**Phase 3+ (Post-MVP):** ~3 weeks to adapt Locust architecture

| Week | Focus |
|------|-------|
| 1 | Fork Locust agent core, strip unused |
| 2 | Implement Bill Forge tools, API integration |
| 3 | Chat UI, testing, tenant isolation |

**Effort Savings:** 60% reduction vs building from scratch

---

## 10. Immediate Next Steps

### This Week (Week 0)

**Day 1-2: Repository Setup**
- [ ] Create bill-forge repository
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
- (-) Tokens can be forwarded (mitigated by audit)

---

## Appendix B: Competitive Analysis Summary

| Competitor | Strengths | Weaknesses | Bill Forge Advantage |
|------------|-----------|------------|---------------------|
| **BILL.com** | Simple, SMB-friendly | Outgrown at scale | Mid-market workflows |
| **Palette** | Deep ERP integrations | Slow UI, opaque pricing | Speed, transparency |
| **Tipalti** | Global payments | Complex, enterprise pricing | Simpler, domestic focus |
| **Coupa** | Enterprise features | Overbuilt for mid-market | Right-sized features |
| **AvidXchange** | AP automation depth | Legacy experience | Modern UX |

---

## Appendix C: Technical Debt Prevention

To avoid accumulating debt during MVP sprint:

1. **Mandatory code review** for all PRs
2. **80% test coverage** target for core paths
3. **ADRs** for all architectural decisions
4. **Weekly debt review** - 20% time for cleanup
5. **No "temporary" hacks** without tracking ticket

---

**Document Approval:**

- [x] CTO Review Complete
- [ ] CEO Alignment
- [ ] Engineering Lead Review
- [ ] Product Manager Review

**Next Steps:**
1. Share with engineering team
2. Create sprint tickets from Phase 0 tasks
3. Set up weekly progress reviews
4. Begin repository setup

---

*This document is the consolidated strategic technical plan for Bill Forge. It supersedes all previous versions and will be updated as decisions evolve.*
