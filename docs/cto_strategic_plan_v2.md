# Bill Forge - CTO Strategic Technical Plan v2

**Date:** February 1, 2026
**Version:** 2.0
**Author:** CTO
**Status:** Final - Ready for Implementation

---

## Executive Summary

This plan provides the technical roadmap for Bill Forge, a B2B SaaS platform for simplified invoice processing targeting mid-market companies (10-1000 employees). Based on the CEO's vision and existing technical documentation, this plan consolidates decisions and provides actionable implementation guidance.

### Key Strategic Decisions (Confirmed)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Architecture** | Greenfield build | Existing Locust codebase is AI orchestration, not invoice processing |
| **Backend Language** | Rust (Axum) | CEO preference, performance for OCR, memory safety for multi-tenant |
| **Frontend** | Next.js 14+ | CEO preference, App Router, RSC for performance |
| **Database Strategy** | Database-per-tenant | Complete data isolation for compliance |
| **OCR Strategy** | Local-first (Tesseract) | Privacy differentiator, cloud fallback for accuracy |
| **Winston AI** | Adapt Locust (Phase 3) | Reuse LangGraph agent framework |

### Critical Path Summary

```
Weeks 1-2:   Foundation (Auth, API Gateway, Tenant Management)
Weeks 3-6:   Invoice Capture MVP (OCR Pipeline, Extraction, UI)
Weeks 7-10:  Invoice Processing MVP (Workflows, Approvals, Email Actions)
Weeks 11-12: Pilot Launch (5 customers, production deployment)
```

---

## 1. Technical Architecture Recommendations

### 1.1 High-Level System Architecture

```
                            ┌─────────────────────────────────────┐
                            │           LOAD BALANCER              │
                            │         (nginx / AWS ALB)            │
                            └──────────────┬──────────────────────┘
                                           │
              ┌────────────────────────────┼────────────────────────────┐
              │                            │                            │
              ▼                            ▼                            ▼
┌─────────────────────────┐  ┌─────────────────────────┐  ┌─────────────────────────┐
│      FRONTEND           │  │       API GATEWAY        │  │    EMAIL ACTIONS        │
│    (Next.js 14+)        │  │      (Rust/Axum)         │  │    (Rust/Axum)          │
│                         │  │                         │  │                         │
│ • Invoice Capture UI    │  │ • Tenant Resolution     │  │ • Signed Token Verify   │
│ • Approval Inbox        │  │ • JWT/API Key Auth      │  │ • One-click Approve     │
│ • Vendor Management     │  │ • Rate Limiting         │  │ • No Auth Required      │
│ • Analytics Dashboards  │  │ • Request Validation    │  │ • Audit Logging         │
│ • Winston Chat (P3)     │  │ • OpenTelemetry         │  │                         │
└─────────────────────────┘  └────────────┬────────────┘  └─────────────────────────┘
                                          │
         ┌────────────────────────────────┼────────────────────────────────┐
         │                                │                                │
         ▼                                ▼                                ▼
┌─────────────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
│   INVOICE SERVICE   │      │  WORKFLOW SERVICE   │      │   VENDOR SERVICE    │
│    (bf-invoice)     │      │   (bf-workflow)     │      │    (bf-vendor)      │
├─────────────────────┤      ├─────────────────────┤      ├─────────────────────┤
│ • Document Upload   │      │ • Rule Engine       │      │ • Master Data CRUD  │
│ • OCR Orchestration │      │ • State Machine     │      │ • Fuzzy Matching    │
│ • Field Extraction  │      │ • Email Notifs      │      │ • Tax Doc Storage   │
│ • Confidence Score  │      │ • SLA Tracking      │      │ • Spend Analysis    │
│ • Queue Routing     │      │ • Delegation        │      │ • Onboarding Flow   │
└─────────┬───────────┘      └─────────────────────┘      └─────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              OCR SERVICE (bf-ocr)                                │
│                                                                                  │
│   ┌──────────────┐      ┌──────────────┐      ┌──────────────┐                 │
│   │  Tesseract 5 │      │ AWS Textract │      │ Google Vision│                 │
│   │   (Primary)  │ ───► │  (Fallback)  │ ───► │  (Tertiary)  │                 │
│   │  confidence  │      │  confidence  │      │              │                 │
│   │    < 75%     │      │    < 75%     │      │              │                 │
│   └──────────────┘      └──────────────┘      └──────────────┘                 │
│                                                                                  │
│   Pipeline: Ingest → Preprocess → OCR → Extract → Validate → Route             │
└─────────────────────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                DATA LAYER                                        │
│                                                                                  │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐            │
│   │   Control Plane   │  │  Tenant DBs      │  │     Redis        │            │
│   │   PostgreSQL      │  │  PostgreSQL      │  │                  │            │
│   │                   │  │  (per-tenant)    │  │ • Sessions       │            │
│   │ • tenants         │  │                  │  │ • Rate Limits    │            │
│   │ • users           │  │ • invoices       │  │ • Job Queues     │            │
│   │ • api_keys        │  │ • vendors        │  │ • Pub/Sub        │            │
│   │ • subscriptions   │  │ • workflows      │  │                  │            │
│   └──────────────────┘  │ • audit_log      │  └──────────────────┘            │
│                         └──────────────────┘                                    │
│   ┌──────────────────┐  ┌──────────────────┐                                   │
│   │     DuckDB       │  │   S3/MinIO       │                                   │
│   │   (Analytics)    │  │  (Documents)     │                                   │
│   │                  │  │                  │                                   │
│   │ • Metrics        │  │ • /tenant/docs/  │                                   │
│   │ • Aggregates     │  │ • /tenant/tax/   │                                   │
│   │ • Reports        │  │ • /tenant/temp/  │                                   │
│   └──────────────────┘  └──────────────────┘                                   │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Tenant Isolation Model

**Database-per-tenant** provides:
- Complete data isolation (regulatory requirement for mid-market financial data)
- Independent backup/restore per tenant
- Easy data portability (tenant can export entire database)
- No complex row-level security policies
- Simplified compliance audits

**Connection Management Strategy:**
```rust
// Pseudocode for tenant connection management
struct TenantConnectionPool {
    control_plane: PgPool,          // Single pool for control plane
    tenant_pools: DashMap<TenantId, PgPool>,  // Lazy-loaded per tenant
    max_pools: usize,               // Cap total pools (e.g., 100)
    idle_timeout: Duration,         // Close idle tenant pools (e.g., 5 min)
}
```

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              OCR PIPELINE                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. INGEST                          2. PREPROCESS                               │
│  ┌──────────────────────┐           ┌──────────────────────┐                   │
│  │ • Accept upload       │  ───────► │ • Deskew (leptonica) │                   │
│  │ • Validate file type  │           │ • Enhance contrast   │                   │
│  │ • Virus scan          │           │ • Noise reduction    │                   │
│  │ • Store to S3         │           │ • Type detection     │                   │
│  │ • Create job record   │           │   (invoice/receipt)  │                   │
│  └──────────────────────┘           └──────────────────────┘                   │
│                                                │                                │
│  3. OCR PROVIDER ROUTER                        ▼                                │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                                                                          │   │
│  │   Tenant Privacy Mode?                                                   │   │
│  │   ├─ YES ──► Tesseract 5 ONLY (no cloud upload)                         │   │
│  │   │                                                                      │   │
│  │   └─ NO ──► Tesseract 5 (Primary)                                       │   │
│  │                └─ confidence < 75% ──► AWS Textract                      │   │
│  │                     └─ confidence < 75% ──► Google Vision                │   │
│  │                                                                          │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                │                                │
│  4. FIELD EXTRACTION                           ▼                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ Header Fields:                    Line Items (Phase 1.5):                │  │
│  │ • vendor_name                     • description                          │  │
│  │ • invoice_number                  • quantity                             │  │
│  │ • invoice_date                    • unit_price                           │  │
│  │ • due_date                        • amount                               │  │
│  │ • total_amount                    • gl_code (if detected)                │  │
│  │ • currency                                                               │  │
│  │ • tax_amount                                                             │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                │                                │
│  5. VALIDATION                                 ▼                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ • Required field presence (vendor, amount, date)                         │  │
│  │ • Format validation (date formats, currency parsing)                     │  │
│  │ • Duplicate detection (invoice # + vendor within tenant)                 │  │
│  │ • Vendor matching (fuzzy match against master list)                      │  │
│  │ • Amount sanity check (flag if > 2x vendor average)                      │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                │                                │
│  6. CONFIDENCE ROUTING                         ▼                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                                                                           │  │
│  │   Overall Score = weighted_average(field_confidences)                    │  │
│  │                                                                           │  │
│  │   ≥ 85%  ────────────────►  AP QUEUE (auto-route to approval workflow)  │  │
│  │                                                                           │  │
│  │   70-84% ────────────────►  REVIEW QUEUE (human verifies flagged fields)│  │
│  │                                                                           │  │
│  │   < 70%  ────────────────►  ERROR QUEUE (manual data entry required)    │  │
│  │                                                                           │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 1.4 Approval Workflow Engine

The workflow engine is a key differentiator. It must support:

1. **Amount-Based Tiers** (80% of mid-market companies use this)
2. **Exception-Only Review** (auto-approve if PO matches)
3. **Email Approvals** (no login required - unique feature)
4. **Delegation** (out-of-office routing)
5. **SLA Tracking** (escalation alerts)

**State Machine:**
```
                                    ┌───────────────┐
                                    │    PENDING    │
                                    └───────┬───────┘
                                            │
                    ┌───────────────────────┼───────────────────────┐
                    │                       │                       │
                    ▼                       ▼                       ▼
            ┌───────────────┐      ┌───────────────┐      ┌───────────────┐
            │  AUTO_APPROVE │      │   L1_PENDING  │      │ EXCEPTION_QUE │
            └───────┬───────┘      └───────┬───────┘      └───────┬───────┘
                    │                      │                       │
                    │              ┌───────┴───────┐               │
                    │              ▼               ▼               │
                    │      ┌───────────┐   ┌───────────┐          │
                    │      │ L2_PENDING│   │  REJECTED │◄─────────┤
                    │      └─────┬─────┘   └───────────┘          │
                    │            │                                 │
                    │    ┌───────┴───────┐                        │
                    │    ▼               ▼                        │
                    │ ┌───────────┐ ┌───────────┐                 │
                    │ │ L3_PENDING│ │  ON_HOLD  │                 │
                    │ └─────┬─────┘ └───────────┘                 │
                    │       │                                      │
                    ▼       ▼                                      │
            ┌───────────────────────────────────────────────────────┐
            │                      APPROVED                         │
            └───────────────────────────────────────────────────────┘
```

**Email Approval Flow:**
```
1. User receives email with signed action links
2. Links contain: tenant_id, invoice_id, action, approver_id, expiry, signature
3. Signature = HMAC-SHA256(payload, tenant_secret)
4. On click:
   - Verify signature
   - Check expiry (72 hours default)
   - Check one-time use (store used tokens in Redis)
   - Execute action
   - Log with IP address
   - Redirect to confirmation page
```

---

## 2. Technology Stack Decisions

### 2.1 Backend Stack (Rust)

| Component | Technology | Version | Crate |
|-----------|------------|---------|-------|
| Web Framework | Axum | 0.7+ | `axum` |
| Async Runtime | Tokio | 1.x | `tokio` |
| Serialization | Serde | Latest | `serde`, `serde_json` |
| Database | SQLx | 0.7+ | `sqlx` |
| Validation | Validator | Latest | `validator` |
| Error Handling | thiserror/anyhow | Latest | `thiserror`, `anyhow` |
| Logging/Tracing | Tracing | Latest | `tracing`, `tracing-subscriber` |
| Configuration | config | Latest | `config` |
| HTTP Client | reqwest | Latest | `reqwest` |
| UUID | uuid | Latest | `uuid` |
| Time | chrono | Latest | `chrono` |
| JWT | jsonwebtoken | Latest | `jsonwebtoken` |
| S3 | rust-s3 | Latest | `rust-s3` |
| OCR (Tesseract) | tesseract-rs | Latest | `tesseract` |

**Cargo Workspace Structure:**
```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "crates/bf-common",
    "crates/bf-tenant",
    "crates/bf-auth",
    "crates/bf-api",
    "crates/bf-invoice",
    "crates/bf-ocr",
    "crates/bf-workflow",
    "crates/bf-vendor",
    "crates/bf-storage",
    "crates/bf-analytics",
]
resolver = "2"

[workspace.dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

### 2.2 Frontend Stack (Next.js)

| Component | Technology | Version |
|-----------|------------|---------|
| Framework | Next.js | 14+ |
| Language | TypeScript | 5.x (strict) |
| Styling | Tailwind CSS | 3.x |
| Component Library | shadcn/ui | Latest |
| State Management | TanStack Query | 5.x |
| Forms | React Hook Form | 7.x |
| Validation | Zod | 3.x |
| Tables | TanStack Table | 8.x |
| Charts | Recharts | 2.x |
| Auth | NextAuth.js | 4.x |

### 2.3 Data Layer

| Component | Technology | Purpose |
|-----------|------------|---------|
| Control Plane DB | PostgreSQL 16+ | Tenant metadata, users, subscriptions |
| Tenant DB | PostgreSQL 16+ | Invoice data, workflows, audit (per-tenant) |
| Analytics | DuckDB | Fast aggregations, dashboards, reports |
| Cache/Queue | Redis 7+ | Sessions, rate limits, job queue, pub/sub |
| Document Storage | MinIO (S3-compat) | Invoice PDFs, tax documents |

### 2.4 OCR Providers

| Provider | Role | Accuracy | Cost | Privacy |
|----------|------|----------|------|---------|
| Tesseract 5 | Primary | 85-90% | Free | Local (best) |
| AWS Textract | Fallback | 95%+ | ~$0.01/page | Cloud |
| Google Vision | Tertiary | 93%+ | ~$0.0015/page | Cloud |

### 2.5 Infrastructure

| Component | Development | Production |
|-----------|-------------|------------|
| Containers | Docker | Docker |
| Orchestration | Docker Compose | Kubernetes |
| CI/CD | GitHub Actions | GitHub Actions |
| Secrets | .env files | AWS Secrets Manager |
| Monitoring | Local Grafana | Grafana Cloud |
| Tracing | Jaeger (local) | AWS X-Ray / Jaeger |
| Email | Mailhog (local) | AWS SES / Resend |

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)

**Goal:** Project scaffolding and core infrastructure

#### Week 1: Repository and Infrastructure
| Task | Owner | Deliverable |
|------|-------|-------------|
| Create monorepo structure | Backend | Cargo workspace + pnpm workspace |
| Docker Compose setup | DevOps | PostgreSQL, Redis, MinIO running |
| CI/CD pipeline | DevOps | GitHub Actions (lint, test, build) |
| Control plane schema | Backend | tenants, users, api_keys tables |
| bf-tenant crate | Backend | Create/list/get tenant APIs |
| SQLx migrations setup | Backend | Migration infrastructure |

#### Week 2: Auth and API Gateway
| Task | Owner | Deliverable |
|------|-------|-------------|
| bf-auth crate | Backend | JWT issue/verify, API key validation |
| bf-api crate | Backend | Axum gateway with middleware |
| Tenant resolution | Backend | Extract tenant from URL path |
| Rate limiting | Backend | Per-tenant limits via Redis |
| Next.js scaffold | Frontend | App with shadcn/ui, login page |
| API client generation | Frontend | TypeScript client from OpenAPI |

**Phase 0 Exit Criteria:**
- [ ] `docker-compose up` starts all infrastructure
- [ ] `cargo test` passes for all crates
- [ ] `POST /api/v1/tenants` creates a tenant database
- [ ] `POST /api/v1/auth/login` returns JWT
- [ ] Next.js app displays login page
- [ ] CI pipeline green on main branch

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Goal:** Working OCR pipeline with manual review capability

#### Week 3-4: OCR Pipeline
| Task | Owner | Deliverable |
|------|-------|-------------|
| bf-storage crate | Backend | S3 upload/download abstraction |
| Document upload API | Backend | `POST /api/v1/{tenant}/invoices/upload` |
| bf-ocr crate | Backend | Tesseract integration |
| Preprocessing pipeline | Backend | Deskew, enhance, type detection |
| Field extraction | Backend | vendor, invoice #, amount, date |
| Confidence scoring | Backend | Weighted average of field scores |
| Queue data model | Backend | AP queue, review queue, error queue |

#### Week 5-6: UI and Vendor Matching
| Task | Owner | Deliverable |
|------|-------|-------------|
| Upload UI | Frontend | Drag-drop with preview |
| OCR results view | Frontend | Show extracted fields with confidence |
| Manual correction | Frontend | Edit fields, highlight low confidence |
| bf-vendor crate | Backend | Vendor CRUD APIs |
| Fuzzy matching | Backend | Trigram similarity matching |
| Queue dashboard | Frontend | List invoices by queue status |

**Phase 1 Exit Criteria:**
- [ ] Upload PDF → OCR extracts fields in <3 seconds
- [ ] Confidence routing: ≥85% → AP, 70-84% → Review, <70% → Error
- [ ] Manual correction saves changes
- [ ] Vendor fuzzy matching suggests top 3 matches
- [ ] OCR accuracy ≥85% on test set of 50 invoices

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Goal:** Approval workflows with email actions

#### Week 7-8: Workflow Engine
| Task | Owner | Deliverable |
|------|-------|-------------|
| bf-workflow crate | Backend | Rule engine, state machine |
| Rule configuration API | Backend | Create/update workflow rules |
| Approval state machine | Backend | State transitions with validation |
| Approval inbox | Frontend | Pending items grouped by priority |
| Approve/reject/hold | Frontend | Action buttons with confirmation |

#### Week 9-10: Email Actions and Audit
| Task | Owner | Deliverable |
|------|-------|-------------|
| Signed token generation | Backend | HMAC tokens with expiry |
| Email action endpoints | Backend | `GET /api/v1/actions/{token}/approve` |
| Email service integration | Backend | AWS SES or Resend |
| Email templates | Backend | Approval request emails |
| Delegation config | Frontend | Out-of-office routing UI |
| SLA tracking | Backend | Deadline alerts, escalation |
| Audit log | Backend | Every action logged with metadata |
| Bulk operations | Frontend | Select multiple → batch approve |

**Phase 2 Exit Criteria:**
- [ ] Amount-based workflow routes invoices correctly
- [ ] Email approve/reject works without login
- [ ] Delegation routes to backup approver
- [ ] SLA alerts trigger at 24h/48h/72h
- [ ] Audit log captures all actions with actor/IP/timestamp

### Phase 3: Pilot Launch (Weeks 11-12)

**Goal:** Deploy to 5 pilot customers

#### Week 11: Production Readiness
| Task | Owner | Deliverable |
|------|-------|-------------|
| Kubernetes deployment | DevOps | Staging + production manifests |
| Security audit | External | Penetration test report |
| Load testing | QA | 100 invoices/minute target |
| Monitoring setup | DevOps | Grafana dashboards, alerts |
| API documentation | Backend | OpenAPI spec published |
| User guides | Product | Help center content |

#### Week 12: Customer Onboarding
| Task | Owner | Deliverable |
|------|-------|-------------|
| Migration tooling | Backend | Import existing invoices/vendors |
| Pilot onboarding | Product | 5 customers configured |
| Feedback mechanism | Product | In-app feedback widget |
| Support runbook | DevOps | Incident response procedures |
| Weekly check-ins | Product | Scheduled calls with pilots |

**Phase 3 Exit Criteria:**
- [ ] Production environment running on Kubernetes
- [ ] Security audit passed with no critical issues
- [ ] Load test passes: 100 invoices/minute sustained
- [ ] 5 pilot customers actively using the platform
- [ ] Support runbook documented

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| **OCR accuracy < 90%** | Med | High | Multi-provider fallback; human-in-loop; training data from corrections |
| **Rust learning curve** | Med | Med | Pair programming; thorough code review; Go fallback for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant; penetration testing; RLS as defense-in-depth |
| **Email token compromise** | Med | High | HMAC signing; 72h expiry; one-time use; IP logging; rate limiting |
| **Connection pool exhaustion** | Med | Med | Per-tenant pools with caps; lazy loading; monitoring alerts |
| **DuckDB scale limits** | Med | Med | Partition by time; archive old data; evaluate ClickHouse if needed |

### 4.2 Product Risks

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| **Feature creep** | High | High | Strict adherence to anti-goals; weekly scope reviews; "Phase 2" frequently |
| **Pilot churn** | Med | High | Weekly check-ins; <24h bug response; dedicated Slack; white-glove support |
| **ERP complexity** | High | Med | Start with QuickBooks (simplest); use official SDKs; defer others |
| **Competitor response** | Med | Med | Move fast; focus on UX; build switching costs via customization |

### 4.3 Operational Risks

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| **Data loss** | Low | Critical | Daily backups; PITR; cross-region replication |
| **Service outage** | Med | High | Multi-AZ; health checks; automatic failover; MTTR <1h target |
| **Key person dependency** | High | High | Documentation; pair programming; knowledge sharing |
| **Security incident** | Low | Critical | Pen testing; security audit; incident response plan |

---

## 5. Resource Requirements

### 5.1 Team Structure

```
BILL FORGE TEAM (5.5 FTE)
├── Engineering (4.5 FTE)
│   ├── Backend Engineer (Rust) × 2
│   │   • bf-api, bf-invoice, bf-workflow, bf-ocr crates
│   │   • Database design, migrations, queries
│   │   • OCR pipeline, approval engine
│   │
│   ├── Frontend Engineer (Next.js) × 1
│   │   • Invoice capture UI, approval inbox
│   │   • Dashboards, analytics views
│   │   • Component library customization
│   │
│   ├── Full-Stack / DevOps Engineer × 1
│   │   • CI/CD, Docker, Kubernetes
│   │   • Monitoring, alerting, runbooks
│   │   • Integration work, gap filling
│   │
│   └── ML/AI Engineer (Contract) × 0.5
│       • OCR optimization, accuracy tuning
│       • Winston AI adaptation (Phase 3)
│
└── Product (1 FTE)
    └── Product Manager × 1
        • Pilot customer relationships
        • Feature prioritization
        • Feedback synthesis
```

### 5.2 Infrastructure Costs (Monthly)

| Component | Dev | Production (5 Pilots) |
|-----------|----:|----------------------:|
| Cloud Compute | $200 | $800 |
| PostgreSQL (RDS) | $50 | $300 |
| Redis (ElastiCache) | $20 | $100 |
| S3/MinIO | $10 | $50 |
| OCR APIs | $0 | $200 |
| Email (SES) | $0 | $50 |
| Monitoring | $0 | $100 |
| Domain + SSL | $10 | $10 |
| **Total** | **$290/mo** | **$1,610/mo** |

### 5.3 Development Tools

| Tool | Cost/User/Mo | Purpose |
|------|-------------:|---------|
| GitHub Team | $4 | Source control, CI/CD |
| Linear | $8 | Issue tracking |
| Figma | $15 | Design |
| Vercel | $20/mo total | Frontend preview deploys |
| Posthog | Free | Product analytics |
| Sentry | Free | Error tracking |

---

## 6. Key Technical Decisions Rationale

### 6.1 Why Rust for Backend?

**Chosen.** The tradeoffs favor Rust for this use case:

| Factor | Assessment |
|--------|------------|
| **Performance** | OCR preprocessing is CPU-intensive; Rust is 10-100x faster than Python |
| **Memory Safety** | Multi-tenant system requires memory safety guarantees |
| **Concurrency** | Tokio async handles high invoice volume efficiently |
| **CEO Preference** | Aligns with stated technical preferences |
| **Talent** | Harder to hire, but team is small and focused |
| **Iteration Speed** | Slower initially, but type system catches bugs early |

**Decision:** Proceed with Rust. For services where speed isn't critical (e.g., background jobs), we could consider Go as a secondary language.

### 6.2 Why Database-per-Tenant?

**Chosen.** Complete data isolation is non-negotiable for mid-market financial data:

| Model | Pros | Cons | Decision |
|-------|------|------|----------|
| **Database-per-tenant** | Complete isolation, easy backup/restore, compliant | Higher connection overhead, complex migrations | **Selected** |
| Row-level security | Single schema, simpler ops | Complex policies, audit concerns, breach risk | Rejected |
| Schema-per-tenant | Middle ground | Still shared DB, migration complexity | Rejected |

### 6.3 Why Local-First OCR?

**Chosen.** Tesseract as primary with cloud fallback:

| Strategy | Rationale |
|----------|-----------|
| **Privacy** | Some industries (legal, healthcare) can't send docs to cloud |
| **Cost** | 80%+ of invoices are clean PDFs Tesseract handles well |
| **Latency** | Local OCR is faster for simple documents |
| **Fallback** | Cloud providers for complex/handwritten when needed |

### 6.4 Why Adapt Locust for Winston?

The existing Locust codebase contains a sophisticated LangGraph-based agent framework. Key reusable components:

| Component | Path | Reuse Strategy |
|-----------|------|----------------|
| Agent base classes | `src/locust/agents/` | Keep, adapt for Bill Forge domain |
| LLM backends | `src/locust/llm/` | Keep (Claude/Ollama switching) |
| Workflow engine | `src/locust/workflows/` | Keep LangGraph patterns |
| Memory/embeddings | `src/locust/memory/` | Keep for document search |

**What to add:**
- Bill Forge domain tools (invoice_search, approval_status, vendor_lookup)
- Tenant-aware context injection
- Integration with Bill Forge APIs

**Timeline:** Phase 3 (post-MVP), 2-3 weeks to adapt.

---

## 7. Monorepo Structure

```
bill-forge/
├── Cargo.toml                    # Workspace root
├── package.json                  # pnpm workspace root
├── pnpm-workspace.yaml
├── docker-compose.yml            # Local development
├── .github/
│   └── workflows/
│       ├── ci.yml
│       ├── deploy-staging.yml
│       └── deploy-prod.yml
│
├── crates/                       # Rust backend
│   ├── bf-common/                # Shared types, errors, utilities
│   ├── bf-tenant/                # Tenant management, DB provisioning
│   ├── bf-auth/                  # JWT, API keys, permissions
│   ├── bf-api/                   # Axum gateway, routes, middleware
│   ├── bf-invoice/               # Invoice capture, queues
│   ├── bf-ocr/                   # OCR provider abstraction
│   ├── bf-workflow/              # Approval rules, state machine
│   ├── bf-vendor/                # Vendor CRUD, matching
│   ├── bf-storage/               # S3/MinIO abstraction
│   └── bf-analytics/             # DuckDB queries, dashboards
│
├── apps/
│   └── web/                      # Next.js frontend
│       ├── src/
│       │   ├── app/              # App Router pages
│       │   │   ├── (auth)/
│       │   │   └── (dashboard)/
│       │   │       ├── invoices/
│       │   │       ├── approvals/
│       │   │       ├── vendors/
│       │   │       └── reports/
│       │   ├── components/
│       │   └── lib/
│       └── package.json
│
├── packages/
│   ├── ui/                       # Extended shadcn/ui components
│   └── api-client/               # Generated TypeScript client
│
├── services/
│   └── winston/                  # AI assistant (Phase 3, from Locust)
│
├── migrations/
│   ├── control-plane/
│   └── tenant/
│
├── infra/
│   ├── terraform/
│   └── kubernetes/
│
└── docs/
    ├── api/                      # OpenAPI specs
    ├── architecture/             # ADRs
    └── runbooks/
```

---

## 8. Success Criteria (3-Month Horizon)

### 8.1 Product Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| OCR Accuracy | ≥90% | Correct fields / Total fields on test set |
| Processing Latency (P95) | <5 sec | Upload to queue placement |
| Approval Cycle Time | <24 hrs | Submission to final approval (avg) |
| Email Approval Success | ≥95% | Successful email actions / Total |
| System Uptime | ≥99.5% | Monthly availability |

### 8.2 Business Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| Pilot Customers | 5 | Actively using platform |
| Invoices Processed | 1,000+ | Total across all pilots |
| Customer NPS | ≥50 | Weekly survey |
| Pilot-to-Paid Intent | ≥60% | "Would you pay for this?" |

### 8.3 Technical Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| API Response Time (P95) | <200ms | Non-OCR endpoints |
| Test Coverage | ≥80% | Line coverage on core crates |
| Critical Bugs | 0 | Unresolved P0 issues |
| Deploy Frequency | Daily | Successful deploys to staging |
| MTTR | <1 hour | Incident detection to resolution |

---

## 9. Answers to CEO Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette Strengths:**
- Established in Nordics/Europe
- Deep SAP/Oracle integrations
- Mature, complex workflow capabilities

**Palette Weaknesses (Our Opportunities):**
- UI consistently described as "slow" and "clunky"
- Limited AI/ML innovation
- Opaque, expensive pricing (enterprise-only feel)
- No local OCR option

**Bill Forge Differentiation:**
1. **Speed:** Sub-second UI vs multi-second loads
2. **Modern UX:** React/Next.js vs legacy tech
3. **Transparent pricing:** Published rates, no "call for quote"
4. **Local-first OCR:** Privacy differentiator for regulated industries
5. **Email approvals:** No login required (unique feature)
6. **Modular:** Buy only what you need

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: Three-tier confidence routing**

| Confidence | Queue | Action |
|------------|-------|--------|
| ≥85% | AP Queue | Auto-route to approval workflow |
| 70-84% | Review Queue | Human verifies flagged fields only |
| <70% | Error Queue | Full manual entry required |

The 85% threshold balances automation rate with error cost. This is industry-aligned; adjustments can be made per-tenant based on their preferences.

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online**

| Priority | ERP | Rationale |
|----------|-----|-----------|
| 1 | QuickBooks Online | Largest mid-market share, simple REST API, OAuth 2.0, excellent docs |
| 2 | NetSuite | Common in growing companies, more complex but well-documented |
| 3 | Sage Intacct | Strong in manufacturing, defer to Phase 2 |

QuickBooks integration can be completed in 2-3 weeks with a single engineer.

### Q4: What approval workflow patterns are most common in mid-market companies?

**Research findings:**

| Pattern | Prevalence | MVP Support |
|---------|------------|-------------|
| Amount-based tiers | 80% | Yes |
| Exception-only review | 60% | Yes |
| Department routing | 40% | Phase 2 |
| Dual approval | 30% | Phase 2 |

**MVP must support:** Amount-based tiers (<$5K auto, $5K-$25K manager, etc.) + exception routing (auto-approve if PO matches).

### Q5: How do competitors handle multi-currency and international invoices?

**Common approaches:**
- Store original currency + converted base currency amount
- Daily exchange rate sync (ECB, Open Exchange Rates)
- Allow manual rate override
- Display both currencies in UI

**MVP Recommendation:**
- Support `currency` field in extraction
- Convert to tenant's base currency for reporting
- Use Open Exchange Rates API (free tier: 1,000/month)
- Defer full multi-currency GL posting to Phase 2

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommendation: Tiered Usage-Based**

| Tier | Monthly Base | Invoices Included | Overage |
|------|-------------|-------------------|---------|
| Starter | $299 | 500 | $0.75/invoice |
| Growth | $799 | 2,000 | $0.50/invoice |
| Scale | $1,999 | 10,000 | $0.30/invoice |

**Why this works:**
- No per-seat pricing (AP teams hate this)
- Predictable base cost (finance can budget)
- Scales with business growth
- Transparent (builds trust)

**Module add-ons:**
- Vendor Management: +$199/mo
- Winston AI: +$299/mo

---

## 10. Next Steps

### Immediate Actions (This Week)

1. **Create bill-forge repository**
   - Initialize Cargo workspace with crate stubs
   - Initialize pnpm workspace
   - Create Docker Compose file

2. **Scaffold core crates**
   - bf-common (shared types, errors)
   - bf-tenant (tenant CRUD, DB provisioning)
   - bf-auth (JWT handling)
   - bf-api (Axum skeleton)

3. **Set up CI/CD**
   - GitHub Actions for Rust (clippy, fmt, test)
   - GitHub Actions for Next.js (lint, build)

4. **Design database schema**
   - Control plane (tenants, users, api_keys)
   - Tenant schema (invoices, vendors, workflows, audit_log)

### Week 1 Deliverables

- [ ] Monorepo initialized (Cargo + pnpm)
- [ ] Docker Compose running PostgreSQL, Redis, MinIO
- [ ] bf-api crate with `/health` endpoint returning 200
- [ ] bf-tenant crate can create tenant database
- [ ] CI pipeline passing on main branch
- [ ] Next.js app displaying placeholder page with shadcn/ui

---

## Appendix A: Database Schema (Core Tables)

```sql
-- CONTROL PLANE DATABASE

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    database_name VARCHAR(100) NOT NULL,
    modules JSONB DEFAULT '["invoice_capture", "invoice_processing"]',
    settings JSONB DEFAULT '{}',
    privacy_mode BOOLEAN DEFAULT FALSE,  -- Local OCR only if true
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id),
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL,  -- admin, approver, viewer
    delegation_to UUID REFERENCES users(id),
    delegation_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tenant_id, email)
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id),
    key_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    permissions JSONB DEFAULT '["read"]',
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- TENANT DATABASE (per tenant)

CREATE TABLE vendors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    normalized_name VARCHAR(255) NOT NULL,
    tax_id VARCHAR(50),
    payment_terms INTEGER DEFAULT 30,
    default_gl_code VARCHAR(50),
    default_cost_center VARCHAR(50),
    status VARCHAR(20) DEFAULT 'active',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vendor_id UUID REFERENCES vendors(id),
    invoice_number VARCHAR(100),
    invoice_date DATE,
    due_date DATE,
    amount DECIMAL(15, 2),
    currency VARCHAR(3) DEFAULT 'USD',
    converted_amount DECIMAL(15, 2),  -- In tenant's base currency
    tax_amount DECIMAL(15, 2),
    status VARCHAR(30) DEFAULT 'pending_ocr',
    queue VARCHAR(20),  -- ap, review, error
    ocr_confidence DECIMAL(5, 2),
    ocr_provider VARCHAR(30),
    document_path VARCHAR(500),
    extracted_data JSONB,
    field_confidences JSONB,  -- Per-field confidence scores
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_invoices_status ON invoices(status);
CREATE INDEX idx_invoices_queue ON invoices(queue);
CREATE INDEX idx_invoices_vendor ON invoices(vendor_id);

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

CREATE TABLE approval_workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    rules JSONB NOT NULL,  -- Rule definitions
    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE approval_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID REFERENCES invoices(id),
    workflow_id UUID REFERENCES approval_workflows(id),
    step_number INTEGER,
    required_approver_id UUID,
    actual_approver_id UUID,
    status VARCHAR(20) DEFAULT 'pending',  -- pending, approved, rejected, skipped
    action_at TIMESTAMPTZ,
    action_method VARCHAR(20),  -- web, email, api
    comments TEXT,
    deadline TIMESTAMPTZ,  -- SLA deadline
    escalated BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_approval_steps_invoice ON approval_steps(invoice_id);
CREATE INDEX idx_approval_steps_approver ON approval_steps(required_approver_id, status);

CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type VARCHAR(50) NOT NULL,
    entity_id UUID NOT NULL,
    action VARCHAR(50) NOT NULL,
    actor_id UUID,
    actor_type VARCHAR(20),  -- user, system, api
    old_values JSONB,
    new_values JSONB,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_actor ON audit_log(actor_id);
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
```

---

## Appendix B: API Endpoints (MVP)

```yaml
# Authentication
POST   /api/v1/auth/login           # Get JWT token
POST   /api/v1/auth/refresh         # Refresh JWT
POST   /api/v1/auth/logout          # Invalidate session

# Tenant Management (internal)
POST   /api/v1/tenants              # Create tenant
GET    /api/v1/tenants/:id          # Get tenant details

# Invoice Capture
POST   /api/v1/:tenant/invoices/upload   # Upload invoice document
GET    /api/v1/:tenant/invoices          # List invoices (paginated, filtered)
GET    /api/v1/:tenant/invoices/:id      # Get invoice details
PATCH  /api/v1/:tenant/invoices/:id      # Update invoice (manual corrections)
POST   /api/v1/:tenant/invoices/:id/reprocess  # Re-run OCR

# Queues
GET    /api/v1/:tenant/queues/ap         # AP queue invoices
GET    /api/v1/:tenant/queues/review     # Review queue invoices
GET    /api/v1/:tenant/queues/error      # Error queue invoices
POST   /api/v1/:tenant/queues/error/:id/resolve  # Mark error as resolved

# Approvals
GET    /api/v1/:tenant/approvals/pending        # My pending approvals
POST   /api/v1/:tenant/invoices/:id/approve     # Approve invoice
POST   /api/v1/:tenant/invoices/:id/reject      # Reject invoice
POST   /api/v1/:tenant/invoices/:id/hold        # Put on hold
POST   /api/v1/:tenant/invoices/batch/approve   # Bulk approve

# Email Actions (no auth required, signed tokens)
GET    /api/v1/actions/:token/approve    # Approve via email link
GET    /api/v1/actions/:token/reject     # Reject via email link

# Vendors
GET    /api/v1/:tenant/vendors           # List vendors
POST   /api/v1/:tenant/vendors           # Create vendor
GET    /api/v1/:tenant/vendors/:id       # Get vendor
PATCH  /api/v1/:tenant/vendors/:id       # Update vendor
GET    /api/v1/:tenant/vendors/match     # Fuzzy match vendor name

# Workflows
GET    /api/v1/:tenant/workflows         # List workflow rules
POST   /api/v1/:tenant/workflows         # Create workflow
PATCH  /api/v1/:tenant/workflows/:id     # Update workflow

# Users (tenant-scoped)
GET    /api/v1/:tenant/users             # List users
POST   /api/v1/:tenant/users             # Create user
PATCH  /api/v1/:tenant/users/:id         # Update user (incl. delegation)

# Analytics
GET    /api/v1/:tenant/analytics/dashboard    # Dashboard metrics
GET    /api/v1/:tenant/analytics/processing   # OCR metrics
GET    /api/v1/:tenant/analytics/approvals    # Approval metrics
GET    /api/v1/:tenant/analytics/spend        # Spend by vendor/cost center
```

---

*This strategic plan is a living document. Updates will be made based on pilot customer feedback and technical learnings.*

**Document History:**
| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-31 | CTO | Initial draft |
| 2.0 | 2026-02-01 | CTO | Consolidated plan with implementation details |
