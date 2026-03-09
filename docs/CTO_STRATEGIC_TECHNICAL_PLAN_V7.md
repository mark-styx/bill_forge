# Bill Forge: CTO Strategic Technical Plan

**Version:** 7.0
**Date:** February 2, 2026
**Author:** Chief Technology Officer
**Status:** Final Execution Plan
**Planning Horizon:** 12 Weeks (Q1 2026)

---

## Executive Summary

Bill Forge enters the AP automation market targeting an underserved segment: **mid-market companies (50-500 employees) processing 300-5,000 invoices monthly**. Enterprise solutions (Coupa, SAP Concur) are overbuilt and overpriced. SMB tools (BILL) lack workflow sophistication. We execute dramatically better on three dimensions: **speed, simplicity, and privacy**.

This plan provides complete technical guidance for the 12-week MVP build, leveraging the existing Locust codebase where appropriate and establishing clean architectural boundaries.

### Strategic Technical Position

**The fast, modular, privacy-first AP platform for growing companies.**

### Key Technical Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Backend Language | Rust (Axum 0.7+) | Sub-200ms API response, compile-time safety, memory efficiency |
| Frontend | Next.js 14+ (App Router) | Server components, TypeScript-first, excellent DX |
| Database Strategy | Database-per-tenant | Complete isolation, compliance-ready, data portability |
| OCR Strategy | Tesseract 5 primary, cloud fallback | Privacy-first positioning, cost optimization |
| AI Assistant | Adapt Locust framework | 60% effort reduction for Winston AI |

---

## 1. Technical Architecture Recommendations

### 1.1 System Architecture Overview

```
+--------------------------------------------------------------------------------+
|                            BILL FORGE PLATFORM                                  |
+--------------------------------------------------------------------------------+
|                                                                                 |
|  PRESENTATION LAYER                                                            |
|  +----------------------------------------------------------------------------+ |
|  |                         NEXT.JS 14+ FRONTEND                               | |
|  |                                                                            | |
|  |  /login         /invoices        /approvals       /vendors      /reports  | |
|  |  +--------+    +-----------+    +-----------+    +--------+    +--------+ | |
|  |  | Auth   |    | Upload    |    | Inbox     |    | Master |    | Dash   | | |
|  |  | SSO    |    | Preview   |    | Actions   |    | Data   |    | Charts | | |
|  |  | MFA    |    | Correct   |    | SLA       |    | Tax    |    | Export | | |
|  |  +--------+    | Queues    |    | Email Act |    +--------+    +--------+ | |
|  |                +-----------+    +-----------+                              | |
|  |                                                                            | |
|  |  Stack: shadcn/ui | Tailwind CSS | TanStack Query | React Hook Form       | |
|  +----------------------------------------------------------------------------+ |
|                                       |                                         |
|                                       v                                         |
|  API LAYER                                                                     |
|  +----------------------------------------------------------------------------+ |
|  |                      RUST API GATEWAY (bf-api)                             | |
|  |                                                                            | |
|  |  Axum 0.7+ | JWT/Session Auth | Tenant Resolution | Rate Limiting | CORS  | |
|  |                                                                            | |
|  |  Request Flow: Log -> Auth -> Tenant -> RateLimit -> Route -> Handler      | |
|  +----------------------------------------------------------------------------+ |
|                                       |                                         |
|           +---------------------------+---------------------------+             |
|           v                           v                           v             |
|  SERVICE LAYER (Rust Crates)                                                   |
|      +--------------+          +--------------+          +--------------+      |
|      |  bf-invoice  |          | bf-workflow  |          |  bf-vendor   |      |
|      +--------------+          +--------------+          +--------------+      |
|      | Upload API   |          | Rule Engine  |          | Master Data  |      |
|      | OCR Pipeline |          | State Machine|          | Tax Storage  |      |
|      | Extraction   |          | Notifications|          | Fuzzy Match  |      |
|      | Queue Route  |          | Email Actions|          | Spend View   |      |
|      +--------------+          +--------------+          +--------------+      |
|           |                           |                           |             |
|           +---------------------------+---------------------------+             |
|                                       v                                         |
|  OCR LAYER                                                                     |
|  +----------------------------------------------------------------------------+ |
|  |                      bf-ocr (Provider Abstraction)                         | |
|  |                                                                            | |
|  |  +----------------+  +----------------+  +----------------+                | |
|  |  |  Tesseract 5   |  |  AWS Textract  |  | Google Vision  |                | |
|  |  |  (Local/Free)  |  |  (Cloud/$0.01) |  |  (Fallback)    |                | |
|  |  |   PRIMARY      |  |   ESCALATION   |  |  HANDWRITING   |                | |
|  |  +----------------+  +----------------+  +----------------+                | |
|  +----------------------------------------------------------------------------+ |
|                                                                                 |
+---------------------------------------------------------------------------------+
|                               DATA LAYER                                        |
|                                                                                 |
|  +------------------+  +------------------+  +------------------+               |
|  |  Control Plane   |  |   Tenant DBs     |  |  MinIO (S3)      |               |
|  |   PostgreSQL     |  |   PostgreSQL     |  |                  |               |
|  |                  |  |                  |  |  /tenant-a/      |               |
|  | - tenants        |  |  bf_tenant_acme: |  |    /invoices/    |               |
|  | - users          |  |  - invoices      |  |    /tax-docs/    |               |
|  | - api_keys       |  |  - vendors       |  |                  |               |
|  | - subscriptions  |  |  - workflows     |  |  /tenant-b/      |               |
|  |                  |  |  - audit_log     |  |    /invoices/    |               |
|  +------------------+  +------------------+  +------------------+               |
|                                                                                 |
|  +------------------+  +------------------+                                     |
|  |  DuckDB          |  |  Redis 7         |                                     |
|  |  (Per-Tenant)    |  |                  |                                     |
|  |                  |  | - Sessions       |                                     |
|  | - metrics        |  | - Rate limits    |                                     |
|  | - aggregates     |  | - Job queues     |                                     |
|  | - reports        |  | - Pub/Sub        |                                     |
|  +------------------+  +------------------+                                     |
|                                                                                 |
+---------------------------------------------------------------------------------+
|                           WINSTON AI SERVICE (Python)                           |
|  +----------------------------------------------------------------------------+ |
|  |  Adapted from Locust: agents/, llm/, memory/, workflows/                   | |
|  |                                                                            | |
|  |  +------------------+  +------------------+  +------------------+          | |
|  |  | Chat Interface   |  |  Tool Registry   |  | Tenant Context   |          | |
|  |  | (Claude/Ollama)  |  | (Invoice Search, |  | (Embeddings,     |          | |
|  |  |                  |  |  Vendor Lookup,  |  |  Semantic Search)|          | |
|  |  |                  |  |  Run Report)     |  |                  |          | |
|  |  +------------------+  +------------------+  +------------------+          | |
|  |                                                                            | |
|  |  Communicates with bf-api via REST | Tenant isolation via API auth         | |
|  +----------------------------------------------------------------------------+ |
+---------------------------------------------------------------------------------+
```

### 1.2 Database-per-Tenant Architecture

**Decision:** Database-per-tenant isolation (not row-level security)

**Why This Matters for Mid-Market:**
- Complete data isolation satisfies healthcare, legal, and financial compliance
- Per-tenant backup/restore for disaster recovery
- Easy data portability (customers can export their entire database)
- No cross-tenant query risk (defense in depth)
- Simplified GDPR/CCPA deletion (drop database)

**Implementation Pattern:**
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
                        |  |  - privacy_mode: true/false   |  |
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

**Connection Management:**
- Connection pooling via `deadpool-postgres` (per-tenant pools)
- Lazy connection creation (connect on first request)
- Maximum 10 connections per tenant in production
- Automatic cleanup of idle connections after 5 minutes

### 1.3 OCR Pipeline Architecture

**Pipeline Flow:**
```
UPLOAD -> PREPROCESS -> CLASSIFY -> PROVIDER ROUTER -> EXTRACT -> VALIDATE -> CONFIDENCE ROUTER -> QUEUE
```

**Provider Router Logic:**

| Condition | Provider | Rationale |
|-----------|----------|-----------|
| Tenant privacy_mode = TRUE | Tesseract 5 only | No cloud data transfer |
| Standard invoice, good quality | Tesseract 5 | Free, fast, sufficient |
| Low confidence (<75%) on Tesseract | AWS Textract | Better table extraction |
| Still low confidence (<75%) | Google Vision | Handwriting support |
| All providers <70% | Error Queue | Requires manual entry |

**Confidence Routing (Three-Tier):**

| Overall Confidence | Queue | User Action |
|-------------------|-------|-------------|
| >= 85% | AP Queue | Auto-route to approval workflow |
| 70-84% | Review Queue | Human verifies flagged fields only |
| < 70% | Error Queue | Full manual entry required |

**Confidence Calculation:**
```
Overall = (amount_conf * 0.30) + (vendor_conf * 0.25) +
          (invoice_num_conf * 0.20) + (date_conf * 0.15) +
          (currency_conf * 0.10)
```

**Preprocessing Steps:**
1. Image format normalization (convert to PNG)
2. Resolution scaling (300 DPI minimum)
3. Deskew correction (Hough transform)
4. Contrast enhancement (adaptive thresholding)
5. Noise reduction (median filter)

### 1.4 Approval Workflow Engine

A configurable state machine with JSON-defined rules.

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
    { "if": "amount_mismatch", "action": "exception_queue" }
  ]
}
```

### 1.5 Email Approval System (Key Differentiator)

Email approvals without login are a primary competitive advantage.

**Security Model:**

| Security Measure | Implementation |
|-----------------|----------------|
| Token signing | HMAC-SHA256 with rotating secret key |
| Expiration | 72 hours from generation |
| Single use | Token invalidated after action |
| Audit trail | IP address, user agent, timestamp logged |
| Rate limiting | Max 10 attempts per token per minute |
| Replay protection | Nonce stored and checked |

**Token Structure:**
```
https://app.billforge.io/api/v1/actions/{base64(invoice_id:action:nonce:expiry:hmac)}
```

**Email Content:**
- Invoice summary (vendor, amount, due date, invoice number)
- One-click Approve/Reject buttons (distinct colors)
- View full invoice link (requires login)
- Clear sender identity and Bill Forge branding
- Mobile-optimized layout

---

## 2. Technology Stack Decisions

### 2.1 Backend (Rust)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Web Framework | **Axum** | 0.7+ | Async-first, Tower middleware ecosystem |
| Async Runtime | **Tokio** | 1.x | Industry standard for Rust async |
| Serialization | **Serde** | Latest | De facto Rust standard |
| Database | **SQLx** | 0.7+ | Compile-time query checking, async |
| Migrations | **sqlx-cli** | 0.7+ | Integrated with SQLx |
| Validation | **validator** | Latest | Derive macros for validation |
| Error Handling | **thiserror + anyhow** | Latest | Typed errors with context |
| Logging | **tracing** | Latest | Structured, async-aware logging |
| Config | **config-rs** | Latest | Multi-source configuration |
| HTTP Client | **reqwest** | Latest | OCR provider API calls |
| UUID | **uuid** | Latest | Entity identifiers |
| Date/Time | **chrono** | Latest | Timestamps with timezone |
| Password | **argon2** | Latest | Secure password hashing |
| JWT | **jsonwebtoken** | Latest | Token-based auth |
| HMAC | **hmac + sha2** | Latest | Email action tokens |
| Testing | **tokio-test, wiremock** | Latest | Async tests with mocking |
| OCR | **tesseract-rs** | Latest | Local OCR bindings |

### 2.2 Frontend (Next.js)

| Component | Technology | Version | Rationale |
|-----------|------------|---------|-----------|
| Framework | **Next.js** | 14+ | App Router, React Server Components |
| Language | **TypeScript** | 5.x | Strict mode enabled |
| Styling | **Tailwind CSS** | 3.x | Utility-first, fast iteration |
| Components | **shadcn/ui** | Latest | Accessible, customizable primitives |
| Server State | **TanStack Query** | 5.x | Caching, optimistic updates |
| Forms | **React Hook Form + Zod** | Latest | Type-safe validation |
| Tables | **TanStack Table** | 8.x | Invoice and queue lists |
| Charts | **Recharts** | 2.x | Dashboard visualizations |
| Date Picker | **date-fns + react-day-picker** | Latest | Date handling |
| Notifications | **Sonner** | Latest | Toast notifications |
| Icons | **Lucide React** | Latest | Consistent iconography |
| Auth Client | **next-auth** | 5.x | Session management |
| API Client | Generated from **OpenAPI** | - | Type-safe API calls |
| File Upload | **react-dropzone** | Latest | Drag-drop upload |
| PDF Preview | **react-pdf** | Latest | Invoice preview |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| OLTP Database | **PostgreSQL 16** | Per-tenant isolation, JSONB, pg_trgm |
| Analytics DB | **DuckDB** | Embedded, fast aggregations, columnar |
| Document Storage | **MinIO** | S3-compatible, local-first development |
| Cache/Queue | **Redis 7** | Sessions, rate limiting, job queues |
| Search | **PostgreSQL FTS + pg_trgm** | Start simple, upgrade if needed |

### 2.4 OCR Providers

| Provider | Priority | Use Case | Cost/Page |
|----------|----------|----------|-----------|
| **Tesseract 5** | Primary | Local processing, privacy mode | Free |
| **AWS Textract** | Secondary | Complex layouts, tables | ~$0.015 |
| **Google Vision** | Tertiary | Handwriting, poor scans | ~$0.0015 |

### 2.5 Infrastructure

| Component | Technology | Environment |
|-----------|------------|-------------|
| Containers | **Docker** | All environments |
| Dev Orchestration | **Docker Compose** | Development |
| Prod Orchestration | **Kubernetes (EKS)** | Production |
| CI/CD | **GitHub Actions** | All environments |
| Secrets | **AWS Secrets Manager** | Production |
| Monitoring | **Prometheus + Grafana** | All environments |
| Tracing | **OpenTelemetry + Jaeger** | All environments |
| Email | **AWS SES** | Production |
| CDN | **CloudFront** | Production |
| DNS | **Route 53** | Production |

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

#### Week 1: Infrastructure & Project Setup

| Task | Deliverable | Priority |
|------|-------------|----------|
| Create monorepo structure | Cargo.toml workspace + pnpm-workspace.yaml | P0 |
| Docker Compose setup | PostgreSQL, Redis, MinIO running locally | P0 |
| CI/CD pipeline | GitHub Actions: lint, test, build on PR | P0 |
| Control plane schema | tenants, users, api_keys tables | P0 |
| bf-tenant crate | Create/list/delete tenants, DB provisioning | P0 |
| SQLx migrations | Migration infrastructure and runner | P0 |

#### Week 2: Auth & API Foundation

| Task | Deliverable | Priority |
|------|-------------|----------|
| JWT authentication | bf-auth crate: issue/verify tokens | P0 |
| API gateway scaffold | bf-api crate with health check endpoint | P0 |
| Tenant resolution middleware | Extract tenant from URL path | P0 |
| Next.js scaffold | App Router with shadcn/ui, login page | P0 |
| API client generation | OpenAPI to TypeScript client | P0 |
| Seed data/fixtures | Development test data | P1 |

**Phase 0 Exit Criteria:**
- [ ] Monorepo with `crates/` and `apps/web/`
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] `GET /health` returns 200 OK
- [ ] bf-tenant can create tenant databases dynamically
- [ ] bf-auth issues and verifies JWTs
- [ ] Next.js app renders login page
- [ ] CI pipeline passes on every PR

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Working OCR pipeline with confidence-based routing

#### Weeks 3-4: OCR Pipeline Core

| Task | Deliverable | Priority |
|------|-------------|----------|
| Document upload API | `POST /api/v1/{tenant}/invoices/upload` | P0 |
| S3 storage abstraction | bf-storage crate (MinIO compatible) | P0 |
| Tesseract integration | bf-ocr crate with local OCR | P0 |
| Field extraction | Vendor, invoice #, amount, date, currency | P0 |
| Confidence scoring | Per-field and overall confidence calculation | P0 |
| Queue data models | invoices, invoice_queue tables | P0 |
| Document preprocessing | Deskew, enhance, normalize | P1 |

#### Weeks 5-6: Capture UI & Vendor Matching

| Task | Deliverable | Priority |
|------|-------------|----------|
| Invoice upload UI | Drag-drop, multi-file, preview | P0 |
| OCR results display | Confidence badges per field | P0 |
| Manual correction UI | Inline edit with field highlighting | P0 |
| Vendor fuzzy matching | Levenshtein distance matching to master list | P0 |
| Vendor CRUD API | `GET/POST/PATCH /vendors` | P0 |
| Queue dashboard | AP queue, review queue, error queue views | P0 |
| Batch upload | Multiple invoices at once | P1 |

**Phase 1 Exit Criteria:**
- [ ] `POST /api/v1/{tenant}/invoices/upload` accepts PDF/images
- [ ] `GET /api/v1/{tenant}/invoices/{id}` returns extracted data
- [ ] `GET /api/v1/{tenant}/queues/ap` lists high-confidence invoices
- [ ] `GET /api/v1/{tenant}/queues/errors` lists low-confidence invoices
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
| Approval state machine | States: pending, l1_wait, approved, rejected, held | P0 |
| Rule configuration API | `GET/POST /workflows` | P0 |
| Approval inbox UI | Pending items with bulk select | P0 |
| Approve/reject/hold actions | Action buttons + API endpoints | P0 |
| Amount-based routing | Threshold rules by dollar amount | P0 |

#### Weeks 9-10: Email Actions & Audit

| Task | Deliverable | Priority |
|------|-------------|----------|
| Signed token generation | HMAC tokens with 72h expiration | P0 |
| Email approval endpoints | `GET /api/v1/actions/{token}/approve` | P0 |
| Email service integration | AWS SES for notifications | P0 |
| Audit trail logging | All actions logged with IP, timestamp | P0 |
| Delegation config | Out-of-office routing rules | P1 |
| SLA tracking | Time-in-queue calculation and alerts | P1 |
| Bulk operations | Batch approve/reject | P1 |

**Phase 2 Exit Criteria:**
- [ ] `POST /api/v1/{tenant}/workflows` creates approval rules
- [ ] `POST /api/v1/{tenant}/invoices/{id}/approve` works
- [ ] `GET /api/v1/actions/{token}/approve` works without authentication
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
| Production environment | Kubernetes deployment on AWS EKS | P0 |
| Security audit | Penetration testing, SAST/DAST | P0 |
| Load testing | 100 invoices/minute sustained | P0 |
| Monitoring + alerting | Prometheus/Grafana dashboards | P0 |
| API documentation | OpenAPI spec published | P0 |
| User guides | Help documentation for end users | P1 |

#### Week 12: Customer Onboarding

| Task | Deliverable | Priority |
|------|-------------|----------|
| Data migration tooling | Import vendors/invoices from CSV | P0 |
| White-glove onboarding | Personal setup for each pilot | P0 |
| Feedback mechanisms | In-app feedback, weekly calls | P0 |
| Bug triage process | P0/P1/P2 classification and response SLAs | P0 |
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
| **OCR accuracy below 85%** | Medium | High | Multi-provider fallback; human review loop; collect training data for improvement |
| **Rust learning curve** | Medium | Medium | Pair programming; code reviews; leverage async patterns from Locust |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; automated security scanning |
| **Email approval token security** | Medium | High | HMAC with 72h expiration; one-time use; rate limiting; IP audit trail; nonce |
| **DuckDB scalability limits** | Medium | Medium | Partition by month; archive >12 months; evaluate ClickHouse if needed |
| **Connection pool exhaustion** | Medium | Medium | Per-tenant pools with limits; lazy connections; monitoring and alerting |

### 4.2 Product/Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep delays MVP** | High | High | Strict anti-goals; weekly scope review; "Phase 2" is default answer |
| **Pilot customer churn** | Medium | High | Weekly check-ins; <24h bug response; dedicated Slack channel |
| **ERP integration complexity** | High | Medium | Start with QuickBooks only; use official SDK; defer others to Phase 2+ |
| **Competitor response** | Medium | Medium | Move fast; differentiate on UX; build switching costs via workflow config |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Medium | High | Multi-AZ deployment; health checks; auto-failover |
| **Key person dependency** | High | High | ADRs for all decisions; pair programming; knowledge sharing |
| **Security incident** | Low | Critical | Pen testing; incident response plan; security monitoring |

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
|  |  - Database schema design and SQLx queries                              | |
|  |  - OCR pipeline and accuracy optimization                               | |
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
|  |  - Winston AI adaptation from Locust (Phase 3+)                          | |
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
| Backend Engineer (Rust) #1 | P0 | Week 1 | Rust, Axum, PostgreSQL, async programming |
| Backend Engineer (Rust) #2 | P0 | Week 1 | Rust, API design, SQLx |
| Frontend Engineer | P0 | Week 2 | Next.js 14+, TypeScript, Tailwind |
| DevOps Engineer | P1 | Week 1 | Docker, Kubernetes, GitHub Actions, AWS |
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
| Monitoring (Grafana Cloud) | $0 | $100 |
| Domain + SSL | $10 | $10 |
| **TOTAL** | **$290/mo** | **$1,610/mo** |

---

## 6. Leveraging the Existing Locust Codebase

### 6.1 Locust Overview

The existing codebase at `/Users/mark/sentinel/locust/` contains a sophisticated multi-agent AI framework (69+ Python files, ~26,900 lines of code). Key components relevant to Bill Forge:

| Locust Component | Location | Reuse Potential |
|------------------|----------|-----------------|
| Agent orchestration | `src/locust/workflows/`, `src/locust/agents/` | High - adapt for Winston |
| LLM backends (Claude + Ollama) | `src/locust/llm/` | High - direct reuse |
| Memory/embeddings | `src/locust/memory/embeddings.py` | High - semantic search |
| Circuit breaker patterns | `src/locust/ceo/errors.py` | High - resilient OCR routing |
| Config management | `src/locust/config.py` | Medium - reference patterns |
| Persistence layer | `src/locust/storage/`, `src/locust/persistence/` | Medium - reference patterns |

### 6.2 What to Reuse for Winston

The Winston AI Assistant will be a Python service adapted from Locust:

**Components to Keep:**
- `llm/claude.py`, `llm/ollama.py` - LLM backend abstraction
- `llm/factory.py` - Backend factory pattern
- `memory/embeddings.py` - Semantic search over tenant data
- `ceo/errors.py` - Circuit breaker for external API resilience
- `config.py` - Configuration patterns (adapt for multi-tenant)

**Components to Remove:**
- `agents/executive.py`, `agents/planning.py`, `agents/execution.py` - Software dev agents
- `codegen/` - Code generation (not needed)
- `research/` - Research workflows (not needed)
- `git/` - Git integration (not needed)
- `workflows/engineering.py` - Engineering execution (not needed)

**New Components to Build:**
- `winston/agent.py` - Conversational agent for invoice queries
- `winston/tools.py` - Bill Forge-specific tool definitions
- `winston/context.py` - Tenant context management
- `winston/api_client.py` - Bill Forge API integration

### 6.3 Winston Tool Registry

```python
@tool
async def search_invoices(query: str, tenant_id: str, limit: int = 10):
    """Search invoices by vendor name, amount, or status."""
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

@tool
async def approve_invoice(invoice_id: str, user_id: str, tenant_id: str):
    """Approve an invoice (with confirmation prompt)."""
    pass

@tool
async def get_invoice_details(invoice_id: str, tenant_id: str):
    """Get full details of a specific invoice."""
    pass
```

### 6.4 Separation of Concerns

```
Bill Forge Platform:
├── bill-forge/              # NEW REPOSITORY
│   ├── crates/              # Rust backend (bf-api, bf-invoice, etc.)
│   ├── apps/web/            # Next.js frontend
│   └── services/winston/    # Python AI service (adapted from Locust)
│
├── locust/                  # EXISTING REPOSITORY (reference)
│   └── src/locust/          # Source for Winston adaptation
```

**Key Principle:** Bill Forge core (Rust) is completely separate from Locust. Winston (Python) adapts Locust components and communicates with Bill Forge via REST APIs.

---

## 7. Monorepo Structure

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
│   │   └── src/
│   │       ├── upload.rs
│   │       ├── extraction.rs
│   │       ├── queue.rs
│   │       └── models.rs
│   │
│   ├── bf-workflow/                # Approval workflow engine
│   │   └── src/
│   │       ├── engine.rs
│   │       ├── state_machine.rs
│   │       ├── rules.rs
│   │       └── email_actions.rs
│   │
│   ├── bf-ocr/                     # OCR provider abstraction
│   │   └── src/
│   │       ├── provider.rs
│   │       ├── tesseract.rs
│   │       ├── textract.rs
│   │       ├── vision.rs
│   │       └── confidence.rs
│   │
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
│           │   ├── (auth)/
│           │   │   └── login/
│           │   ├── (dashboard)/
│           │   │   ├── invoices/
│           │   │   ├── approvals/
│           │   │   ├── vendors/
│           │   │   └── reports/
│           │   └── layout.tsx
│           ├── components/         # UI components
│           │   ├── ui/             # shadcn/ui components
│           │   ├── invoice/
│           │   ├── approval/
│           │   └── layout/
│           └── lib/                # Utilities
│               ├── api.ts          # Generated API client
│               └── auth.ts
│
├── packages/                       # Shared JS packages
│   ├── ui/                         # Extended shadcn/ui
│   └── api-client/                 # Generated TypeScript client
│
├── services/                       # Additional services
│   └── winston/                    # AI assistant (Phase 3+)
│       ├── pyproject.toml          # Python (adapted from Locust)
│       └── src/winston/
│           ├── agent.py            # Adapted from locust/agents/
│           ├── tools.py            # Bill Forge specific tools
│           ├── llm/                # From locust/llm/
│           └── memory/             # From locust/memory/
│
├── migrations/                     # Database migrations
│   ├── control-plane/
│   │   ├── 001_create_tenants.sql
│   │   ├── 002_create_users.sql
│   │   └── 003_create_api_keys.sql
│   └── tenant/
│       ├── 001_create_vendors.sql
│       ├── 002_create_invoices.sql
│       ├── 003_create_workflows.sql
│       └── 004_create_audit_log.sql
│
├── infra/                          # Infrastructure as code
│   ├── terraform/
│   │   ├── main.tf
│   │   ├── eks.tf
│   │   ├── rds.tf
│   │   └── variables.tf
│   └── kubernetes/
│       ├── base/
│       └── overlays/
│           ├── staging/
│           └── production/
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

## 8. Core Database Schema

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

CREATE INDEX idx_tenants_slug ON tenants(slug);

-- Global users (can belong to multiple tenants)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    status VARCHAR(20) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);

-- User-tenant membership
CREATE TABLE tenant_users (
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) DEFAULT 'member',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (tenant_id, user_id)
);

-- API keys for programmatic access
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(255) NOT NULL,
    permissions JSONB DEFAULT '[]',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);
```

### Per-Tenant Schema (bf_tenant_{slug})

```sql
-- Enable trigram extension for fuzzy matching
CREATE EXTENSION IF NOT EXISTS pg_trgm;

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
CREATE INDEX idx_vendors_status ON vendors(status);

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
    workflow_state VARCHAR(30) DEFAULT 'pending',
    ocr_confidence DECIMAL(5, 2),
    ocr_provider VARCHAR(50),
    document_path VARCHAR(500),
    extracted_data JSONB,
    field_confidences JSONB,
    queue VARCHAR(20) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_invoices_status ON invoices(status);
CREATE INDEX idx_invoices_queue ON invoices(queue);
CREATE INDEX idx_invoices_vendor_id ON invoices(vendor_id);
CREATE INDEX idx_invoices_workflow_state ON invoices(workflow_state);
CREATE INDEX idx_invoices_created_at ON invoices(created_at);

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

CREATE INDEX idx_line_items_invoice ON invoice_line_items(invoice_id);

-- Approval workflows
CREATE TABLE approval_workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    rules JSONB NOT NULL,
    is_default BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Approval steps
CREATE TABLE approval_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id) ON DELETE CASCADE,
    workflow_id UUID REFERENCES approval_workflows(id),
    step_number INTEGER,
    approver_id UUID,
    delegate_id UUID,
    status VARCHAR(20) DEFAULT 'pending',
    action_at TIMESTAMPTZ,
    action_method VARCHAR(20),
    comments TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_approval_steps_approver_status
    ON approval_steps(approver_id, status);
CREATE INDEX idx_approval_steps_invoice ON approval_steps(invoice_id);

-- Email action tokens
CREATE TABLE email_action_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id) ON DELETE CASCADE,
    action VARCHAR(20) NOT NULL,
    token_hash VARCHAR(255) NOT NULL,
    nonce VARCHAR(64) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    used_from_ip INET,
    used_user_agent TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_email_tokens_hash ON email_action_tokens(token_hash);
CREATE INDEX idx_email_tokens_nonce ON email_action_tokens(nonce);

-- User delegation (out-of-office)
CREATE TABLE user_delegations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    delegator_id UUID NOT NULL,
    delegate_id UUID NOT NULL,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_delegations_delegator ON user_delegations(delegator_id, is_active);

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
CREATE INDEX idx_audit_log_actor ON audit_log(actor_id);
```

---

## 9. API Design

### 9.1 URL Pattern

```
/api/v1/{tenant}/resource/{id}
```

Example: `/api/v1/acme-corp/invoices/inv_123abc`

### 9.2 Response Format

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
    "field": null,
    "request_id": "req_abc123"
  }
}
```

### 9.3 Key Endpoints (MVP)

```
# Authentication
POST   /api/v1/auth/login
POST   /api/v1/auth/refresh
POST   /api/v1/auth/logout
GET    /api/v1/auth/me

# Invoice Capture
POST   /api/v1/{tenant}/invoices/upload
GET    /api/v1/{tenant}/invoices
GET    /api/v1/{tenant}/invoices/{id}
PATCH  /api/v1/{tenant}/invoices/{id}
POST   /api/v1/{tenant}/invoices/{id}/reprocess
DELETE /api/v1/{tenant}/invoices/{id}

# Queues
GET    /api/v1/{tenant}/queues/ap
GET    /api/v1/{tenant}/queues/review
GET    /api/v1/{tenant}/queues/errors
POST   /api/v1/{tenant}/queues/{queue_type}/{id}/move

# Approvals
GET    /api/v1/{tenant}/approvals/pending
GET    /api/v1/{tenant}/approvals/pending/count
POST   /api/v1/{tenant}/invoices/{id}/approve
POST   /api/v1/{tenant}/invoices/{id}/reject
POST   /api/v1/{tenant}/invoices/{id}/hold
POST   /api/v1/{tenant}/invoices/bulk/approve
POST   /api/v1/{tenant}/invoices/bulk/reject

# Email Actions (signed tokens, no auth required)
GET    /api/v1/actions/{signed_token}
POST   /api/v1/actions/{signed_token}/confirm

# Vendors
GET    /api/v1/{tenant}/vendors
POST   /api/v1/{tenant}/vendors
GET    /api/v1/{tenant}/vendors/{id}
PATCH  /api/v1/{tenant}/vendors/{id}
GET    /api/v1/{tenant}/vendors/search?q={query}

# Workflows
GET    /api/v1/{tenant}/workflows
POST   /api/v1/{tenant}/workflows
GET    /api/v1/{tenant}/workflows/{id}
PATCH  /api/v1/{tenant}/workflows/{id}
DELETE /api/v1/{tenant}/workflows/{id}

# User Settings
GET    /api/v1/{tenant}/users/me/delegation
POST   /api/v1/{tenant}/users/me/delegation
DELETE /api/v1/{tenant}/users/me/delegation

# Audit
GET    /api/v1/{tenant}/audit?entity_type={type}&entity_id={id}

# Reports (basic dashboard)
GET    /api/v1/{tenant}/reports/summary
GET    /api/v1/{tenant}/reports/processing-times
GET    /api/v1/{tenant}/reports/queue-status
```

### 9.4 Rate Limiting

| Endpoint Category | Rate Limit |
|-------------------|------------|
| Authentication | 10 req/min per IP |
| Invoice Upload | 100 req/min per tenant |
| Email Actions | 10 req/min per token |
| Read operations | 1000 req/min per tenant |
| Write operations | 200 req/min per tenant |

---

## 10. Success Metrics

### 10.1 Technical Metrics (3-Month Horizon)

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

### 10.2 Business Metrics (3-Month Horizon)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Pilot Customers** | 5 | Actively using platform |
| **Invoices Processed** | 3,500+ | Total across all pilots |
| **Customer NPS** | >= 50 | Bi-weekly survey |
| **Pilot-to-Paid Intent** | >= 60% | Conversion conversations |
| **Email Approval Success** | >= 95% | Successful / Total attempts |

### 10.3 Operational Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deployment Frequency** | Daily | Deploys to staging |
| **Mean Time to Recovery** | < 1 hour | Incident to resolution |
| **Change Failure Rate** | < 15% | Deploys requiring rollback |

---

## 11. Answers to CEO's Strategic Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**Strengths:**
- 20+ years in Nordic/European markets with deep SAP/Oracle integrations
- Mature workflow engine for complex multinational scenarios
- Established customer base provides stability proof
- Strong presence in enterprise manufacturing

**Weaknesses (Our Opportunities):**
- UI described as "slow" and "clunky" in customer reviews (3-5 second page loads)
- Limited AI/ML innovation in recent years
- Opaque "call for quote" pricing model
- Poor mobile experience
- Slow customer support response times
- High implementation costs ($50K+)

**Bill Forge Differentiation:**

| Dimension | Palette | Bill Forge | Our Advantage |
|-----------|---------|------------|---------------|
| UI Speed | Multi-second loads | Sub-second | 10x faster experience |
| Setup Time | Weeks/months | Hours/days | Self-service onboarding |
| Pricing | "Call for quote" | Published online | Trust and transparency |
| OCR | Cloud-only | Local-first option | Privacy positioning |
| Approvals | Login required | Email (no login) | Frictionless UX |

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence | Queue | User Action |
|------------|-------|-------------|
| **>= 85%** | AP Queue | Auto-route to approval workflow |
| **70-84%** | Review Queue | Human verifies flagged fields only |
| **< 70%** | Error Queue | Full manual entry required |

**Implementation Details:**
- Calculate overall confidence as weighted average of field confidences
- Weight amount (30%) and vendor_name (25%) higher - these are critical fields
- Store per-field confidence for granular review UI
- Collect corrections as training data for future optimization
- Allow tenant-configurable thresholds (some customers want stricter/looser routing)

### Q3: Which ERP integration should we prioritize first?

**Recommendation: QuickBooks Online (Priority 1)**

| ERP | Priority | Complexity | Target Completion |
|-----|----------|------------|-------------------|
| **QuickBooks Online** | 1 | Low | Phase 2 (Month 4) |
| NetSuite | 2 | Medium | Phase 3 (Month 6) |
| Sage Intacct | 3 | Medium | Phase 3 (Month 7) |
| Dynamics 365 | 4 | High | 2027 |

**Why QuickBooks First:**
- Largest addressable market for 50-500 employee companies (7M+ businesses)
- Simplest REST API with excellent documentation and OAuth 2.0
- Enables QuickBooks ProAdvisor partnership channel (75K+ potential referral partners)
- Direct alignment with primary ICP (AP managers at growing companies)
- App Store listing provides organic discovery

### Q4: What approval workflow patterns are most common?

**Research-Based Patterns:**

1. **Amount-Based Tiers (85% adoption)** - MVP Priority
   ```
   < $5,000:      Auto-approve (if vendor known)
   $5K - $25K:    Manager approval
   $25K - $50K:   Director/VP approval
   > $50K:        CFO/Controller approval
   ```

2. **Exception-Only Review (65%)** - MVP Priority
   - Clean invoices (known vendor, expected amount) auto-approve
   - Exceptions (new vendor, amount mismatch) route to review queue

3. **Department/Cost Center (45%)** - Phase 2

4. **Dual Approval (30%)** - Phase 2

**MVP Implementation Focus:** Amount-based tiers + exception routing

### Q5: How do competitors handle multi-currency?

**Common Approaches:**
- Store original currency + converted base currency
- Daily rate sync from ECB, Open Exchange Rates, or XE
- Allow manual rate override for large transactions
- Display both currencies in all views
- Post to ERP in base currency only

**Bill Forge MVP Approach:**
- Capture currency from invoice as metadata
- Support: USD, EUR, GBP, CAD (covers 80%+ of mid-market)
- Convert for display totals using daily rates (Open Exchange Rates API)
- Store both original and converted amounts
- Send base currency amount to ERP
- Flag variance >2% between captured and current rate for review
- **Defer full multi-currency GL posting to Phase 3**

### Q6: What's the pricing model that resonates?

**Recommendation: Tiered Usage-Based Pricing**

| Tier | Monthly Base | Invoices Included | Overage | Target Customer |
|------|--------------|-------------------|---------|-----------------|
| **Starter** | $299 | 500 | $0.75/inv | Early adopters, testing |
| **Growth** | $799 | 2,000 | $0.50/inv | Primary ICP (Sarah) |
| **Scale** | $1,999 | 10,000 | $0.30/inv | Secondary ICP (Marcus) |
| **Enterprise** | Custom | Custom | Custom | 10K+ invoices |

**Why This Model Works:**
- **No per-seat pricing:** AP teams hate paying for each approver
- **Predictable base:** Finance can budget effectively
- **Scales with business:** Pricing aligned with value delivered
- **Transparent:** Published pricing builds trust vs "call for quote"

---

## 12. Architecture Decision Records

### ADR-001: Database-per-Tenant Isolation

**Status:** Accepted

**Decision:** Use database-per-tenant model (not row-level security)

**Context:** Mid-market customers in healthcare, legal, and financial sectors require complete data isolation for compliance.

**Consequences:**
- (+) Complete data isolation for compliance (HIPAA, SOC 2)
- (+) Per-tenant backup/restore capability
- (+) Easy data portability (export entire database)
- (+) Simplified GDPR deletion (drop database)
- (-) Higher connection overhead (mitigated by connection pooling)
- (-) More complex migrations (automated via migration runner)

### ADR-002: OCR Provider Strategy

**Status:** Accepted

**Decision:** Tesseract 5 as default, cloud providers for escalation

**Context:** Privacy-first positioning is a key differentiator. Some customers (healthcare, legal) require local-only processing.

**Consequences:**
- (+) Privacy-first positioning for regulated industries
- (+) Low cost for high-confidence invoices (Tesseract is free)
- (+) Local processing option (no cloud data transfer)
- (-) Slightly lower accuracy than cloud-only for complex documents

### ADR-003: Email Approval Security

**Status:** Accepted

**Decision:** HMAC-signed tokens, 72h expiration, one-time use, nonce for replay protection

**Context:** Email approvals without login are a key differentiator, but must be secure.

**Consequences:**
- (+) Frictionless approver experience (no login required)
- (+) Works on mobile without app installation
- (+) Complete audit trail (IP, timestamp, user agent)
- (-) Tokens can be forwarded (mitigated by audit logging and alerts)

### ADR-004: Dual Codebase Strategy

**Status:** Accepted

**Decision:** Separate Bill Forge (Rust) from Locust (Python), adapt Locust for Winston

**Context:** Rust provides performance and safety for the core platform. Locust's Python AI framework is ideal for Winston.

**Consequences:**
- (+) Clean separation of concerns
- (+) Optimal language for each purpose (Rust for performance, Python for AI)
- (+) Locust agent architecture directly reusable for Winston
- (-) Two codebases to maintain (mitigated by clear boundaries)

### ADR-005: Next.js App Router

**Status:** Accepted

**Decision:** Use Next.js 14+ with App Router (not Pages Router)

**Context:** App Router provides React Server Components, better performance, and is the future of Next.js.

**Consequences:**
- (+) Server components reduce client bundle size
- (+) Better SEO and initial load performance
- (+) Streaming and suspense support
- (-) Learning curve for team familiar with Pages Router

### ADR-006: Rust for Backend

**Status:** Accepted

**Decision:** Use Rust with Axum for the core Bill Forge backend

**Context:** Mid-market positioning requires sub-200ms API response times. Rust provides this while maintaining memory safety.

**Consequences:**
- (+) Exceptional performance (compile to native code)
- (+) Memory safety without garbage collection pauses
- (+) Strong type system catches bugs at compile time
- (+) Excellent async story with Tokio
- (-) Higher learning curve for team
- (-) Slower initial development velocity

---

## 13. Immediate Next Steps

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

## Appendix A: Local Development Setup

```bash
# Prerequisites
# - Rust 1.75+
# - Node.js 20+
# - pnpm 8+
# - Docker & Docker Compose

# Clone and setup
git clone https://github.com/billforge/bill-forge.git
cd bill-forge

# Copy environment template
cp .env.example .env

# Start infrastructure
docker compose up -d postgres redis minio

# Install dependencies
pnpm install
cargo build

# Run control plane migrations
cargo run -p bf-tenant -- migrate

# Create a test tenant
cargo run -p bf-tenant -- create --slug demo --name "Demo Company"

# Start services (separate terminals)
cargo run -p bf-api           # API: http://localhost:8080
pnpm --filter web dev         # Web: http://localhost:3000

# MinIO Console: http://localhost:9001 (minioadmin/minioadmin)

# Run tests
cargo test
pnpm test
```

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| AP | Accounts Payable |
| OCR | Optical Character Recognition |
| NPS | Net Promoter Score |
| PMF | Product-Market Fit |
| ICP | Ideal Customer Profile |
| P0/P1/P2 | Priority levels (P0 = launch-blocking) |
| HMAC | Hash-based Message Authentication Code |
| JWT | JSON Web Token |
| FTE | Full-Time Equivalent |
| ADR | Architecture Decision Record |

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0-6.0 | Jan-Feb 2026 | CTO | Initial drafts and iterations |
| 7.0 | Feb 2, 2026 | CTO | Final execution plan with Locust leverage strategy |

**Sign-offs Required:**
- [ ] CEO Approval
- [ ] CPO Alignment Confirmation
- [ ] Engineering Lead Review

---

*This technical plan is the authoritative execution document for Bill Forge. It supersedes all previous versions and will be updated based on pilot feedback and market learnings.*
