# Bill Forge: CTO Strategic Technical Plan

**Date:** January 31, 2026
**Version:** 2.0
**Status:** Ready for CEO Review
**Horizon:** 3 Months (Q1 2026)

---

## Executive Summary

Bill Forge will be a modern, modular invoice processing platform targeting mid-market companies frustrated with slow, expensive legacy AP tools. Based on the CEO's vision and codebase analysis, I recommend a **hybrid approach**: build the core platform in Rust/Next.js as specified, while leveraging the existing Locust agent framework for the Winston AI Assistant.

**Current State:** The existing `/Users/mark/sentinel/locust` codebase is Locust—a multi-agent AI development framework in Python—not invoice processing software. This is an asset, not a blocker: Locust's agent orchestration architecture is directly applicable to Winston.

**Key Strategic Decisions:**
1. **Build core platform from scratch** in Rust (Axum) + Next.js 14
2. **Leverage Locust** for Winston AI Assistant (Phase 3+)
3. **Database-per-tenant architecture** for complete data isolation
4. **Local-first OCR** with cloud fallback for privacy-conscious customers
5. **QuickBooks Online** as first ERP integration

---

## 1. Technical Architecture Recommendations

### 1.1 System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        BILL FORGE PLATFORM                          │
│                                                                     │
│   PRESENTATION LAYER                                                │
│   ┌─────────────────────────────────────────────────────────────┐  │
│   │                    Next.js 14+ (App Router)                  │  │
│   │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐       │  │
│   │  │ Invoice  │ │ Approval │ │  Vendor  │ │ Reports  │       │  │
│   │  │ Capture  │ │ Workflow │ │  Portal  │ │Dashboard │       │  │
│   │  └──────────┘ └──────────┘ └──────────┘ └──────────┘       │  │
│   │                    ┌──────────────┐                         │  │
│   │                    │ Winston Chat │ (Phase 3)               │  │
│   │                    └──────────────┘                         │  │
│   └─────────────────────────────────────────────────────────────┘  │
│                                │                                    │
│   API LAYER                    ▼                                    │
│   ┌─────────────────────────────────────────────────────────────┐  │
│   │                    API Gateway (Rust/Axum)                   │  │
│   │  • JWT + API Key Authentication                              │  │
│   │  • Per-tenant rate limiting                                  │  │
│   │  • Request routing & tenant resolution                       │  │
│   │  • OpenAPI documentation                                     │  │
│   └─────────────────────────────────────────────────────────────┘  │
│                                │                                    │
│   SERVICE LAYER               ▼                                    │
│   ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ │
│   │   Invoice   │ │  Workflow   │ │   Vendor    │ │  Analytics  │ │
│   │   Service   │ │   Service   │ │   Service   │ │   Service   │ │
│   │   (Rust)    │ │   (Rust)    │ │   (Rust)    │ │  (DuckDB)   │ │
│   └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ │
│                                │                                    │
│   DATA LAYER                   ▼                                    │
│   ┌───────────────┐ ┌───────────────┐ ┌───────────────┐           │
│   │  PostgreSQL   │ │    DuckDB     │ │ S3-Compatible │           │
│   │ (Per-Tenant)  │ │  (Analytics)  │ │   (MinIO)     │           │
│   └───────────────┘ └───────────────┘ └───────────────┘           │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 Multi-Tenant Isolation Model

**Strategy: Database-per-tenant with shared compute**

```
┌─────────────────────────────────────────────────────────────────────┐
│                      CONTROL PLANE DATABASE                         │
│  (Single PostgreSQL instance for tenant metadata)                   │
│                                                                     │
│  tenants: id, slug, database_name, modules[], settings, billing    │
│  users: id, tenant_id, email, role, permissions                    │
│  api_keys: id, tenant_id, key_hash, scopes[], rate_limit           │
└─────────────────────────────────────────────────────────────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          ▼                     ▼                     ▼
    ┌───────────┐         ┌───────────┐         ┌───────────┐
    │ tenant_001│         │ tenant_002│         │ tenant_003│
    │ PostgreSQL│         │ PostgreSQL│         │ PostgreSQL│
    │           │         │           │         │           │
    │ invoices  │         │ invoices  │         │ invoices  │
    │ vendors   │         │ vendors   │         │ vendors   │
    │ workflows │         │ workflows │         │ workflows │
    │ audit_log │         │ audit_log │         │ audit_log │
    └───────────┘         └───────────┘         └───────────┘
```

**Rationale:**
- Complete data isolation (regulatory compliance, data sovereignty)
- Per-tenant backup/restore without affecting others
- Easy data export for customer portability
- No risk of cross-tenant data leakage
- Simpler query patterns (no tenant_id filters everywhere)

**Trade-offs:**
- Higher connection overhead → mitigate with connection pooling (PgBouncer)
- Migration complexity → use versioned migration runner per tenant
- 5 pilot customers = 5 databases, manageable at this scale

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         OCR PIPELINE                                 │
│                                                                      │
│  INGEST              PREPROCESS           PROVIDER ROUTER            │
│  ┌──────────┐        ┌──────────┐        ┌───────────────────────┐  │
│  │• PDF     │───────►│• Deskew  │───────►│  Tenant Settings      │  │
│  │• Images  │        │• Enhance │        │  ┌─────────────────┐  │  │
│  │• Email   │        │• Classify│        │  │ privacy_mode?   │  │  │
│  │• API     │        │          │        │  │   │             │  │  │
│  └──────────┘        └──────────┘        │  │   ├── YES ────► Tesseract Only  │
│                                          │  │   │             │  │  │
│                                          │  │   └── NO ─────► Provider Selection │
│                                          │  └─────────────────┘  │  │
│                                          └───────────────────────┘  │
│                                                     │                │
│                           ┌─────────────────────────┴────────┐      │
│                           ▼                                  ▼      │
│                    ┌─────────────┐                    ┌───────────┐ │
│                    │  Tesseract  │                    │  Textract │ │
│                    │   (Local)   │                    │   (AWS)   │ │
│                    │  DEFAULT    │                    │  FALLBACK │ │
│                    └──────┬──────┘                    └─────┬─────┘ │
│                           │                                  │      │
│                           └──────────────┬───────────────────┘      │
│                                          ▼                          │
│  EXTRACTION              VALIDATION              ROUTING            │
│  ┌──────────────┐        ┌──────────────┐       ┌───────────────┐  │
│  │ Header:      │        │ Required     │       │ Confidence    │  │
│  │ • Vendor     │───────►│ fields check │──────►│ Routing:      │  │
│  │ • Invoice #  │        │ Format valid │       │               │  │
│  │ • Date       │        │ Duplicate    │       │ ≥85% → AP     │  │
│  │ • Amount     │        │ detection    │       │ 70-84% → Review│ │
│  │ • Due Date   │        │              │       │ <70% → Error  │  │
│  │              │        │              │       │               │  │
│  │ Line Items:  │        │              │       │               │  │
│  │ • Desc/Qty   │        │              │       │               │  │
│  │ • Unit/Total │        │              │       │               │  │
│  └──────────────┘        └──────────────┘       └───────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

**OCR Provider Strategy:**

| Provider | When Used | Cost | Privacy |
|----------|-----------|------|---------|
| Tesseract 5 (Local) | Default for all documents | Free | Full isolation |
| AWS Textract | Confidence < 75% on Tesseract | $1.50/1000 pages | Data leaves premises |
| Google Vision | Backup/comparison | $1.50/1000 pages | Data leaves premises |

**Confidence Thresholds:**
- **≥85%**: Auto-route to AP Queue (ready for approval)
- **70-84%**: Route to Review Queue (human verification of flagged fields)
- **<70%**: Route to Error Queue (manual data entry required)

### 1.4 Approval Workflow Engine

```
┌─────────────────────────────────────────────────────────────────────┐
│                    APPROVAL WORKFLOW ENGINE                         │
│                                                                     │
│   RULE ENGINE (Rust Expression Evaluator)                           │
│   ┌─────────────────────────────────────────────────────────────┐  │
│   │  Rules are tenant-configurable, evaluated in order:          │  │
│   │                                                              │  │
│   │  rule: amount < 5000                                         │  │
│   │  action: auto_approve                                        │  │
│   │                                                              │  │
│   │  rule: amount >= 5000 && amount < 25000                      │  │
│   │  action: require_approval(level: 1)                          │  │
│   │                                                              │  │
│   │  rule: amount >= 25000 && amount < 50000                     │  │
│   │  action: require_approval(level: 2)                          │  │
│   │                                                              │  │
│   │  rule: amount >= 50000                                       │  │
│   │  action: require_approval(level: 3, role: "cfo")             │  │
│   │                                                              │  │
│   │  rule: vendor.is_new == true                                 │  │
│   │  action: add_review_step(type: "new_vendor_review")          │  │
│   │                                                              │  │
│   │  rule: invoice.po_mismatch > 0.05                            │  │
│   │  action: route_to_exception_queue                            │  │
│   └─────────────────────────────────────────────────────────────┘  │
│                               │                                     │
│   STATE MACHINE               ▼                                     │
│   ┌─────────────────────────────────────────────────────────────┐  │
│   │                                                              │  │
│   │    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌────────┐ │  │
│   │    │ PENDING │───►│ L1_APPR │───►│ L2_APPR │───►│APPROVED│ │  │
│   │    └────┬────┘    └────┬────┘    └────┬────┘    └────────┘ │  │
│   │         │              │              │                     │  │
│   │         ▼              ▼              ▼                     │  │
│   │    ┌─────────┐    ┌─────────┐    ┌─────────┐              │  │
│   │    │REJECTED │    │ ON_HOLD │    │EXCEPTION│              │  │
│   │    └─────────┘    └─────────┘    └─────────┘              │  │
│   │                                                              │  │
│   └─────────────────────────────────────────────────────────────┘  │
│                               │                                     │
│   NOTIFICATION SERVICE        ▼                                     │
│   ┌─────────────────────────────────────────────────────────────┐  │
│   │  • Email approvals with signed action links (no login)       │  │
│   │  • In-app notification badges and alerts                     │  │
│   │  • SLA escalation (configurable hours before escalate)       │  │
│   │  • Delegation auto-routing (vacation coverage)               │  │
│   │  • Audit trail on every action                               │  │
│   └─────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

**Email Approval Security:**
- Signed tokens using HMAC-SHA256 with tenant-specific secrets
- Token expiration (24 hours default, configurable)
- Single-use tokens (invalidated after action)
- IP logging for audit trail
- Rate limiting per token

---

## 2. Technology Stack Decisions

### 2.1 Backend Stack (Rust)

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Framework** | Axum 0.7+ | CEO preference, tower middleware, async-first |
| **Runtime** | Tokio | Industry standard async runtime |
| **Database** | SQLx | Compile-time query verification, async |
| **Serialization** | Serde | De facto Rust standard |
| **Validation** | validator | Derive-macro based request validation |
| **Error Handling** | thiserror | Structured API errors |
| **Logging** | tracing | Structured logs, distributed tracing spans |
| **Configuration** | config-rs | Environment + file + defaults |
| **HTTP Client** | reqwest | Async HTTP for OCR providers |
| **JWT** | jsonwebtoken | Token generation/validation |

**Why Rust?**
- Performance critical for OCR pipeline (10-100x vs Python)
- Memory safety essential for multi-tenant data isolation
- CEO preference and long-term maintainability
- Trade-off: Slower initial velocity, steeper learning curve

### 2.2 Frontend Stack (Next.js)

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Framework** | Next.js 14+ (App Router) | CEO preference, RSC, server actions |
| **Language** | TypeScript (strict) | Type safety, IDE support |
| **Styling** | Tailwind CSS + shadcn/ui | CEO preference, consistent design |
| **State** | TanStack Query v5 | Server state, caching, mutations |
| **Forms** | React Hook Form + Zod | Type-safe validation |
| **Tables** | TanStack Table | Invoice lists, queue views |
| **Charts** | Recharts | Analytics dashboards |
| **Auth** | NextAuth.js v5 | Session management |

### 2.3 Data Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **OLTP** | PostgreSQL 16 | CEO preference, JSONB, extensions |
| **Analytics** | DuckDB | CEO preference, embedded, fast aggregations |
| **Documents** | MinIO | S3-compatible, local development |
| **Cache** | Redis | Sessions, rate limiting, queues |
| **Search** | pg_trgm + Full-Text | Start simple, Elasticsearch later |

### 2.4 Infrastructure

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Containers** | Docker | Standard, portable |
| **Development** | Docker Compose | Local orchestration |
| **Production** | Kubernetes (later) | Start with single VM, scale later |
| **CI/CD** | GitHub Actions | Integrated with repo |
| **Monitoring** | Prometheus + Grafana | Open source, extensible |
| **Tracing** | OpenTelemetry | Vendor-neutral |

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)

**Goal:** Establish project infrastructure and development environment

#### Week 1: Project Setup

| Task | Owner | Deliverable |
|------|-------|-------------|
| Create monorepo structure | Backend | Cargo workspace + pnpm workspace |
| Docker Compose environment | Backend | PostgreSQL, Redis, MinIO running |
| CI/CD pipeline | Backend | GitHub Actions: lint, test, build |
| Tenant management service | Backend | CRUD for tenants, database provisioning |
| Database migrations | Backend | sqlx-cli setup, control plane schema |

#### Week 2: Auth & Frontend

| Task | Owner | Deliverable |
|------|-------|-------------|
| Authentication service | Backend | JWT issuance, API key validation |
| API gateway scaffold | Backend | Axum with tenant resolution middleware |
| Next.js scaffold | Frontend | App router, shadcn/ui configured |
| Login/auth UI | Frontend | Login page, protected routes |
| Development fixtures | Backend | Seed data for local development |

**Phase 0 Deliverables:**
- [ ] Monorepo: `bill-forge/` with Cargo workspace + pnpm workspace
- [ ] Docker Compose: PostgreSQL, Redis, MinIO
- [ ] Auth: JWT login, API key generation
- [ ] CI: Tests run on every PR
- [ ] Dev fixtures: 2 test tenants with sample data

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Goal:** Upload invoices, extract data via OCR, route to queues

#### Week 3-4: OCR Pipeline

| Task | Owner | Deliverable |
|------|-------|-------------|
| Document upload API | Backend | `POST /invoices/upload` → S3 |
| Tesseract integration | Backend | `bf-ocr` crate with provider trait |
| Field extraction | Backend | Vendor, invoice #, amount, date |
| Confidence scoring | Backend | Per-field and overall confidence |
| Queue routing logic | Backend | AP queue, review queue, error queue |
| AWS Textract integration | Backend | Fallback provider for low confidence |

#### Week 5-6: Invoice Capture UI

| Task | Owner | Deliverable |
|------|-------|-------------|
| Upload interface | Frontend | Drag-drop, multi-file, progress |
| Invoice preview | Frontend | PDF viewer with field highlighting |
| Queue views | Frontend | AP queue, review queue, error queue |
| Manual correction UI | Frontend | Edit extracted fields, re-submit |
| Vendor matching UI | Frontend | Suggest matches, create new vendor |
| Extraction dashboard | Frontend | Accuracy metrics, queue depths |

**Phase 1 Deliverables:**
- [ ] Upload API with S3 storage integration
- [ ] Tesseract OCR extracting: vendor, invoice #, amount, date, due date
- [ ] AWS Textract fallback for confidence < 75%
- [ ] Confidence-based routing (85%/70% thresholds)
- [ ] Invoice capture UI with upload, preview, correction
- [ ] Queue views (AP, review, error)

**Success Metrics:**
- 85%+ extraction accuracy on clean PDF invoices
- < 5 second processing time (upload to queue)
- Manual correction reduces error rate by 90%

### Phase 2: Invoice Processing MVP (Weeks 7-10)

**Goal:** Configurable approval workflows with email actions

#### Week 7-8: Workflow Engine

| Task | Owner | Deliverable |
|------|-------|-------------|
| Rule engine design | Backend | Expression evaluator for conditions |
| Approval state machine | Backend | Pending → L1 → L2 → Approved states |
| Workflow configuration | Backend | CRUD for workflow rules |
| Approval actions | Backend | Approve, reject, hold, delegate |
| Rule configuration UI | Frontend | Visual rule builder |
| Approval inbox | Frontend | Pending approvals, actions |

#### Week 9-10: Email & SLA

| Task | Owner | Deliverable |
|------|-------|-------------|
| Email notifications | Backend | SMTP integration, templates |
| Signed action links | Backend | HMAC tokens for email approve/reject |
| Email approval endpoints | Backend | `GET /actions/{token}/approve` |
| Delegation support | Backend | Out-of-office auto-routing |
| SLA tracking | Backend | Time in queue, escalation triggers |
| Audit trail | Backend | Log every action with actor, timestamp |
| Bulk operations | Frontend | Select multiple, batch submit |

**Phase 2 Deliverables:**
- [ ] Rule engine: amount-based routing, exception handling
- [ ] Multi-level approval chains (3 levels)
- [ ] Email approve/reject without login
- [ ] Delegation configuration
- [ ] SLA monitoring (time in queue, escalations)
- [ ] Complete audit trail

**Success Metrics:**
- < 3 second approval action latency
- Email approval success rate > 95%
- 100% audit coverage

### Phase 3: Pilot Launch (Weeks 11-12)

**Goal:** Production deployment, 5 pilot customers onboarded

#### Week 11: Production Readiness

| Task | Owner | Deliverable |
|------|-------|-------------|
| Production environment | SRE | Cloud deployment (AWS/GCP) |
| Security audit | Security | Penetration testing, vulnerability scan |
| Load testing | QA | 100 invoices/minute throughput |
| Monitoring setup | SRE | Prometheus, Grafana, alerting |
| Documentation | All | API docs, user guides, runbooks |

#### Week 12: Customer Onboarding

| Task | Owner | Deliverable |
|------|-------|-------------|
| Onboarding workflow | Product | Tenant provisioning automation |
| Data migration tools | Backend | Import from CSV/Excel |
| Customer support setup | Product | Ticketing, escalation procedures |
| Feedback collection | Product | In-app feedback, weekly calls |
| Pilot kickoff | Product | 5 customers live |

**Phase 3 Deliverables:**
- [ ] Production deployment on cloud infrastructure
- [ ] Security audit passed
- [ ] Load test: 100 invoices/minute sustained
- [ ] 5 pilot customers onboarded and processing invoices
- [ ] Support procedures documented

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy below 90%** | Medium | High | Multi-provider fallback, collect training data from corrections, human-in-loop for low confidence |
| **Rust learning curve** | Medium | Medium | Pair programming, code reviews, experienced Rust hire, consider Go for non-critical services |
| **Tenant isolation breach** | Low | Critical | Database-per-tenant, penetration testing, row-level security as defense-in-depth |
| **Email approval spoofing** | Medium | High | HMAC-signed tokens, expiration, single-use, IP logging |
| **DuckDB scale limits** | Low | Medium | Partition by time, archive old data, evaluate ClickHouse for 100+ tenants |

### 4.2 Product Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep before PMF** | High | High | Weekly scope reviews, strict adherence to anti-goals |
| **Pilot customer churn** | Medium | High | Weekly check-ins, fast bug resolution, dedicated support |
| **ERP integration complexity** | High | Medium | Start with QuickBooks (simplest API), use existing libraries |
| **Competitor response** | Medium | Low | Focus on mid-market speed/simplicity, avoid enterprise features |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss** | Low | Critical | Daily backups, PITR, cross-region replication |
| **Extended outage** | Medium | High | Multi-AZ deployment, health checks, failover automation |
| **Key person dependency** | High | High | Documentation, knowledge sharing, pair programming |

---

## 5. Resource Requirements

### 5.1 Team Structure (5.5 FTE)

```
CORE ENGINEERING (4 FTE)
├── Backend Engineer (Rust) - 2 FTE
│   └── API, OCR pipeline, workflow engine
├── Frontend Engineer (Next.js) - 1 FTE
│   └── UI components, dashboards
└── Full-Stack Engineer - 1 FTE
    └── Integration, DevOps, gap filling

SPECIALIST ROLES (1.5 FTE)
├── ML/AI Engineer (Contract) - 0.5 FTE
│   └── OCR optimization, Winston AI (Phase 3+)
└── Product Manager - 1 FTE
    └── Pilot relationships, prioritization
```

### 5.2 Infrastructure Costs

| Component | Development | Production (5 pilots) |
|-----------|-------------|----------------------|
| Cloud Compute | $200/mo | $800/mo |
| PostgreSQL (managed) | $50/mo | $300/mo |
| Redis | $20/mo | $100/mo |
| S3 Storage (100GB) | $10/mo | $50/mo |
| AWS Textract | $0 (Tesseract) | $200/mo |
| Monitoring | $0 (self-hosted) | $100/mo |
| **Total** | **$280/mo** | **$1,550/mo** |

### 5.3 Tools

| Tool | Cost | Purpose |
|------|------|---------|
| GitHub Team | $4/user/mo | Source control, CI/CD |
| Linear | $8/user/mo | Issue tracking |
| Figma | $15/user/mo | Design |
| Sentry | Free tier | Error tracking |

---

## 6. Leveraging Existing Codebase

### 6.1 Locust → Winston AI Adaptation

The existing Locust codebase at `/Users/mark/sentinel/locust` is a multi-agent AI framework that can be adapted for Winston. Key components to leverage:

**Keep (adapt for Bill Forge):**
- `src/locust/llm/` - LLM backend abstraction (Claude, Ollama)
- `src/locust/workflows/` - LangGraph workflow patterns
- `src/locust/agents/base.py` - Agent base class design
- Rate limiting and fallback mechanisms

**Modify:**
- Remove development agents (CTO, CPO, Backend Dev, etc.)
- Create Bill Forge domain tools:
  - `invoice_search(query)` - Search invoices by vendor, amount, date
  - `approval_status(invoice_id)` - Get approval workflow status
  - `perform_action(invoice_id, action)` - Approve, reject, hold
  - `vendor_lookup(name)` - Vendor disambiguation
- Add tenant context to all operations
- Integrate with Bill Forge REST API

**Timeline:** Phase 3+ (post-MVP), 2-3 weeks adaptation

### 6.2 What NOT to Use from Locust

- SQLite storage (replace with PostgreSQL)
- CLI interface (replace with Next.js UI)
- Development-focused agents
- MCP tools (replace with Bill Forge-specific tools)

---

## 7. Answers to CEO Questions

### Q1: Palette/Rillion Differentiation

**Their Strengths:**
- Established in Nordic/European markets
- Deep ERP integrations (SAP, Oracle)
- Mature workflow engine

**Their Weaknesses:**
- Slow, dated UI (common complaint in reviews)
- Expensive for mid-market
- Limited AI innovation

**Our Differentiation:**
1. **Speed** - Modern UI, fast OCR, sub-second actions
2. **Simplicity** - Buy what you need, not a monolithic suite
3. **Privacy** - Local-first OCR option
4. **AI-First** - Winston assistant for natural language queries
5. **Pricing** - Usage-based, transparent

### Q2: OCR Accuracy Threshold

**Recommendation: 85% confidence for auto-approval**

- **≥85%** → AP Queue (auto-flow to approval)
- **70-84%** → Review Queue (human verification of flagged fields)
- **<70%** → Error Queue (manual data entry)

This balances automation rate (~70% of clean invoices) with acceptable error cost.

### Q3: First ERP Integration

**Recommendation: QuickBooks Online**

1. **QuickBooks Online** (Priority 1) - Largest mid-market share, simple REST API
2. **NetSuite** (Priority 2) - Common in growing companies
3. **Sage** (Priority 3) - Strong in manufacturing

QuickBooks has the most accessible API and largest addressable market in our target segment.

### Q4: Common Approval Patterns

**Top 3 patterns to implement:**

1. **Amount-Based Tiering** (80% of mid-market)
   - < $5K → Auto-approve or manager
   - $5K-$25K → Department head
   - $25K-$50K → Finance director
   - > $50K → CFO

2. **Exception-Only Review** (60%)
   - Auto-approve if PO matches
   - Route for review only on mismatch

3. **Department Routing** (40%)
   - Cost center owner approval
   - Finance approval above threshold

### Q5: Multi-Currency Handling

**MVP Approach:**
- Store original currency + converted base amount
- Use Open Exchange Rates API (free tier: 1000 calls/month)
- Allow manual rate override
- Display both currencies in UI

**Defer to Phase 2:**
- Full multi-currency GL posting
- Historical rate lookups
- Automatic rate refreshes

### Q6: Pricing Model

**Recommendation: Tiered Usage-Based**

| Tier | Invoices/Month | Price | Overage |
|------|---------------|-------|---------|
| Starter | 0-500 | $299/mo | $0.75/invoice |
| Growth | 0-2,000 | $799/mo | $0.50/invoice |
| Scale | 0-10,000 | $1,999/mo | $0.30/invoice |

**Add-on Modules:**
- Vendor Management: +$199/mo
- Winston AI: +$299/mo (Phase 3+)
- Premium OCR (Textract-first): +$149/mo

**Why this works:**
- Predictable base cost (finance teams prefer this)
- Scales with growth without cliff pricing
- No per-seat licensing (AP teams hate this)
- Module add-ons preserve modular architecture value prop

---

## 8. Monorepo Structure

```
bill-forge/
├── Cargo.toml                    # Workspace root
├── package.json                  # pnpm workspace root
├── pnpm-workspace.yaml
├── docker-compose.yml            # Local development
├── .github/workflows/
│   ├── ci.yml                    # Lint, test, build
│   └── deploy.yml                # Production deployment
│
├── crates/                       # Rust crates
│   ├── bf-api/                   # API gateway (Axum)
│   ├── bf-invoice/               # Invoice capture service
│   ├── bf-workflow/              # Approval workflow engine
│   ├── bf-vendor/                # Vendor management
│   ├── bf-ocr/                   # OCR provider abstraction
│   ├── bf-storage/               # S3/MinIO abstraction
│   ├── bf-auth/                  # Authentication/authorization
│   ├── bf-tenant/                # Tenant management
│   └── bf-common/                # Shared types, utilities
│
├── apps/
│   └── web/                      # Next.js 14 application
│       ├── package.json
│       ├── next.config.js
│       └── src/
│           ├── app/              # App router pages
│           ├── components/       # UI components
│           └── lib/              # Utilities
│
├── packages/
│   ├── ui/                       # Shared shadcn/ui components
│   └── api-client/               # Generated TypeScript client
│
├── services/
│   └── winston/                  # AI assistant (Phase 3+)
│       ├── pyproject.toml        # Python project
│       └── src/                  # Adapted from Locust
│
├── migrations/
│   ├── control-plane/            # Tenant management schema
│   └── tenant/                   # Per-tenant schema
│
└── docs/
    ├── api/                      # OpenAPI specs
    └── runbooks/                 # Operational guides
```

---

## 9. Success Criteria

### Product Metrics (3-Month)

| Metric | Target |
|--------|--------|
| OCR Accuracy (standard invoices) | ≥90% |
| Processing Latency (upload to queue) | <5 seconds |
| Approval Cycle Time | <24 hours average |
| Email Approval Success Rate | ≥95% |
| System Uptime | ≥99.5% |

### Business Metrics (3-Month)

| Metric | Target |
|--------|--------|
| Pilot Customers | 5 |
| Invoices Processed | 1,000+ total |
| Customer Satisfaction | ≥4/5 |
| Critical Bugs (unresolved P0) | 0 |

### Technical Metrics (3-Month)

| Metric | Target |
|--------|--------|
| Test Coverage | ≥80% |
| API Response Time (P95) | <200ms (non-OCR) |
| Deployment Frequency | Daily to staging |
| Mean Time to Recovery | <1 hour |

---

## 10. Immediate Next Steps

### This Week

1. **Review this plan** with stakeholders, identify blockers
2. **Confirm team assignments** - 2 Rust engineers, 1 frontend, 1 full-stack
3. **Create monorepo** - Initialize bill-forge/ with Cargo workspace
4. **Docker Compose** - PostgreSQL, Redis, MinIO running locally
5. **Pilot outreach** - Identify 10 potential pilot candidates

### Week 1 Deliverables

- [ ] Monorepo initialized (Cargo workspace + pnpm)
- [ ] Docker Compose running all dependencies
- [ ] Basic Axum service with health check endpoint
- [ ] Next.js app with shadcn/ui configured
- [ ] CI pipeline running tests on PR

---

## Appendix: Key Architectural Decisions

| Decision | Choice | Rationale | Trade-offs |
|----------|--------|-----------|------------|
| Language | Rust | Performance, safety, CEO preference | Learning curve, slower initial velocity |
| Multi-tenancy | Database-per-tenant | Data isolation, compliance | Connection overhead, migration complexity |
| OCR default | Tesseract (local) | Privacy, cost | Lower accuracy than cloud |
| Frontend | Next.js 14 | CEO preference, RSC | Framework complexity |
| State management | TanStack Query | Server state focus | Learning curve |
| Auth | JWT + API keys | Standard, stateless | Token management |
| Email approvals | Signed tokens | No login friction | Security requires care |

---

*This plan is a living document. Updates will be made as we learn from implementation and pilot customer feedback.*
