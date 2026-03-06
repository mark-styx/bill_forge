# Bill Forge Technical Plan

**Date:** January 31, 2026
**Version:** 1.0
**Author:** CTO (AI-Assisted)
**Status:** Draft for Review

---

## Executive Summary

### Current State Assessment

The existing codebase at `/Users/mark/sentinel/locust` is **Locust**, a multi-agent AI development framework—not invoice processing software. This represents both a gap and an opportunity:

| Aspect | CEO Vision | Current Reality |
|--------|-----------|-----------------|
| Language | Rust (Axum) | Python (LangChain, LangGraph) |
| Frontend | Next.js 14+ | None (CLI only) |
| Database | PostgreSQL + DuckDB | SQLite + DuckDB |
| Purpose | Invoice Processing | AI Agent Orchestration |

**Strategic Options:**

1. **Build From Scratch** - New Rust/Next.js codebase as envisioned
2. **Leverage Locust** - Use the AI agent framework to accelerate Bill Forge development
3. **Hybrid Approach** - Use Locust for AI features (Winston), build core platform separately

**Recommendation:** Option 3 (Hybrid) - Build Bill Forge core in the preferred Rust/Next.js stack while repurposing Locust's agent architecture for Winston AI Assistant.

---

## 1. Technical Architecture Recommendations

### 1.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           BILL FORGE PLATFORM                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                         FRONTEND LAYER                          │   │
│  │                         (Next.js 14+)                           │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │   │
│  │  │ Invoice  │ │ Approval │ │  Vendor  │ │ Reports  │           │   │
│  │  │ Capture  │ │ Workflow │ │  Portal  │ │Dashboard │           │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │   │
│  │                    ┌──────────────┐                             │   │
│  │                    │ Winston Chat │                             │   │
│  │                    │   (AI UI)    │                             │   │
│  │                    └──────────────┘                             │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                          API GATEWAY                             │   │
│  │                         (Rust/Axum)                              │   │
│  │  • Authentication/Authorization (JWT + API Keys)                 │   │
│  │  • Rate Limiting (per tenant)                                    │   │
│  │  • Request Routing                                               │   │
│  │  • Tenant Resolution                                             │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│         ┌──────────────────────────┼──────────────────────────┐        │
│         ▼                          ▼                          ▼        │
│  ┌─────────────┐         ┌─────────────────┐         ┌─────────────┐   │
│  │   INVOICE   │         │    WORKFLOW     │         │   VENDOR    │   │
│  │   SERVICE   │         │    SERVICE      │         │   SERVICE   │   │
│  │  (Rust)     │         │    (Rust)       │         │   (Rust)    │   │
│  ├─────────────┤         ├─────────────────┤         ├─────────────┤   │
│  │• OCR Queue  │         │• Approval Rules │         │• Master Data│   │
│  │• Extraction │         │• Email Actions  │         │• Tax Docs   │   │
│  │• Validation │         │• SLA Tracking   │         │• Matching   │   │
│  │• Confidence │         │• Delegation     │         │• Onboarding │   │
│  └─────────────┘         └─────────────────┘         └─────────────┘   │
│         │                          │                          │        │
│         └──────────────────────────┼──────────────────────────┘        │
│                                    │                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      ANALYTICS SERVICE                           │   │
│  │                         (DuckDB)                                 │   │
│  │  • Real-time dashboards   • Spend analysis                      │   │
│  │  • Processing metrics     • Export engine                       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                       WINSTON AI SERVICE                         │   │
│  │                    (Python/LangGraph - from Locust)              │   │
│  │  • Natural language queries   • Anomaly detection               │   │
│  │  • Platform actions           • Vendor disambiguation           │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                           DATA LAYER                                    │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐     │
│  │    PostgreSQL    │  │     DuckDB       │  │   S3-Compatible  │     │
│  │   (Per-Tenant)   │  │   (Analytics)    │  │    (Documents)   │     │
│  │                  │  │                  │  │                  │     │
│  │ • Invoices       │  │ • Metrics        │  │ • Invoice PDFs   │     │
│  │ • Vendors        │  │ • Dashboards     │  │ • Tax Documents  │     │
│  │ • Workflows      │  │ • Reports        │  │ • Attachments    │     │
│  │ • Users          │  │ • Audit Logs     │  │                  │     │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘     │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Tenant Isolation Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    CONTROL PLANE                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Tenant Management Service               │    │
│  │  • Tenant provisioning    • Database routing        │    │
│  │  • Module subscriptions   • Billing integration     │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│   Tenant A    │    │   Tenant B    │    │   Tenant C    │
│ ┌───────────┐ │    │ ┌───────────┐ │    │ ┌───────────┐ │
│ │ PostgreSQL│ │    │ │ PostgreSQL│ │    │ │ PostgreSQL│ │
│ │  (tenant_a)│ │    │ │  (tenant_b)│ │    │ │  (tenant_c)│ │
│ └───────────┘ │    │ └───────────┘ │    │ └───────────┘ │
│ ┌───────────┐ │    │ ┌───────────┐ │    │ ┌───────────┐ │
│ │   DuckDB  │ │    │ │   DuckDB  │ │    │ │   DuckDB  │ │
│ │ (analytics)│ │    │ │ (analytics)│ │    │ │ (analytics)│ │
│ └───────────┘ │    │ └───────────┘ │    │ └───────────┘ │
│ ┌───────────┐ │    │ ┌───────────┐ │    │ ┌───────────┐ │
│ │  S3 Path  │ │    │ │  S3 Path  │ │    │ │  S3 Path  │ │
│ │/tenant_a/ │ │    │ │/tenant_b/ │ │    │ │/tenant_c/ │ │
│ └───────────┘ │    │ └───────────┘ │    │ └───────────┘ │
└───────────────┘    └───────────────┘    └───────────────┘
```

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      OCR PIPELINE                            │
│                                                              │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────┐  │
│  │  Ingest  │───►│  Preprocess  │───►│  Provider Router  │  │
│  │          │    │              │    │                   │  │
│  │• Upload  │    │• Deskew      │    │   ┌───────────┐   │  │
│  │• Email   │    │• Enhance     │    │   │ Tesseract │   │  │
│  │• API     │    │• Detect type │    │   │  (Local)  │   │  │
│  └──────────┘    └──────────────┘    │   └───────────┘   │  │
│                                      │   ┌───────────┐   │  │
│                                      │   │  Textract │   │  │
│                                      │   │   (AWS)   │   │  │
│                                      │   └───────────┘   │  │
│                                      │   ┌───────────┐   │  │
│                                      │   │  Vision   │   │  │
│                                      │   │ (Google)  │   │  │
│                                      │   └───────────┘   │  │
│                                      └─────────┬─────────┘  │
│                                                │             │
│                                                ▼             │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │    Route     │◄───│   Validate   │◄───│   Extract     │  │
│  │              │    │              │    │               │  │
│  │ Confidence   │    │• Field check │    │• Header data  │  │
│  │ >= 85%: AP   │    │• Format      │    │• Line items   │  │
│  │ < 85%: Error │    │• Duplicates  │    │• Totals       │  │
│  └──────────────┘    └──────────────┘    └───────────────┘  │
│         │                                                    │
│         ▼                                                    │
│  ┌────────────┐         ┌────────────┐                      │
│  │  AP Queue  │         │Error Queue │                      │
│  │ (Auto-flow)│         │ (Manual)   │                      │
│  └────────────┘         └────────────┘                      │
└─────────────────────────────────────────────────────────────┘
```

### 1.4 Approval Workflow Engine

```
┌─────────────────────────────────────────────────────────────┐
│                  APPROVAL WORKFLOW ENGINE                    │
│                                                              │
│  ┌────────────────────────────────────────────────────┐     │
│  │                  RULE ENGINE                        │     │
│  │  ┌────────────────────────────────────────────┐    │     │
│  │  │  Condition Evaluator (Rust Expression)      │    │     │
│  │  │                                             │    │     │
│  │  │  amount < 5000 → auto_approve               │    │     │
│  │  │  amount >= 5000 AND amount < 25000 → L1     │    │     │
│  │  │  amount >= 25000 AND amount < 50000 → L2    │    │     │
│  │  │  amount >= 50000 → L3 (CFO)                 │    │     │
│  │  │  vendor.is_new → add_review_step            │    │     │
│  │  │  invoice.has_po_mismatch → exception_queue  │    │     │
│  │  └────────────────────────────────────────────┘    │     │
│  └────────────────────────────────────────────────────┘     │
│                            │                                 │
│                            ▼                                 │
│  ┌────────────────────────────────────────────────────┐     │
│  │                  STATE MACHINE                      │     │
│  │                                                     │     │
│  │    ┌─────────┐     ┌─────────┐     ┌─────────┐    │     │
│  │    │ Pending │────►│ L1 Appr │────►│ L2 Appr │    │     │
│  │    └────┬────┘     └────┬────┘     └────┬────┘    │     │
│  │         │               │               │          │     │
│  │         ▼               ▼               ▼          │     │
│  │    ┌─────────┐     ┌─────────┐     ┌─────────┐    │     │
│  │    │Approved │     │Rejected │     │On Hold  │    │     │
│  │    └─────────┘     └─────────┘     └─────────┘    │     │
│  └────────────────────────────────────────────────────┘     │
│                            │                                 │
│                            ▼                                 │
│  ┌────────────────────────────────────────────────────┐     │
│  │              NOTIFICATION SERVICE                   │     │
│  │  • Email (approve/reject links with signed tokens) │     │
│  │  • In-app notifications                            │     │
│  │  • SLA escalation alerts                           │     │
│  │  • Delegation auto-routing                         │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Backend Services (Rust)

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Web Framework** | Axum 0.7+ | CEO preference, async-first, tower middleware ecosystem |
| **Async Runtime** | Tokio | Industry standard, required by Axum |
| **Serialization** | Serde + serde_json | De facto Rust standard |
| **Database ORM** | SQLx | Compile-time checked queries, async-native |
| **Migrations** | sqlx-cli | Integrated with SQLx |
| **Validation** | validator | Derive macros for request validation |
| **Error Handling** | thiserror + anyhow | Structured errors for APIs, flexible internal errors |
| **Logging** | tracing + tracing-subscriber | Structured logging, spans for distributed tracing |
| **Configuration** | config-rs | Multi-source config (env, files, defaults) |
| **Testing** | tokio-test + wiremock | Async test support, HTTP mocking |

### 2.2 Frontend (Next.js)

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Framework** | Next.js 14+ (App Router) | CEO preference, RSC for performance |
| **Language** | TypeScript (strict mode) | Type safety, IDE support |
| **Styling** | Tailwind CSS + shadcn/ui | CEO preference, consistent design system |
| **State Management** | TanStack Query | Server state caching, optimistic updates |
| **Forms** | React Hook Form + Zod | Type-safe validation |
| **Tables** | TanStack Table | Complex data grids for invoice lists |
| **Charts** | Recharts | Analytics dashboards |
| **Auth** | NextAuth.js | Session management, OAuth support |

### 2.3 Data Layer

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **OLTP Database** | PostgreSQL 16+ | CEO preference, per-tenant isolation, JSONB for flexible fields |
| **Analytics DB** | DuckDB | CEO preference, embedded analytics, fast aggregations |
| **Document Storage** | MinIO (S3-compatible) | CEO preference for S3 abstraction, local-first dev |
| **Caching** | Redis | Session storage, rate limiting, queue backing |
| **Search** | PostgreSQL Full-Text + pg_trgm | Start simple, add Elasticsearch if needed |

### 2.4 OCR Providers

| Provider | Use Case | Notes |
|----------|----------|-------|
| **Tesseract 5** | Local/Privacy-first | Default for sensitive tenants |
| **AWS Textract** | High-volume production | Best accuracy for structured forms |
| **Google Vision** | Fallback/Comparison | Good for handwritten notes |

### 2.5 Winston AI (from Locust)

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Agent Framework** | LangGraph (from Locust) | Already built, tested |
| **LLM Backend** | Claude API + Ollama fallback | Local option for privacy |
| **Embeddings** | text-embedding-3-small | Cost-effective semantic search |
| **Vector Store** | DuckDB + pgvector | Embedded analytics DB already in use |

### 2.6 Infrastructure

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Container Runtime** | Docker | Standard, already in use |
| **Orchestration** | Docker Compose (dev), Kubernetes (prod) | Progressive complexity |
| **CI/CD** | GitHub Actions | Standard, good ecosystem |
| **Secrets** | HashiCorp Vault / AWS Secrets Manager | Secure credential management |
| **Monitoring** | Prometheus + Grafana | Industry standard |
| **APM** | OpenTelemetry | Vendor-neutral tracing |

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)
**Goal:** Establish project structure and development environment

```
Week 1:
├── Set up monorepo structure (Cargo workspace + pnpm)
├── Configure CI/CD pipeline (lint, test, build)
├── Create Docker Compose development environment
├── Implement tenant management control plane
└── Database schema design (PostgreSQL + migrations)

Week 2:
├── Implement authentication service (JWT + API keys)
├── Set up Next.js frontend scaffold
├── Configure shadcn/ui component library
├── Implement API gateway with tenant resolution
└── Create development fixtures and seed data
```

**Deliverables:**
- [ ] Monorepo with Rust workspace + Next.js
- [ ] Docker Compose with PostgreSQL, Redis, MinIO
- [ ] Basic auth flow (login, API keys)
- [ ] CI pipeline (test, lint, build)
- [ ] Database migrations infrastructure

### Phase 1: Invoice Capture MVP (Weeks 3-6)
**Goal:** Working OCR pipeline with manual review capability

```
Week 3-4:
├── Implement document upload API (PDF, images)
├── Integrate Tesseract for local OCR
├── Build extraction pipeline (header fields)
├── Create confidence scoring system
├── Implement AP queue and error queue

Week 5-6:
├── Build invoice capture UI (upload, preview)
├── Implement manual correction interface
├── Add vendor matching logic
├── Line item extraction
├── Create extraction accuracy dashboard
```

**Deliverables:**
- [ ] Upload API with S3 storage
- [ ] Tesseract OCR integration
- [ ] Field extraction (vendor, invoice #, amount, date)
- [ ] Confidence-based routing (85% threshold)
- [ ] Manual review/correction UI
- [ ] Basic vendor matching

**Success Metrics:**
- 85%+ field extraction accuracy on clean PDFs
- < 3 second processing time per invoice
- Manual correction reduces errors by 95%

### Phase 2: Invoice Processing MVP (Weeks 7-10)
**Goal:** Approval workflows with email actions

```
Week 7-8:
├── Design workflow rule engine
├── Implement approval state machine
├── Build rule configuration UI
├── Create approval inbox view
├── Implement basic approval actions (approve/reject/hold)

Week 9-10:
├── Add email approval capability (signed links)
├── Implement delegation support
├── Build SLA tracking and escalation
├── Create bulk operations (batch submit)
├── Add audit trail logging
```

**Deliverables:**
- [ ] Workflow rule engine (amount-based routing)
- [ ] Multi-level approval chains
- [ ] Email approve/reject (no login required)
- [ ] Delegation configuration
- [ ] SLA monitoring dashboard
- [ ] Complete audit trail

**Success Metrics:**
- < 5 second approval action latency
- Email approvals work without authentication
- 100% audit coverage

### Phase 3: Pilot Launch (Weeks 11-12)
**Goal:** Deploy to 5 pilot customers

```
Week 11:
├── Production environment setup
├── Security audit and penetration testing
├── Performance load testing (100 invoices/minute)
├── Monitoring and alerting configuration
├── Documentation (API docs, user guides)

Week 12:
├── Pilot customer onboarding
├── Data migration tooling
├── Support escalation procedures
├── Feedback collection mechanisms
├── Bug triage and hotfix process
```

**Deliverables:**
- [ ] Production deployment on cloud infrastructure
- [ ] Security audit completion
- [ ] Load test passing 100 invoices/minute
- [ ] 5 pilot customers onboarded
- [ ] Support runbook

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy below 90%** | Medium | High | Multi-provider fallback, human-in-loop for low confidence, training data collection |
| **Rust learning curve** | Medium | Medium | Pair programming, code reviews, consider Go as fallback for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant, penetration testing, row-level security as defense-in-depth |
| **Email approval security** | Medium | High | Signed tokens with expiration, rate limiting, IP logging |
| **DuckDB scalability** | Medium | Medium | Partition by time, archive old data, evaluate ClickHouse if needed |
| **Winston AI latency** | Medium | Medium | Async processing, streaming responses, local Ollama for simple queries |

### 4.2 Product Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep before PMF** | High | High | Strict adherence to anti-goals, weekly scope reviews |
| **Pilot customer churn** | Medium | High | Weekly check-ins, fast bug resolution, onboarding support |
| **ERP integration complexity** | High | Medium | Start with QuickBooks (simplest), use existing libraries |
| **Competitor response** | Medium | Medium | Focus on mid-market, avoid enterprise complexity |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups, point-in-time recovery, cross-region replication |
| **Service outage** | Medium | High | Multi-AZ deployment, health checks, automatic failover |
| **Key person dependency** | High | High | Documentation, pair programming, knowledge sharing sessions |

---

## 5. Resource Requirements

### 5.1 Team Structure

```
┌─────────────────────────────────────────────────────────────┐
│                      BILL FORGE TEAM                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                   CORE ENGINEERING                    │   │
│  │                                                       │   │
│  │  • Backend Engineer (Rust) - 2 FTE                   │   │
│  │    - API development, OCR pipeline, workflow engine  │   │
│  │                                                       │   │
│  │  • Frontend Engineer (Next.js) - 1 FTE               │   │
│  │    - UI components, dashboards, user experience      │   │
│  │                                                       │   │
│  │  • Full-Stack Engineer - 1 FTE                       │   │
│  │    - Integration work, DevOps, gap filling           │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                   SPECIALIST ROLES                    │   │
│  │                                                       │   │
│  │  • ML/AI Engineer (Part-time/Contract) - 0.5 FTE    │   │
│  │    - OCR optimization, Winston AI, anomaly detection │   │
│  │                                                       │   │
│  │  • Product Manager - 1 FTE                           │   │
│  │    - Pilot customer relationships, prioritization    │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  TOTAL: 5.5 FTE for MVP                                     │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Infrastructure Costs (Monthly)

| Component | Development | Production (5 pilots) |
|-----------|-------------|----------------------|
| **Cloud Compute** | $200 | $800 |
| **PostgreSQL (managed)** | $50 | $300 |
| **Redis** | $20 | $100 |
| **S3 Storage (100GB)** | $10 | $50 |
| **OCR API (AWS Textract)** | $0 (Tesseract) | $200 |
| **LLM API (Claude)** | $100 | $300 |
| **Monitoring** | $0 (self-hosted) | $100 |
| **Total** | **$380/month** | **$1,850/month** |

### 5.3 Development Tools

| Tool | Cost | Purpose |
|------|------|---------|
| GitHub Team | $4/user/month | Source control, CI/CD |
| Linear | $8/user/month | Issue tracking |
| Figma | $15/user/month | Design |
| Posthog | Free tier | Analytics |
| Sentry | Free tier | Error tracking |

---

## 6. Key Technical Decisions

### 6.1 Why Rust for Backend?

**Pros:**
- Performance: 10-100x faster than Python for CPU-bound OCR processing
- Memory safety: Critical for multi-tenant data isolation
- Concurrency: Tokio async runtime handles high invoice volume
- CEO preference alignment

**Cons:**
- Steeper learning curve
- Slower iteration speed initially
- Smaller talent pool

**Decision:** Proceed with Rust. Performance and safety benefits outweigh velocity concerns for a financial data platform.

### 6.2 Why Database-Per-Tenant?

**Pros:**
- Complete data isolation (regulatory compliance)
- Per-tenant backup/restore
- Easy data portability
- No row-level security complexity

**Cons:**
- Higher connection overhead
- More complex migrations
- Connection pooling per tenant

**Decision:** Use database-per-tenant. Data isolation is non-negotiable for mid-market financial data.

### 6.3 OCR Provider Strategy

**Strategy:** Local-first with cloud fallback

1. **Default:** Tesseract 5 (local) for all invoices
2. **Escalation:** If confidence < 75%, retry with AWS Textract
3. **Privacy Mode:** Tenant can disable cloud OCR entirely

**Rationale:** Balances cost, privacy, and accuracy. Most invoices are standard PDFs that Tesseract handles well.

### 6.4 Leveraging Locust for Winston

**Strategy:** Fork Locust's agent architecture for Winston

**What to Keep:**
- LangGraph workflow engine
- Agent abstraction layer
- LLM backend switching (Claude/Ollama)
- Checkpoint/resume capability

**What to Modify:**
- Remove software development agents
- Add Bill Forge domain tools (invoice lookup, approval actions)
- Integrate with Bill Forge APIs
- Add tenant-aware context

**Timeline:** Phase 3 (post-MVP), approximately 2-3 weeks of adaptation.

---

## 7. API Design Principles

### 7.1 REST API Standards

```rust
// URL Pattern: /api/v1/{tenant}/resource/{id}
// Example: /api/v1/acme-corp/invoices/inv_123abc

// Response Format
{
  "data": { ... },           // Single object or array
  "meta": {
    "page": 1,
    "per_page": 50,
    "total": 1234
  },
  "errors": []               // Empty on success
}

// Error Format
{
  "data": null,
  "meta": {},
  "errors": [
    {
      "code": "INVOICE_NOT_FOUND",
      "message": "Invoice inv_123abc not found",
      "field": null
    }
  ]
}
```

### 7.2 Key Endpoints (MVP)

```
# Invoice Capture
POST   /api/v1/{tenant}/invoices/upload
GET    /api/v1/{tenant}/invoices
GET    /api/v1/{tenant}/invoices/{id}
PATCH  /api/v1/{tenant}/invoices/{id}
POST   /api/v1/{tenant}/invoices/{id}/reprocess

# Queues
GET    /api/v1/{tenant}/queues/ap
GET    /api/v1/{tenant}/queues/errors
POST   /api/v1/{tenant}/queues/errors/{id}/resolve

# Approvals
GET    /api/v1/{tenant}/approvals/pending
POST   /api/v1/{tenant}/invoices/{id}/approve
POST   /api/v1/{tenant}/invoices/{id}/reject
POST   /api/v1/{tenant}/invoices/{id}/hold

# Email Actions (signed tokens, no auth)
GET    /api/v1/actions/{signed_token}/approve
GET    /api/v1/actions/{signed_token}/reject

# Vendors
GET    /api/v1/{tenant}/vendors
POST   /api/v1/{tenant}/vendors
GET    /api/v1/{tenant}/vendors/{id}
```

---

## 8. Database Schema (Core Tables)

```sql
-- Tenant Management (Control Plane)
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    slug VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    database_name VARCHAR(100) NOT NULL,
    modules JSONB DEFAULT '["invoice_capture", "invoice_processing"]',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Per-Tenant Schema (in tenant database)

CREATE TABLE vendors (
    id UUID PRIMARY KEY,
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

CREATE TABLE invoices (
    id UUID PRIMARY KEY,
    vendor_id UUID REFERENCES vendors(id),
    invoice_number VARCHAR(100),
    invoice_date DATE,
    due_date DATE,
    amount DECIMAL(15, 2),
    currency VARCHAR(3) DEFAULT 'USD',
    status VARCHAR(20) DEFAULT 'pending',
    ocr_confidence DECIMAL(5, 2),
    document_path VARCHAR(500),
    extracted_data JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY,
    invoice_id UUID REFERENCES invoices(id),
    description TEXT,
    quantity DECIMAL(15, 4),
    unit_price DECIMAL(15, 4),
    amount DECIMAL(15, 2),
    gl_code VARCHAR(50),
    cost_center VARCHAR(50),
    sort_order INTEGER
);

CREATE TABLE approval_workflows (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    rules JSONB NOT NULL,
    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE approval_steps (
    id UUID PRIMARY KEY,
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

CREATE TABLE audit_log (
    id UUID PRIMARY KEY,
    entity_type VARCHAR(50),
    entity_id UUID,
    action VARCHAR(50),
    actor_id UUID,
    actor_type VARCHAR(20), -- 'user', 'system', 'api'
    old_values JSONB,
    new_values JSONB,
    ip_address INET,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## 9. Success Criteria for 3-Month Horizon

### 9.1 Product Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| OCR Accuracy (standard invoices) | ≥ 90% | (Correct fields / Total fields) per invoice batch |
| Processing Latency | < 5 seconds | P95 time from upload to queue placement |
| Approval Cycle Time | < 24 hours | Average time from submission to final approval |
| Email Approval Success Rate | ≥ 95% | Successful email actions / Total email actions |
| System Uptime | ≥ 99.5% | Monthly availability |

### 9.2 Business Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Pilot Customers | 5 | Actively using the platform |
| Invoices Processed | 1,000+ | Total across all pilots |
| Customer Satisfaction | ≥ 4/5 | Weekly NPS survey |
| Critical Bugs | 0 | Unresolved P0 issues |

### 9.3 Technical Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Test Coverage | ≥ 80% | Line coverage for core services |
| API Response Time | < 200ms | P95 for non-OCR endpoints |
| Deployment Frequency | Daily | Successful deploys to staging |
| Mean Time to Recovery | < 1 hour | From incident detection to resolution |

---

## 10. Answers to CEO Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses?

**Strengths:**
- Established market presence in Nordics/Europe
- Strong ERP integrations (SAP, Oracle)
- Mature workflow engine

**Weaknesses:**
- Slow, legacy UI (common complaint in reviews)
- Expensive for mid-market
- Limited AI/automation innovation

**Differentiation Strategy:**
- Speed and simplicity (modern UI, fast OCR)
- Flexible pricing (usage-based, modular)
- AI-first approach (Winston assistant)
- Local-first OCR option for privacy

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation: 85% confidence threshold**

- **≥ 85%:** Route to AP Queue (auto-flow)
- **70-84%:** Route to Review Queue (human verification)
- **< 70%:** Route to Error Queue (manual entry required)

Rationale: 85% balances automation rate with error cost. Lower thresholds increase manual work; higher thresholds miss good invoices.

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online (Priority 1)**

1. **QuickBooks Online** - Largest mid-market share, REST API, OAuth
2. **NetSuite** - Second priority, common in growing companies
3. **Sage** - Third priority, strong in manufacturing/distribution

QuickBooks has the simplest API and largest addressable market for 10-1000 employee companies.

### Q4: What approval workflow patterns are most common in mid-market?

**Top 3 Patterns:**

1. **Amount-Based Tiering (80% of companies)**
   - < $5K: Auto-approve or manager
   - $5K-$25K: Department head
   - $25K-$50K: Finance director
   - > $50K: CFO/Controller

2. **Exception-Only Review (60%)**
   - Auto-approve if PO matches
   - Route for review only on mismatch

3. **Department Routing (40%)**
   - Route to cost center owner
   - Finance approval on all > threshold

### Q5: How do competitors handle multi-currency and international invoices?

**Common Approaches:**
- Store original currency + converted USD amount
- Daily exchange rate sync (ECB, Open Exchange Rates)
- Allow manual rate override
- Display both currencies in UI

**Recommendation for MVP:**
- Support currency field in extraction
- Convert to tenant's base currency for totals
- Use Open Exchange Rates API (free tier: 1000/month)
- Defer full multi-currency GL until Phase 2

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommendation: Tiered Usage-Based Pricing**

| Tier | Invoices/Month | Price | Per-Invoice Overage |
|------|---------------|-------|---------------------|
| Starter | 0-500 | $299/month | $0.75 |
| Growth | 0-2,000 | $799/month | $0.50 |
| Scale | 0-10,000 | $1,999/month | $0.30 |

**Why This Works:**
- Predictable base cost (finance teams like this)
- Scales with business growth
- No per-seat licensing (AP teams hate this)
- Module add-ons: Vendor Management (+$199), Winston AI (+$299)

---

## 11. Next Steps

### Immediate Actions (This Week)

1. **Validate Architecture**
   - Review this plan with stakeholders
   - Identify any blocking concerns
   - Confirm Rust/Next.js decision

2. **Set Up Development Environment**
   - Create monorepo structure
   - Configure Docker Compose
   - Set up CI/CD pipeline

3. **Recruit/Assign Team**
   - Identify 2 Rust engineers
   - Assign frontend engineer
   - Engage ML/AI contractor

4. **Pilot Customer Outreach**
   - Identify 10 potential pilot candidates
   - Schedule discovery calls
   - Gather invoice samples for OCR testing

### Week 1 Deliverables

- [ ] Monorepo initialized with Cargo workspace + pnpm
- [ ] PostgreSQL + Redis + MinIO running in Docker Compose
- [ ] Basic Axum service with health check
- [ ] Next.js app with shadcn/ui configured
- [ ] CI pipeline running tests on PR

---

## Appendix A: Monorepo Structure

```
bill-forge/
├── Cargo.toml                 # Workspace root
├── package.json               # pnpm workspace root
├── pnpm-workspace.yaml
├── docker-compose.yml
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── deploy.yml
│
├── crates/                    # Rust crates
│   ├── bf-api/               # Main API gateway
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── bf-invoice/           # Invoice capture service
│   ├── bf-workflow/          # Approval workflow engine
│   ├── bf-vendor/            # Vendor management
│   ├── bf-ocr/               # OCR provider abstraction
│   ├── bf-storage/           # S3/MinIO abstraction
│   ├── bf-auth/              # Authentication/authorization
│   ├── bf-tenant/            # Tenant management
│   └── bf-common/            # Shared types, utilities
│
├── apps/                      # Frontend applications
│   └── web/                  # Next.js main app
│       ├── package.json
│       ├── next.config.js
│       └── src/
│           ├── app/          # App router pages
│           ├── components/   # UI components
│           └── lib/          # Utilities
│
├── packages/                  # Shared JS packages
│   ├── ui/                   # shadcn/ui components
│   └── api-client/           # Generated TypeScript client
│
├── services/                  # Additional services
│   └── winston/              # AI assistant (Python/LangGraph)
│       ├── pyproject.toml
│       └── src/
│
├── migrations/                # Database migrations
│   ├── control-plane/        # Tenant management DB
│   └── tenant/               # Per-tenant schema
│
└── docs/                      # Documentation
    ├── api/                  # OpenAPI specs
    ├── architecture/         # Architecture decisions
    └── runbooks/             # Operational guides
```

---

## Appendix B: Local Development Setup

```bash
# Prerequisites
- Rust 1.75+
- Node.js 20+
- pnpm 8+
- Docker & Docker Compose

# Clone and setup
git clone https://github.com/billforge/bill-forge.git
cd bill-forge

# Start infrastructure
docker-compose up -d postgres redis minio

# Install dependencies
pnpm install
cargo build

# Run migrations
cargo run -p bf-tenant -- migrate

# Start services
cargo run -p bf-api &
pnpm --filter web dev

# Access
# API: http://localhost:8080
# Web: http://localhost:3000
# MinIO: http://localhost:9001
```

---

*This technical plan is a living document and will be updated as we learn from pilot customers and market feedback.*
