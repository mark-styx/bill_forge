# Bill Forge: CTO Strategic Technical Plan

**Version:** 4.0
**Date:** February 1, 2026
**Author:** Chief Technology Officer
**Status:** Final - Ready for Execution
**Planning Horizon:** 12 Weeks (Q1 2026)

---

## Executive Summary

Bill Forge is a modular B2B SaaS platform targeting mid-market companies (50-500 employees) frustrated with either overbuilt enterprise AP tools or underpowered SMB solutions. This document provides the authoritative technical blueprint for our 12-week MVP sprint.

### Strategic Position

**The fast, fair, and intelligent AP platform for growing companies.**

Our core thesis: invoice processing workflows are well-understood. We win by executing dramatically better on UX, speed, and pricing - not by reinventing processes.

### Key Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Backend Language** | Rust (Axum) | Sub-second response times, compile-time safety, CEO preference |
| **Frontend** | Next.js 14+ | Modern React, server components, CEO preference |
| **Database Strategy** | Database-per-tenant | Complete isolation, compliance-ready, portability |
| **OCR Strategy** | Tesseract 5 primary, cloud fallback | Privacy-first positioning, cost optimization |
| **AI Foundation** | Adapt Locust framework | 60% effort reduction for Winston AI |

---

## 1. Technical Architecture Recommendations

### 1.1 High-Level System Architecture

```
+------------------------------------------------------------------------------+
|                           BILL FORGE PLATFORM                                 |
+------------------------------------------------------------------------------+
|                                                                               |
|  +-------------------------------------------------------------------------+ |
|  |                        NEXT.JS 14+ FRONTEND                              | |
|  |                                                                          | |
|  |  /invoices          /approvals        /vendors          /reports        | |
|  |  +----------+      +----------+      +----------+      +----------+    | |
|  |  | Upload   |      |  Inbox   |      |  Master  |      |Dashboard |    | |
|  |  | Preview  |      | Actions  |      |   Data   |      | Charts   |    | |
|  |  | Correct  |      |   SLA    |      | Tax Docs |      | Export   |    | |
|  |  +----------+      +----------+      +----------+      +----------+    | |
|  |                                                                          | |
|  |  Tech: shadcn/ui | Tailwind CSS | TanStack Query | React Hook Form      | |
|  +-------------------------------------------------------------------------+ |
|                                        |                                      |
|                                        v                                      |
|  +-------------------------------------------------------------------------+ |
|  |                         RUST API GATEWAY (bf-api)                        | |
|  |                                                                          | |
|  |  Axum 0.7+ | JWT Auth | Tenant Resolution | Rate Limiting | CORS        | |
|  |                                                                          | |
|  |  Middleware Stack: Request -> Log -> Auth -> Tenant -> RateLimit -> Handler |
|  +-------------------------------------------------------------------------+ |
|                                        |                                      |
|            +---------------------------+---------------------------+         |
|            v                           v                           v         |
|     +--------------+          +--------------+          +--------------+    |
|     |  bf-invoice  |          | bf-workflow  |          |  bf-vendor   |    |
|     +--------------+          +--------------+          +--------------+    |
|     | Upload API   |          | Rule Engine  |          | Master Data  |    |
|     | OCR Pipeline |          | State Machine|          | Tax Storage  |    |
|     | Extraction   |          | Notifications|          | Fuzzy Match  |    |
|     | Queue Route  |          | Email Actions|          | Spend View   |    |
|     +--------------+          +--------------+          +--------------+    |
|            |                           |                           |         |
|            +---------------------------+---------------------------+         |
|                                        v                                      |
|  +-------------------------------------------------------------------------+ |
|  |                     bf-ocr (Provider Abstraction)                        | |
|  |                                                                          | |
|  |  +----------------+  +----------------+  +----------------+              | |
|  |  |  Tesseract 5   |  |  AWS Textract  |  | Google Vision  |              | |
|  |  |  (Local/Free)  |  |  (Cloud/$0.01) |  |  (Fallback)    |              | |
|  |  |   DEFAULT      |  |   ESCALATION   |  |  HANDWRITING   |              | |
|  |  +----------------+  +----------------+  +----------------+              | |
|  +-------------------------------------------------------------------------+ |
|                                                                               |
+-------------------------------------------------------------------------------+
|                              DATA LAYER                                       |
|                                                                               |
|  +------------------+  +------------------+  +------------------+             |
|  |  Control Plane   |  |   Tenant DBs     |  |  MinIO (S3)      |             |
|  |   PostgreSQL     |  |   PostgreSQL     |  |                  |             |
|  |                  |  |                  |  |  /tenant-a/      |             |
|  | - tenants        |  |  bf_tenant_acme: |  |    /invoices/    |             |
|  | - users          |  |  - invoices      |  |    /tax-docs/    |             |
|  | - api_keys       |  |  - vendors       |  |                  |             |
|  | - subscriptions  |  |  - workflows     |  |  /tenant-b/      |             |
|  |                  |  |  - audit_log     |  |    /invoices/    |             |
|  +------------------+  +------------------+  +------------------+             |
|                                                                               |
|  +------------------+  +------------------+                                   |
|  |  DuckDB          |  |  Redis           |                                   |
|  |  (Per-Tenant)    |  |                  |                                   |
|  |                  |  | - Sessions       |                                   |
|  | - metrics        |  | - Rate limits    |                                   |
|  | - aggregates     |  | - Job queues     |                                   |
|  | - reports        |  | - Pub/Sub        |                                   |
|  +------------------+  +------------------+                                   |
|                                                                               |
+-------------------------------------------------------------------------------+
```

### 1.2 Database-per-Tenant Architecture

**Decision:** Database-per-tenant (not row-level security)

**Why This Matters:**
- Complete data isolation satisfies healthcare, legal, and financial compliance requirements
- Per-tenant backup/restore capability for disaster recovery
- Easy data portability (customers can export their database)
- No cross-tenant query risk (defense in depth)
- Trade-off: Higher connection overhead (mitigated by per-tenant connection pools)

```
                        +------------------------------------+
                        |          CONTROL PLANE             |
                        |                                    |
                        |  control_plane_db (PostgreSQL)     |
                        |  +------------------------------+  |
                        |  | tenants                       |  |
                        |  |  - id: uuid                   |  |
                        |  |  - slug: "acme-corp"          |  |
                        |  |  - db_name: "bf_tenant_acme"  |  |
                        |  |  - modules: ["capture","proc"]|  |
                        |  |  - settings: {...}            |  |
                        |  +------------------------------+  |
                        +------------------------------------+
                                         |
                    +--------------------+--------------------+
                    v                    v                    v
            +--------------+     +--------------+     +--------------+
            |bf_tenant_acme|     |bf_tenant_tech|     |bf_tenant_mfg |
            +--------------+     +--------------+     +--------------+
            | - invoices   |     | - invoices   |     | - invoices   |
            | - vendors    |     | - vendors    |     | - vendors    |
            | - workflows  |     | - workflows  |     | - workflows  |
            | - audit_log  |     | - audit_log  |     | - audit_log  |
            +--------------+     +--------------+     +--------------+
```

### 1.3 OCR Pipeline Architecture

The OCR pipeline is the heart of Invoice Capture, our primary module.

**Pipeline Flow:**

```
INGEST -> PREPROCESS -> PROVIDER ROUTER -> EXTRACT -> VALIDATE -> CONFIDENCE ROUTER
```

**Provider Router Logic:**

| Condition | Provider | Rationale |
|-----------|----------|-----------|
| Tenant privacy_mode = TRUE | Tesseract 5 only | No cloud data transfer |
| Standard invoice, good quality | Tesseract 5 | Free, fast, sufficient |
| Low confidence (<75%) | AWS Textract | Better table extraction |
| Still low confidence (<75%) | Google Vision | Handwriting support |
| All providers <70% | Error Queue | Requires manual entry |

**Confidence Routing:**

| Overall Confidence | Queue | User Action |
|-------------------|-------|-------------|
| >= 85% | AP Queue | Auto-route to workflow |
| 70-84% | Review Queue | Verify flagged fields only |
| < 70% | Error Queue | Full manual entry |

**Confidence Calculation:**
```
Overall = (amount_conf * 0.30) + (vendor_conf * 0.25) +
          (invoice_num_conf * 0.20) + (date_conf * 0.15) +
          (currency_conf * 0.10)
```

### 1.4 Approval Workflow Engine

The workflow engine is a configurable state machine with JSON-defined rules.

**State Diagram:**

```
                        +----------+
                        | PENDING  |
                        +----+-----+
                             |
          +------------------+------------------+
          v                  v                  v
    +-----------+     +-----------+     +-----------+
    | L1_WAIT   |---->| L2_WAIT   |---->| L3_WAIT   |
    | (Manager) |     | (Director)|     |   (CFO)   |
    +-----+-----+     +-----+-----+     +-----+-----+
          |                 |                 |
          v                 v                 v
    +-----------------------------------------------------+
    |               TERMINAL STATES                        |
    |  +-----------+ +-----------+ +-----------+          |
    |  | APPROVED  | | REJECTED  | | ON_HOLD   |          |
    |  +-----------+ +-----------+ +-----------+          |
    +-----------------------------------------------------+
```

**Rule Configuration Example:**

```json
{
  "name": "Standard Amount Tiers",
  "priority": 1,
  "conditions": [
    { "if": "amount < 5000 && vendor.is_known", "action": "auto_approve" },
    { "if": "amount >= 5000 && amount < 25000",
      "action": { "route_to": "manager", "level": 1 } },
    { "if": "amount >= 25000 && amount < 50000",
      "action": { "route_to": "director", "level": 2 } },
    { "if": "amount >= 50000",
      "action": { "route_to": "cfo", "level": 3 } }
  ],
  "exceptions": [
    { "if": "vendor.is_new", "action": "add_review" },
    { "if": "po_mismatch", "action": "exception_queue" }
  ]
}
```

### 1.5 Email Approval System (Key Differentiator)

Email approvals without login are a primary competitive advantage.

**Security Model:**

| Security Measure | Implementation |
|-----------------|----------------|
| Token signing | HMAC-SHA256 with secret key |
| Expiration | 72 hours from generation |
| Single use | Token invalidated after action |
| Audit trail | IP address, user agent, timestamp logged |
| Rate limiting | Max 10 attempts per token per minute |

**Token Structure:**
```
https://app.billforge.io/api/v1/actions/{base64(invoice_id:action:timestamp:hmac)}
```

**Email Template Key Elements:**
- Invoice summary (vendor, amount, due date)
- One-click Approve/Reject buttons
- View invoice link (requires login)
- Clear sender identity and branding

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | Async-first, Tower middleware, CEO preference |
| Async Runtime | **Tokio** | 1.x | Industry standard |
| Serialization | **Serde** | Latest | De facto Rust standard |
| Database | **SQLx** | 0.7+ | Compile-time query checking |
| Migrations | **sqlx-cli** | 0.7+ | Integrated with SQLx |
| Validation | **validator** | Latest | Derive macros |
| Error Handling | **thiserror** | Latest | Typed errors |
| Logging | **tracing** | Latest | Structured logging |
| Config | **config-rs** | Latest | Multi-source config |
| HTTP Client | **reqwest** | Latest | OCR provider calls |
| UUID | **uuid** | Latest | Entity identifiers |
| Date/Time | **chrono** | Latest | Timestamps |
| Password | **argon2** | Latest | Secure hashing |
| JWT | **jsonwebtoken** | Latest | Token auth |
| Testing | **tokio-test, wiremock** | Latest | Async tests |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | App Router, RSC |
| Language | **TypeScript** | 5.x | Strict mode |
| Styling | **Tailwind CSS** | 3.x | Utility-first |
| Components | **shadcn/ui** | Latest | Accessible, customizable |
| Server State | **TanStack Query** | 5.x | Caching, optimistic updates |
| Forms | **React Hook Form + Zod** | Latest | Type-safe validation |
| Tables | **TanStack Table** | 8.x | Invoice lists |
| Charts | **Recharts** | 2.x | Dashboards |
| Date Picker | **date-fns + react-day-picker** | Latest | Date handling |
| Notifications | **Sonner** | Latest | Toasts |
| Icons | **Lucide React** | Latest | Consistent iconography |
| Auth Client | **next-auth** | 4.x | Session management |
| API Client | Generated from **OpenAPI** | - | Type-safe calls |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB |
| Analytics DB | **DuckDB** | Embedded, fast aggregations |
| Document Storage | **MinIO** | S3-compatible, local-first |
| Cache/Queue | **Redis 7** | Sessions, rate limiting, jobs |
| Search | **PostgreSQL FTS + pg_trgm** | Start simple |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost/Page |
|----------|----------|----------|-----------|
| **Tesseract 5** | Primary | Local, privacy-first | Free |
| **AWS Textract** | Secondary | Complex layouts | ~$0.01 |
| **Google Vision** | Tertiary | Handwriting | ~$0.0015 |

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
+------------------------------------------------------------------------------+
|                        12-WEEK MVP TIMELINE                                   |
+------------------------------------------------------------------------------+
|                                                                               |
|  Week:   1    2    3    4    5    6    7    8    9   10   11   12            |
|          +----+----+----+----+----+----+----+----+----+----+----+            |
|          |    |         |              |              |         |            |
|          | P0 |       P1              |      P2      |   P3    |            |
|          |FOUN|   INVOICE CAPTURE     | INVOICE PROC |  PILOT  |            |
|          |DATI|                       |              |  LAUNCH |            |
|          |ON  |                       |              |         |            |
|                                                                               |
|  Milestones:                                                                  |
|  M1 (W2): Auth + Tenant Complete    M3 (W10): Workflow Functional            |
|  M2 (W6): OCR Pipeline Ready        M4 (W12): 5 Pilots Live                  |
|                                                                               |
+------------------------------------------------------------------------------+
```

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Project structure, infrastructure, authentication

#### Week 1: Infrastructure

| Task | Deliverable | Priority |
|------|-------------|----------|
| Create monorepo structure | Cargo.toml workspace + pnpm-workspace.yaml | P0 |
| Docker Compose setup | PostgreSQL, Redis, MinIO running locally | P0 |
| CI/CD pipeline | GitHub Actions: lint, test, build on PR | P0 |
| Control plane schema | tenants, users, api_keys tables | P0 |
| Tenant service | bf-tenant crate: create/list tenants | P0 |
| SQLx migrations | Migration infrastructure | P0 |

#### Week 2: Auth + API Foundation

| Task | Deliverable | Priority |
|------|-------------|----------|
| JWT authentication | bf-auth crate: issue/verify tokens | P0 |
| API gateway | bf-api crate with health check | P0 |
| Tenant resolution middleware | Extract tenant from URL path | P0 |
| Next.js scaffold | App with shadcn/ui, login page | P0 |
| API client generation | OpenAPI to TypeScript client | P0 |
| Seed data/fixtures | Development test data | P1 |

**Phase 0 Exit Criteria:**
- [ ] Monorepo with crates/ and apps/web/
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] bf-api health endpoint: GET /health returns 200 OK
- [ ] bf-tenant can create tenant databases dynamically
- [ ] bf-auth issues and verifies JWTs
- [ ] Next.js app renders login page
- [ ] CI pipeline passes on every PR

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with confidence-based routing

#### Weeks 3-4: OCR Pipeline

| Task | Deliverable | Priority |
|------|-------------|----------|
| Document upload API | POST /api/v1/{tenant}/invoices/upload | P0 |
| S3 storage abstraction | bf-storage crate | P0 |
| Tesseract integration | bf-ocr crate with local OCR | P0 |
| Field extraction | Vendor, invoice #, amount, date, currency | P0 |
| Confidence scoring | Per-field and overall confidence | P0 |
| Queue data models | invoices, invoice_queue tables | P0 |

#### Weeks 5-6: Capture UI + Vendor Matching

| Task | Deliverable | Priority |
|------|-------------|----------|
| Invoice upload UI | Drag-drop, file preview | P0 |
| OCR results display | Confidence badges per field | P0 |
| Manual correction UI | Inline edit with highlighting | P0 |
| Vendor fuzzy matching | Levenshtein distance matching | P0 |
| Vendor CRUD API | GET/POST/PATCH /vendors | P0 |
| Queue dashboard | AP queue, review queue, error queue views | P0 |

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

| Metric | Target |
|--------|--------|
| OCR accuracy (clean PDFs) | >= 85% |
| Processing time (P95) | < 3 seconds |
| Manual correction time | < 30 seconds per field |

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Objective:** Approval workflows with email actions

#### Weeks 7-8: Workflow Engine

| Task | Deliverable | Priority |
|------|-------------|----------|
| Workflow rule engine | bf-workflow crate with JSON rules | P0 |
| Approval state machine | States: pending, l1_wait, approved, etc. | P0 |
| Rule configuration API | GET/POST /workflows | P0 |
| Approval inbox UI | Pending items with bulk select | P0 |
| Approve/reject/hold actions | Action buttons + API endpoints | P0 |

#### Weeks 9-10: Email Actions + Audit

| Task | Deliverable | Priority |
|------|-------------|----------|
| Signed token generation | HMAC tokens with 72h expiration | P0 |
| Email approval endpoints | GET /api/v1/actions/{token}/approve | P0 |
| Email service integration | AWS SES for notifications | P0 |
| Delegation config | Out-of-office routing | P1 |
| SLA tracking | Time-in-queue calculation | P1 |
| Audit trail logging | All actions logged with IP | P0 |
| Bulk operations | Batch approve/reject | P1 |

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
| Approval action latency (P95) | < 5 seconds |
| Email approval success rate | >= 95% |
| Audit coverage | 100% of state changes |

### Phase 3: Pilot Launch (Weeks 11-12)

**Objective:** Production deployment and 5 pilot customers

#### Week 11: Production Readiness

| Task | Deliverable | Priority |
|------|-------------|----------|
| Production environment | Kubernetes deployment on AWS | P0 |
| Security audit | Penetration testing, SAST/DAST | P0 |
| Load testing | 100 invoices/minute sustained | P0 |
| Monitoring + alerting | Prometheus/Grafana dashboards | P0 |
| API documentation | OpenAPI spec published | P0 |
| User guides | Help documentation | P1 |

#### Week 12: Customer Onboarding

| Task | Deliverable | Priority |
|------|-------------|----------|
| Data migration tooling | Import from CSV | P0 |
| White-glove onboarding | Personal setup for each pilot | P0 |
| Feedback mechanisms | In-app feedback, weekly calls | P0 |
| Bug triage process | P0/P1/P2 classification | P0 |
| Hotfix process | Emergency deploy pipeline | P0 |

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
| **Rust learning curve** | Medium | Medium | Pair programming; code reviews; hire experienced Rust devs |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; RLS as defense-in-depth |
| **Email approval token security** | Medium | High | HMAC with 72h expiration; one-time use; rate limiting; IP audit |
| **DuckDB scalability** | Medium | Medium | Partition by month; archive >12 months; evaluate ClickHouse |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant pools with limits; lazy connections; alerting |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep delays MVP** | High | High | Strict anti-goals; weekly scope review; "Phase 2" default |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack |
| **ERP integration complexity** | High | Medium | Start with QuickBooks; use official SDK; defer others |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ; health checks; auto-failover |
| **Key person dependency** | High | High | ADRs; pair programming; knowledge sharing |
| **Security incident** | Low | Critical | Pen testing; incident response plan |

### 4.4 Risk Priority Matrix

```
                        IMPACT
                    Low       Medium      High        Critical
              +----------+----------+----------+----------+
         High |          | Feature  |          |          |
              |          | creep    |          |          |
              +----------+----------+----------+----------+
PROBABILITY   |          | Rust     | OCR      |          |
       Medium |          | learning | accuracy |          |
              |          | DuckDB   | Pilot    |          |
              |          | Connpool | churn    |          |
              |          |          | Email sec|          |
              +----------+----------+----------+----------+
         Low  |          |          |          | Data loss|
              |          |          |          | Tenant   |
              |          |          |          | isolation|
              |          |          |          | Security |
              +----------+----------+----------+----------+
```

---

## 5. Resource Requirements

### 5.1 Team Structure

```
+------------------------------------------------------------------------------+
|                            BILL FORGE TEAM                                    |
+------------------------------------------------------------------------------+
|                                                                               |
|  ENGINEERING (4.5 FTE)                                                       |
|  +-------------------------------------------------------------------------+ |
|  |                                                                          | |
|  |  Backend Engineer (Rust) - 2 FTE                                        | |
|  |  - bf-api, bf-invoice, bf-workflow, bf-ocr crates                       | |
|  |  - Database schema design and queries                                    | |
|  |  - OCR pipeline and accuracy optimization                                | |
|  |  - Approval workflow engine                                              | |
|  |                                                                          | |
|  |  Frontend Engineer (Next.js/TypeScript) - 1 FTE                         | |
|  |  - Invoice capture UI (upload, preview, correction)                      | |
|  |  - Approval inbox and workflow UI                                        | |
|  |  - Dashboard and analytics views                                         | |
|  |  - Component library (shadcn/ui customization)                           | |
|  |                                                                          | |
|  |  Full-Stack / DevOps Engineer - 1 FTE                                   | |
|  |  - CI/CD pipeline, Docker, Kubernetes                                    | |
|  |  - Monitoring, alerting, observability                                   | |
|  |  - Integration work between frontend and backend                         | |
|  |  - Security hardening                                                    | |
|  |                                                                          | |
|  |  ML/AI Engineer (Contract) - 0.5 FTE                                    | |
|  |  - OCR accuracy tuning and provider selection                            | |
|  |  - Field extraction optimization                                         | |
|  |  - Winston AI adaptation (Phase 3+)                                      | |
|  |                                                                          | |
|  +-------------------------------------------------------------------------+ |
|                                                                               |
|  PRODUCT (1 FTE)                                                             |
|  +-------------------------------------------------------------------------+ |
|  |  Product Manager - 1 FTE                                                 | |
|  |  - Pilot customer relationships and onboarding                           | |
|  |  - Feature prioritization and roadmap                                    | |
|  |  - User research and feedback synthesis                                  | |
|  +-------------------------------------------------------------------------+ |
|                                                                               |
|  TOTAL: 5.5 FTE for 12-week MVP                                              |
|                                                                               |
+------------------------------------------------------------------------------+
```

### 5.2 Hiring Priorities

| Role | Priority | Start By | Key Skills |
|------|----------|----------|------------|
| Backend Engineer (Rust) #1 | P0 | Week 1 | Rust, Axum, PostgreSQL, async |
| Backend Engineer (Rust) #2 | P0 | Week 1 | Rust, API design, SQLx |
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

CREATE INDEX idx_vendors_normalized_name ON vendors
    USING gin(normalized_name gin_trgm_ops);

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
    queue VARCHAR(20) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_invoices_status ON invoices(status);
CREATE INDEX idx_invoices_queue ON invoices(queue);
CREATE INDEX idx_invoices_vendor_id ON invoices(vendor_id);

-- Invoice line items (Phase 2)
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
    action_method VARCHAR(20),
    comments TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_approval_steps_approver_status
    ON approval_steps(approver_id, status);

-- Email action tokens
CREATE TABLE email_action_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id),
    action VARCHAR(20) NOT NULL,
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
    actor_type VARCHAR(20),
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
| **OCR Accuracy** | >= 85% | Correct fields / Total fields |
| **OCR Accuracy (clean PDFs)** | >= 90% | Well-formatted digital PDFs |
| **Processing Latency (P95)** | < 5 sec | Upload to queue placement |
| **API Response Time (P95)** | < 200ms | Non-OCR endpoints |
| **System Uptime** | >= 99.5% | Monthly availability |
| **Test Coverage** | >= 80% | Line coverage on core crates |
| **Critical Bugs** | 0 | Unresolved P0 in production |
| **Security Vulnerabilities** | 0 Critical/High | SAST/DAST results |

### 9.2 Business Metrics (3-Month Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 3,500+ | Total across pilots |
| **Customer NPS** | >= 50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | >= 60% | Conversion conversations |
| **Email Approval Success** | >= 95% | Successful / Total |

### 9.3 Operational Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deployment Frequency** | Daily | Deploys to staging |
| **Mean Time to Recovery** | < 1 hour | Incident to resolution |
| **Change Failure Rate** | < 15% | Deploys requiring rollback |

---

## 10. Answers to CEO's Strategic Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**Strengths:**
- 20+ years in Nordic/European markets with deep SAP/Oracle integrations
- Mature workflow engine for complex multinational scenarios
- Established customer base provides stability proof

**Weaknesses (Our Opportunities):**
- UI described as "slow" and "clunky" in customer reviews (3-5 second page loads)
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

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence | Queue | Action |
|------------|-------|--------|
| **>= 85%** | AP Queue | Auto-route to workflow |
| **70-84%** | Review Queue | Human verifies flagged fields |
| **< 70%** | Error Queue | Full manual entry required |

**Implementation Notes:**
- Calculate overall confidence as weighted average of field confidences
- Weight amount (30%) and vendor_name (25%) higher (critical fields)
- Store per-field confidence for granular review UI
- Collect corrections as training data for future optimization
- Allow tenant-configurable thresholds for stricter/looser routing

### Q3: Which ERP integration should we prioritize first?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Complexity | Timeline |
|-----|----------|------------|----------|
| **QuickBooks Online** | 1 | Low | 2-3 weeks |
| NetSuite | 2 | Medium | 4-6 weeks |
| Sage Intacct | 3 | Medium | 4-6 weeks |
| Dynamics 365 | 4 | High | 6-8 weeks |

**Why QuickBooks First:**
- Largest addressable market for 50-500 employee companies
- Simplest REST API with excellent documentation
- Enables ProAdvisor partnership channel (75K+ referral partners)
- Direct alignment with primary ICP

### Q4: What approval workflow patterns are most common?

**Research-Based Patterns:**

1. **Amount-Based Tiers (85% of companies)** - MVP Priority
   ```
   < $5,000:      Auto-approve (if vendor known)
   $5K - $25K:    Manager approval
   $25K - $50K:   Director/VP approval
   > $50K:        CFO/Controller approval
   ```

2. **Exception-Only Review (65%)** - MVP Priority
   - Clean invoices (match PO, known vendor) auto-approve
   - Exceptions (no PO, new vendor) route to review queue

3. **Department/Cost Center (45%)** - Phase 2
4. **Dual Approval (30%)** - Phase 2

**MVP Implementation:** Amount-based tiers + exception routing

### Q5: How do competitors handle multi-currency?

**Common Approaches:**
- Store original currency + converted base currency
- Daily rate sync from ECB, Open Exchange Rates, or XE
- Allow manual rate override for large transactions
- Display both currencies in all views
- Post to ERP in base currency only

**Bill Forge MVP Approach:**
- Capture currency from invoice as metadata
- Support: USD, EUR, GBP, CAD
- Convert for display totals using daily rates (Open Exchange Rates API)
- Store both original and converted amounts
- Send base currency amount to ERP
- **Defer full multi-currency GL posting to Phase 3**

### Q6: What's the pricing model that resonates?

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
- **Transparent:** Published pricing builds trust vs "call for quote"

---

## 11. Winston AI Strategy (Leveraging Locust)

### 11.1 What to Reuse from Locust

The existing Locust codebase provides a sophisticated multi-agent AI framework. For Winston, we'll adapt:

| Locust Component | Adaptation for Winston |
|------------------|------------------------|
| Agent base classes (agents/base.py) | Simplify for single-agent use |
| LLM backend switching (llm/) | Keep Claude + Ollama support |
| Memory/embeddings (memory/) | Use for semantic search over tenant data |
| Error handling (ceo/errors.py) | Reuse circuit breaker, execution guard |

**Remove from Winston:**
- Software development agents (CTO, CPO, etc.)
- Code generation modules
- Research workflows
- Git integration

### 11.2 Winston Tool Design

```python
@tool
async def search_invoices(query: str, tenant_id: str, limit: int = 10):
    """Search invoices by vendor name, amount, or status.

    Examples:
    - "invoices from Acme Corp"
    - "pending invoices over $10,000"
    """
    pass

@tool
async def list_pending_approvals(user_id: str, tenant_id: str):
    """List all invoices pending the user's approval."""
    pass

@tool
async def vendor_lookup(search: str, tenant_id: str):
    """Search vendors by name or tax ID."""
    pass

@tool
async def run_report(report_type: str, date_range: DateRange, tenant_id: str):
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
| 1.0-3.0 | Jan-Feb 2026 | CTO | Initial drafts and iterations |
| 4.0 | Feb 1, 2026 | CTO | Consolidated execution-ready version based on CEO vision |

**Sign-offs Required:**
- [ ] CEO Approval
- [ ] CPO Alignment Confirmation
- [ ] Engineering Lead Review

---

*This technical plan is the strategic execution document for Bill Forge. It supersedes all previous versions and will be updated as decisions evolve based on pilot feedback.*
