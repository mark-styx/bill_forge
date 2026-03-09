# CTO Strategic Technical Plan: Bill Forge

**Document Version:** 1.0
**Date:** January 31, 2026
**Author:** CTO
**Status:** For CEO Review

---

## Executive Summary

After comprehensive analysis of the existing codebase and the CEO's vision, I present this strategic technical plan for Bill Forge. The current repository contains Locust, a Python-based AI agent orchestration framework—an excellent system, but fundamentally misaligned with the Bill Forge invoice processing platform vision.

**Key Recommendation:** Build Bill Forge as a new project within this monorepo, leveraging the CEO's preferred Rust + Next.js stack, while preserving Locust's agent architecture for the Winston AI module in Phase 3.

**Timeline to MVP:** 12 weeks
**Team Size:** 4-5 FTEs
**Infrastructure Cost:** $500-2,000/month (scaling with usage)

---

## 1. Technical Architecture Recommendations

### 1.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Bill Forge Platform                           │
├─────────────────────────────────────────────────────────────────────────┤
│  FRONTEND TIER                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Next.js 14+ App (TypeScript + Tailwind + shadcn/ui)            │   │
│  │  ├── Invoice Capture UI (upload, preview, correction)          │   │
│  │  ├── Approval Inbox (queue management, bulk actions)           │   │
│  │  ├── Vendor Management (CRUD, W-9 storage)                     │   │
│  │  ├── Reporting Dashboards (DuckDB-powered)                     │   │
│  │  └── Winston AI Chat Interface (Phase 3)                       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────┤
│  API GATEWAY                                                            │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Rust + Axum (REST API)                                         │   │
│  │  ├── Authentication (JWT + API Keys)                            │   │
│  │  ├── Rate Limiting (Redis-backed)                               │   │
│  │  ├── Request Validation (serde + validator)                     │   │
│  │  └── Tenant Context Injection                                   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────┤
│  SERVICE LAYER (Rust Crates)                                            │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │   Invoice    │ │   Workflow   │ │    Vendor    │ │   Winston    │   │
│  │   Capture    │ │   Engine     │ │  Management  │ │  AI (Python) │   │
│  │              │ │              │ │              │ │              │   │
│  │ - OCR Pipe   │ │ - Approvals  │ │ - Master Data│ │ - LangGraph  │   │
│  │ - Extraction │ │ - Routing    │ │ - W-9 Store  │ │ - Embeddings │   │
│  │ - Validation │ │ - SLA Track  │ │ - Matching   │ │ - NL Queries │   │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │
├─────────────────────────────────────────────────────────────────────────┤
│  DATA TIER                                                              │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │  PostgreSQL  │ │   DuckDB     │ │    MinIO     │ │    Redis     │   │
│  │   (OLTP)     │ │  (Analytics) │ │  (Documents) │ │   (Cache)    │   │
│  │              │ │              │ │              │ │              │   │
│  │ Per-tenant   │ │ Embedded     │ │ S3-compat    │ │ Sessions     │   │
│  │ schemas      │ │ analytics    │ │ PDF storage  │ │ Rate limits  │   │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Multi-Tenant Architecture

**Strategy: Schema-per-Tenant in PostgreSQL**

```
PostgreSQL Instance
├── public schema (control plane)
│   ├── tenants
│   ├── users (cross-tenant, for platform admins)
│   └── audit_log_global
└── tenant_{uuid} schema (per tenant)
    ├── invoices
    ├── invoice_line_items
    ├── vendors
    ├── approval_workflows
    ├── approval_steps
    ├── approval_history
    ├── users (tenant-scoped)
    ├── roles
    └── audit_log
```

**Rationale:**
- Complete data isolation without operational overhead of separate databases
- Efficient for mid-market (10-1000 employees per tenant)
- Simple backup/restore per tenant
- Can migrate large tenants to dedicated databases later if needed

### 1.3 OCR Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          OCR Processing Pipeline                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌──────────┐    ┌──────────┐    ┌──────────────────────────────────┐ │
│   │  Upload  │───▶│ Preproc  │───▶│      OCR Provider Selection      │ │
│   │  (MinIO) │    │ (ImageMagick)│ │                                  │ │
│   └──────────┘    └──────────┘    │  ┌─────────┐  ┌─────────────────┐│ │
│                                    │  │Tesseract│  │ AWS Textract    ││ │
│                                    │  │(local)  │  │ (cloud fallback)││ │
│                                    │  └────┬────┘  └────────┬────────┘│ │
│                                    │       │                │         │ │
│                                    └───────┴────────────────┴─────────┘ │
│                                            │                            │
│                                            ▼                            │
│                              ┌─────────────────────────┐                │
│                              │   Field Extraction      │                │
│                              │   - Vendor name         │                │
│                              │   - Invoice number      │                │
│                              │   - Date                │                │
│                              │   - Amount              │                │
│                              │   - Line items          │                │
│                              └───────────┬─────────────┘                │
│                                          │                              │
│                                          ▼                              │
│                              ┌─────────────────────────┐                │
│                              │   Confidence Scoring    │                │
│                              │   Threshold: 85%        │                │
│                              └───────────┬─────────────┘                │
│                                          │                              │
│                    ┌─────────────────────┼─────────────────────┐        │
│                    ▼                     │                     ▼        │
│         ┌──────────────────┐             │         ┌──────────────────┐ │
│         │   AP Queue       │             │         │   Error Queue    │ │
│         │   (High Conf)    │◀────────────┘         │   (Low Conf)     │ │
│         │   Auto-process   │                       │   Manual review  │ │
│         └──────────────────┘                       └──────────────────┘ │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**OCR Provider Strategy:**

| Provider | Use Case | Cost | Accuracy |
|----------|----------|------|----------|
| **Tesseract 5** (primary) | Local processing, privacy-first | Free | 85-90% |
| **AWS Textract** (fallback) | Low-confidence retries, complex docs | $1.50/1000 pages | 95%+ |
| **Google Vision** (optional) | Handwritten content | $1.50/1000 pages | 95%+ |

**Confidence Thresholds:**
- **≥85%**: Route to AP Queue (auto-processable)
- **70-84%**: Route to AP Queue with review flag
- **<70%**: Route to Error Queue (manual correction required)

### 1.4 Approval Workflow Engine

```yaml
# Example Workflow Definition (stored in JSONB)
workflow:
  name: "Standard AP Approval"
  version: 1
  rules:
    - condition: "amount < 500"
      action: "auto_approve"

    - condition: "amount >= 500 AND amount < 5000"
      action: "require_approval"
      approvers: ["department_manager"]
      sla_hours: 24

    - condition: "amount >= 5000 AND amount < 50000"
      action: "require_approval"
      approvers: ["department_manager", "finance_lead"]
      sla_hours: 48

    - condition: "amount >= 50000"
      action: "require_approval"
      approvers: ["department_manager", "finance_lead", "controller"]
      escalation:
        after_hours: 72
        escalate_to: ["cfo"]
      sla_hours: 72

  exception_handling:
    duplicate_detection: true
    vendor_mismatch: "route_to_error_queue"
    missing_po: "hold_for_review"
```

---

## 2. Technology Stack Decisions

### 2.1 Core Stack (Aligned with CEO Preferences)

| Layer | Technology | Version | Rationale |
|-------|-----------|---------|-----------|
| **Backend Runtime** | Rust | 1.75+ | Performance, memory safety, CEO preference |
| **Web Framework** | Axum | 0.7+ | Async, ergonomic, well-maintained |
| **ORM** | SeaORM | 0.12+ | Async, type-safe, multi-database |
| **Serialization** | Serde | 1.0+ | Industry standard |
| **Validation** | Validator | 0.16+ | Declarative, composable |
| **Frontend** | Next.js | 14+ | App Router, React Server Components |
| **UI Components** | shadcn/ui | Latest | CEO preference, accessible |
| **Styling** | Tailwind CSS | 3.4+ | CEO preference |
| **State Management** | TanStack Query | 5.0+ | Server state, caching |
| **Forms** | React Hook Form + Zod | Latest | Type-safe validation |

### 2.2 Data Layer

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| **OLTP Database** | PostgreSQL 16+ | Mature, JSONB, row-level security |
| **Analytics Database** | DuckDB | Embedded, fast analytics, CEO preference |
| **Document Storage** | MinIO | S3-compatible, self-hosted option |
| **Cache** | Redis 7+ | Sessions, rate limiting, queues |
| **Search** | PostgreSQL FTS + pg_trgm | Sufficient for MVP, avoid Elasticsearch complexity |

### 2.3 OCR & Document Processing

| Component | Technology | Notes |
|-----------|-----------|-------|
| **Primary OCR** | Tesseract 5 | Local-first, privacy compliance |
| **Cloud OCR** | AWS Textract | Fallback for low-confidence |
| **Image Processing** | ImageMagick/libvips | Preprocessing (deskew, denoise) |
| **PDF Processing** | pdf-rs + image-rs | Native Rust handling |

### 2.4 Infrastructure

| Component | Technology | Notes |
|-----------|-----------|-------|
| **Container Runtime** | Docker + Compose | Local development |
| **Container Orchestration** | Kubernetes (later) | Production scaling |
| **CI/CD** | GitHub Actions | Integrated with repo |
| **Monitoring** | Prometheus + Grafana | Metrics and alerting |
| **Logging** | Structured JSON + Loki | Centralized logging |
| **Secrets** | Vault or SOPS | Secure secrets management |

### 2.5 Winston AI Module (Phase 3)

| Component | Technology | Notes |
|-----------|-----------|-------|
| **Orchestration** | LangGraph | Leverage existing Locust patterns |
| **LLM Backend** | Claude API | Primary |
| **Embeddings** | Voyage AI or local | Semantic search |
| **Vector Store** | pgvector | PostgreSQL extension |

### 2.6 Technology Decisions Deferred

- **SSO/SAML**: Not before product-market fit (CEO anti-goal)
- **Mobile App**: Not before web is solid (CEO anti-goal)
- **Kubernetes**: Not until 10+ customers require scaling
- **Multi-region**: Defer until enterprise demand

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Establish development infrastructure and base architecture

| Priority | Deliverable | Owner | Dependencies |
|----------|-------------|-------|--------------|
| P0.1 | Monorepo structure (Cargo workspace + pnpm) | Backend Lead | None |
| P0.2 | Docker Compose (PostgreSQL, Redis, MinIO) | SRE | None |
| P0.3 | CI/CD pipeline (lint, test, build) | SRE | P0.1 |
| P0.4 | Base Axum API structure with health endpoints | Backend Lead | P0.1 |
| P0.5 | Database migrations framework (SeaORM) | Backend Lead | P0.2 |
| P0.6 | Next.js app scaffold with shadcn/ui | Frontend Lead | None |
| P0.7 | Development environment documentation | All | P0.1-P0.6 |

**Exit Criteria:**
- [ ] `cargo build --workspace` succeeds
- [ ] `pnpm build` succeeds
- [ ] `docker compose up` starts all services
- [ ] Health check endpoints return 200
- [ ] CI pipeline passes on main branch

### Phase 1: Invoice Capture MVP (Weeks 3-6)

**Objective:** Enable invoice upload, OCR processing, and manual correction

| Priority | Deliverable | Owner | Dependencies |
|----------|-------------|-------|--------------|
| P1.1 | Tenant management API (CRUD, schema creation) | Backend | P0.5 |
| P1.2 | User authentication (JWT + API keys) | Backend | P1.1 |
| P1.3 | Invoice upload endpoint (multipart + S3) | Backend | P1.1 |
| P1.4 | Tesseract 5 integration (FFI wrapper) | Backend | None |
| P1.5 | Field extraction pipeline (vendor, invoice#, amount, date) | Backend | P1.4 |
| P1.6 | Confidence scoring algorithm | Backend | P1.5 |
| P1.7 | Queue routing logic (AP queue vs error queue) | Backend | P1.6 |
| P1.8 | Vendor master data API | Backend | P1.1 |
| P1.9 | Vendor fuzzy matching (pg_trgm) | Backend | P1.8 |
| P1.10 | Invoice upload UI (drag-drop, preview) | Frontend | P1.3 |
| P1.11 | Manual correction interface | Frontend | P1.5 |
| P1.12 | AP Queue view (list, filter, sort) | Frontend | P1.7 |
| P1.13 | Error Queue view with inline editing | Frontend | P1.7 |

**Exit Criteria:**
- [ ] Upload PDF → Extract fields → Route to queue (end-to-end)
- [ ] 85%+ accuracy on standard invoice formats
- [ ] Manual correction saves updates correctly
- [ ] Vendor matching finds existing vendors

### Phase 2: Invoice Processing (Weeks 7-10)

**Objective:** Enable approval workflows, routing rules, and email approvals

| Priority | Deliverable | Owner | Dependencies |
|----------|-------------|-------|--------------|
| P2.1 | Workflow engine core (rule evaluation) | Backend | P1.7 |
| P2.2 | Approval routing (amount-based, department) | Backend | P2.1 |
| P2.3 | Approval API (approve, reject, delegate) | Backend | P2.1 |
| P2.4 | Email approval links (signed tokens, 72h expiry) | Backend | P2.3 |
| P2.5 | SLA tracking and escalation | Backend | P2.1 |
| P2.6 | Audit trail (who approved what, when) | Backend | P2.3 |
| P2.7 | Bulk submit for payment batches | Backend | P2.3 |
| P2.8 | Workflow configuration UI | Frontend | P2.1 |
| P2.9 | Approval inbox (pending, approved, rejected) | Frontend | P2.3 |
| P2.10 | Email templates (approval request, reminder) | Backend | P2.4 |
| P2.11 | Delegation management UI | Frontend | P2.3 |

**Exit Criteria:**
- [ ] Invoice flows through multi-level approval
- [ ] Amount-based routing works correctly
- [ ] Email approval links function without login
- [ ] SLA violations trigger escalation
- [ ] Full audit trail captured

### Phase 3: Polish & Pilot (Weeks 11-12)

**Objective:** Production readiness, first customer onboarding

| Priority | Deliverable | Owner | Dependencies |
|----------|-------------|-------|--------------|
| P3.1 | Error handling improvements | Backend | All |
| P3.2 | Rate limiting implementation | Backend | P0.4 |
| P3.3 | Monitoring dashboards (Grafana) | SRE | P0.3 |
| P3.4 | Alerting rules (PagerDuty/Slack) | SRE | P3.3 |
| P3.5 | Backup/restore procedures | SRE | P1.1 |
| P3.6 | Security audit (OWASP Top 10) | Security | All |
| P3.7 | Performance testing (k6) | QA | All |
| P3.8 | User documentation | Product | All |
| P3.9 | Pilot customer environments | SRE | P1.1 |
| P3.10 | Onboarding workflow | Product | P3.8 |

**Exit Criteria:**
- [ ] 5 pilot customer environments provisioned
- [ ] P95 latency < 500ms for all endpoints
- [ ] Zero critical security vulnerabilities
- [ ] Runbook for common operational tasks

### Phase 4: ERP Integration (Weeks 13-16, Post-MVP)

**Objective:** First ERP integration for seamless data flow

| Priority | Deliverable | Notes |
|----------|-------------|-------|
| P4.1 | Integration framework architecture | Abstract provider interface |
| P4.2 | QuickBooks Online integration | Recommended first integration |
| P4.3 | Vendor sync (bidirectional) | Import/export vendor master |
| P4.4 | GL code mapping | Map invoice items to chart of accounts |
| P4.5 | Payment status webhook | Update invoice status from ERP |

**QuickBooks Recommended First:**
- Largest mid-market install base
- Well-documented REST API
- OAuth 2.0 authentication
- Sandbox environment available

### Phase 5: Winston AI (Weeks 17-20, Post-MVP)

**Objective:** Conversational AI assistant for platform navigation and queries

| Priority | Deliverable | Notes |
|----------|-------------|-------|
| P5.1 | LangGraph agent framework (port from Locust) | Reuse existing patterns |
| P5.2 | Invoice data access tools | Read-only invoice queries |
| P5.3 | Natural language query parsing | "Show invoices from Acme" |
| P5.4 | Anomaly detection | Duplicate invoices, unusual amounts |
| P5.5 | Chat UI integration | Embedded in main app |

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy below 90%** | Medium | High | Multi-provider fallback, confidence thresholds, manual correction UI |
| **Rust team onboarding delays** | Medium | High | Pair programming, comprehensive documentation, consider senior Rust contractor |
| **Performance issues at scale** | Low | Medium | Early load testing, horizontal scaling design, connection pooling |
| **PostgreSQL schema-per-tenant limits** | Low | Medium | Monitor schema count, plan migration path to separate databases |
| **Third-party OCR API changes** | Low | Medium | Abstract provider interface, multiple providers |

### 4.2 Product Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Scope creep before PMF** | High | High | Strict adherence to MVP scope, CEO-approved anti-goals list |
| **Pilot customer churn** | Medium | High | Weekly check-ins, fast iteration on feedback, dedicated support |
| **Competitor feature parity pressure** | Medium | Medium | Focus on differentiators (speed, simplicity, local-first) |
| **Complex workflow requirements** | Medium | Medium | Start simple, iterate based on actual customer needs |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Data loss/corruption** | Low | Critical | Automated backups, point-in-time recovery, backup testing |
| **Security breach** | Low | Critical | Security audit, penetration testing, bug bounty (later) |
| **Compliance violations** | Low | High | SOC 2 roadmap, data encryption at rest/transit |
| **Vendor lock-in** | Low | Medium | Abstract storage/OCR interfaces, self-hosted alternatives |

### 4.4 Risk Matrix Summary

```
                    IMPACT
                    Low    Medium    High    Critical
P   High            -      -         Scope   -
R                                    creep
O   Medium          -      Competitor OCR     Customer
B                          pressure  accuracy churn
A   Low             -      Perf,     Workflow Data loss,
B                          vendor    complex  Security
I                          lock-in
L
I
T
Y
```

---

## 5. Resource Requirements

### 5.1 Team Composition

| Role | Count | Seniority | Responsibilities |
|------|-------|-----------|------------------|
| **Backend Engineer (Rust)** | 2 | Senior | API, OCR pipeline, workflow engine |
| **Frontend Engineer** | 1 | Mid-Senior | Next.js app, UI components |
| **SRE/DevOps** | 0.5 | Senior | Infrastructure, CI/CD, monitoring |
| **Product Manager** | 1 | Mid | Customer management, roadmap, specs |
| **QA Engineer** | 0.5 | Mid | Test automation, manual testing |
| **Design** | 0.5 | Mid | UI/UX, component design |

**Total: 5.5 FTEs for MVP (12 weeks)**

### 5.2 Hiring Priorities

1. **Senior Rust Engineer** (Critical Path)
   - Lead OCR pipeline and workflow engine
   - Establish Rust patterns and mentorship
   - Should have: async Rust, Axum/Actix, PostgreSQL

2. **Mid-Senior Rust Engineer** (Important)
   - API development, database layer
   - Testing infrastructure
   - Should have: Rust fundamentals, web services experience

3. **Frontend Engineer** (Important)
   - Full Next.js application
   - shadcn/ui customization
   - Should have: React, TypeScript, Tailwind, accessibility

### 5.3 Infrastructure Costs (Monthly)

| Environment | Component | Cost Estimate |
|-------------|-----------|---------------|
| **Development** | Docker on local machines | $0 |
| **Staging** | Small VPS (4 vCPU, 8GB) | $50 |
| | PostgreSQL (managed, 1 vCPU) | $25 |
| | MinIO (50GB) | $5 |
| | **Subtotal** | **$80** |
| **Production (5 pilots)** | VPS (8 vCPU, 16GB) | $150 |
| | PostgreSQL (managed, 2 vCPU) | $75 |
| | Redis (managed, 1GB) | $25 |
| | MinIO/S3 (100GB) | $10 |
| | Monitoring (Grafana Cloud) | $50 |
| | **Subtotal** | **$310** |
| **OCR APIs** | AWS Textract (fallback) | ~$100-300 (usage) |

**Total Monthly (Production):** $500-700/month for MVP scale

### 5.4 Tooling & Licenses

| Tool | Cost | Notes |
|------|------|-------|
| GitHub Team | $4/user/month | Already using |
| Figma | $15/user/month | Design collaboration |
| Linear/Jira | $8/user/month | Project management |
| Sentry | $26/month | Error tracking |
| **Total** | ~$100/month | 5-person team |

---

## 6. Timeline Estimates

### 6.1 High-Level Timeline

```
WEEK  1  2  3  4  5  6  7  8  9  10  11  12  13  14  15  16
      ├──────┤  ├───────────────┤  ├────────────────┤  ├────────────────┤
      Phase 0   Phase 1           Phase 2              Phase 3
      Foundation Invoice Capture   Invoice Processing   Polish & Pilot

      ────────────────────────────────────────────────▶
                         MVP Launch
                         (End of Week 12)
```

### 6.2 Milestone Schedule

| Milestone | Target Date | Key Deliverables |
|-----------|-------------|------------------|
| **M0: Dev Ready** | Week 2 | Monorepo, CI/CD, Docker, base API |
| **M1: OCR Working** | Week 4 | Upload → Tesseract → Fields extracted |
| **M2: Queues Working** | Week 6 | Confidence routing, correction UI |
| **M3: Approvals Working** | Week 8 | Basic workflow, approval API |
| **M4: Email Approvals** | Week 10 | Approve via email link |
| **M5: Production Ready** | Week 12 | Monitoring, security, pilots provisioned |
| **M6: First ERP** | Week 16 | QuickBooks Online integration |
| **M7: Winston AI Beta** | Week 20 | NL queries, anomaly detection |

### 6.3 Dependencies and Critical Path

```
                     ┌──────────────────┐
                     │ Monorepo Setup   │
                     │ (Week 1)         │
                     └────────┬─────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
     ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
     │ Auth/Tenant │  │ OCR Pipeline │  │ Frontend    │
     │ (Week 2-3)  │  │ (Week 2-4)   │  │ Scaffold    │
     └──────┬──────┘  └──────┬───────┘  │ (Week 2)    │
            │                │          └──────┬──────┘
            │                │                 │
            └────────┬───────┘                 │
                     ▼                         │
            ┌─────────────────┐                │
            │ Invoice Upload  │◀───────────────┘
            │ API + UI        │
            │ (Week 4-5)      │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │ Queue Routing   │
            │ (Week 5-6)      │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │ Workflow Engine │
            │ (Week 7-8)      │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │ Email Approvals │
            │ (Week 9-10)     │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │ Production      │
            │ Hardening       │
            │ (Week 11-12)    │
            └─────────────────┘
```

**Critical Path:** Monorepo → Auth/Tenant → OCR → Invoice Upload → Queues → Workflows → Email Approvals → Production

---

## 7. Addressing CEO Questions

### 7.1 Palette/Rillion Strengths and Weaknesses

**Strengths:**
- Established market presence in Nordic regions
- Deep ERP integrations (SAP, Oracle)
- Mature workflow engine

**Weaknesses:**
- Legacy architecture, slower iteration
- Complex pricing, expensive for mid-market
- Poor mobile experience
- OCR often requires professional services

**Our Differentiation:**
| Area | Palette | Bill Forge |
|------|---------|------------|
| Speed | Legacy, slow | Rust backend, sub-second response |
| Pricing | Complex tiers | Simple usage-based |
| OCR | Aftermarket add-on | Built-in, local-first option |
| Setup | Weeks with PS | Self-serve in hours |
| Modularity | Monolithic suite | Buy what you need |

### 7.2 Ideal OCR Accuracy Threshold

**Recommendation: 85% confidence threshold**

- **≥85%**: Auto-route to AP Queue (high confidence)
- **70-84%**: AP Queue with review flag (medium confidence)
- **<70%**: Error Queue (requires manual review)

**Rationale:**
- Industry standard for "clean" invoices is 90%+
- 85% balances automation vs. accuracy
- Manual correction UI handles edge cases
- Track and improve thresholds based on correction rates

### 7.3 First ERP Integration Priority

**Recommendation: QuickBooks Online**

| ERP | Market Share (Mid-Market) | API Quality | Integration Effort |
|-----|---------------------------|-------------|-------------------|
| QuickBooks Online | 45%+ | Excellent | 2-3 weeks |
| NetSuite | 15% | Good | 4-5 weeks |
| Sage | 10% | Moderate | 3-4 weeks |
| Dynamics 365 | 8% | Good | 4-5 weeks |

**QuickBooks First Because:**
- Largest mid-market install base
- OAuth 2.0, REST API, well-documented
- Sandbox available for testing
- Fastest time to integration

**NetSuite Second:** Important for companies outgrowing QuickBooks

### 7.4 Common Approval Workflow Patterns

Based on industry research, mid-market companies typically use:

**Pattern 1: Amount-Based Escalation (Most Common)**
```
< $500      → Auto-approve
$500-5K     → Manager
$5K-50K     → Manager + Finance Lead
$50K+       → Manager + Finance Lead + Controller/CFO
```

**Pattern 2: Department + Amount**
```
Any invoice → Department Manager (based on cost center)
> $10K      → + Finance approval
> $50K      → + Executive approval
```

**Pattern 3: Vendor-Based**
```
Known vendor + PO match → Auto-approve
New vendor             → Full approval chain
Non-PO invoice         → Additional review
```

**Recommendation:** Support all three patterns via rule engine; default to Pattern 1.

### 7.5 Multi-Currency and International Invoices

**Competitor Approaches:**
- **Palette:** Full multi-currency, localized tax handling
- **Tipalti:** Excellent international, cross-border payments
- **BILL:** Basic multi-currency, US-focused

**Our Approach (MVP):**
- **Phase 1 (MVP):** USD only, single currency
- **Phase 2:** Multi-currency display (convert to base currency)
- **Phase 3:** True multi-currency with exchange rate sync

**Rationale:** Mid-market US companies rarely have complex multi-currency needs initially. Focus on core value, add later.

### 7.6 Pricing Model for Mid-Market

**Recommendation: Usage-Based with Predictable Tiers**

| Tier | Invoices/Month | Price | Per Invoice |
|------|----------------|-------|-------------|
| Starter | Up to 100 | $99/mo | $0.99 |
| Growth | 101-500 | $299/mo | $0.60-0.80 |
| Scale | 501-2000 | $699/mo | $0.35-0.70 |
| Enterprise | 2000+ | Custom | Negotiated |

**Why This Works:**
- Predictable for budgeting (unlike pure per-invoice)
- Scales with customer success
- Starter tier enables easy adoption
- No seat-based friction (invoice count correlates with company size)

**Add-on Modules (Future):**
- Vendor Management: +$49/mo
- Advanced Analytics: +$99/mo
- Winston AI: +$149/mo
- Premium OCR (Textract-only): +$0.10/invoice

---

## 8. Codebase Strategy

### 8.1 Current State Assessment

The existing `/Users/mark/sentinel/locust` repository contains:
- **Locust:** A Python-based AI agent orchestration framework
- **25,912 lines** of production-quality code
- **Excellent patterns:** LangGraph workflows, async execution, embeddings

**However:** Zero overlap with Bill Forge invoice processing requirements.

### 8.2 Recommended Approach: Hybrid Monorepo

```
/Users/mark/sentinel/locust/
├── /locust                    # Existing AI framework (preserve)
│   └── (existing Python code)
│
├── /bill-forge                # NEW: Invoice platform
│   ├── /crates                # Rust backend
│   │   ├── bf-api             # Axum REST API
│   │   ├── bf-core            # Shared types, errors
│   │   ├── bf-db              # SeaORM, migrations
│   │   ├── bf-ocr             # Tesseract, Textract
│   │   ├── bf-workflow        # Approval engine
│   │   └── bf-storage         # MinIO/S3 abstraction
│   │
│   ├── /web                   # Next.js frontend
│   │   ├── /app               # App router pages
│   │   ├── /components        # UI components
│   │   └── /lib               # Utilities
│   │
│   └── /winston               # Winston AI (Phase 3)
│       └── (adapted from Locust LangGraph)
│
├── /docker                    # Shared Docker configs
├── /docs                      # Documentation
└── Cargo.toml                 # Workspace root
```

### 8.3 What to Preserve from Locust

**Directly Reusable (Winston AI - Phase 3):**
- `locust/workflows/` - LangGraph patterns
- `locust/memory/` - Embeddings and vector search
- `locust/config.py` - Configuration management patterns

**Pattern Reference:**
- Agent abstraction model
- Async execution patterns
- Structured logging approach
- Testing infrastructure

### 8.4 What to Build Fresh

**Everything for Core Platform:**
- Rust API layer (Axum)
- Database layer (SeaORM + PostgreSQL)
- OCR pipeline (Tesseract FFI)
- Workflow engine
- All frontend components

---

## 9. Quality Gates and Success Metrics

### 9.1 Technical Quality Gates

| Gate | Requirement | Enforcement |
|------|-------------|-------------|
| **Code Coverage** | ≥80% for core modules | CI block |
| **Linting** | Zero warnings (clippy, ESLint) | CI block |
| **Type Safety** | 100% typed (Rust guarantees, TS strict) | CI block |
| **API Contracts** | OpenAPI spec, breaking change detection | CI check |
| **Security Scan** | Zero high/critical vulnerabilities | CI block |
| **Performance** | P95 < 500ms for all endpoints | Staging gate |

### 9.2 MVP Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **OCR Accuracy** | ≥90% on standard invoices | Correction rate tracking |
| **Processing Time** | <3 seconds upload-to-queue | APM monitoring |
| **Approval Cycle Time** | 50% reduction vs. manual | Customer survey |
| **Error Rate** | <1% API errors | Sentry/logging |
| **Pilot Retention** | 4/5 pilots continue post-MVP | Customer status |

### 9.3 Launch Readiness Checklist

**Technical:**
- [ ] All Phase 1-3 deliverables complete
- [ ] Performance testing passed (k6)
- [ ] Security audit complete (OWASP)
- [ ] Backup/restore tested
- [ ] Runbook documented
- [ ] On-call rotation established

**Product:**
- [ ] User documentation complete
- [ ] Onboarding flow tested with real users
- [ ] Support processes defined
- [ ] Feedback collection mechanism in place

**Business:**
- [ ] Pilot customer agreements signed
- [ ] Pricing confirmed
- [ ] Terms of service reviewed
- [ ] Privacy policy updated

---

## 10. Next Steps and Immediate Actions

### 10.1 This Week (Week 0)

| Action | Owner | Due |
|--------|-------|-----|
| Review and approve this technical plan | CEO | Day 2 |
| Finalize team composition and hiring plan | CTO + HR | Day 3 |
| Create Bill Forge directory structure | Backend Lead | Day 3 |
| Set up Cargo workspace + pnpm | Backend Lead | Day 4 |
| Initialize Docker Compose (PG, Redis, MinIO) | SRE | Day 4 |
| Create GitHub project board for Phase 0-1 | PM | Day 5 |

### 10.2 Week 1

| Action | Owner | Due |
|--------|-------|-----|
| Establish CI pipeline (GitHub Actions) | SRE | Day 7 |
| Create base Axum API with health check | Backend | Day 7 |
| Set up SeaORM with initial migrations | Backend | Day 7 |
| Scaffold Next.js app with shadcn/ui | Frontend | Day 7 |
| Development environment documentation | All | Day 7 |

### 10.3 Key Decisions Required from CEO

1. **Team Budget:** Confirm 5.5 FTE allocation for 12-week MVP
2. **Hiring Authority:** Approve senior Rust engineer search
3. **Infrastructure Budget:** Approve $500-700/month for staging/production
4. **Pilot Customers:** Identify 5 target companies for MVP pilots
5. **QuickBooks First:** Confirm first ERP integration choice

---

## Appendix A: Technology Comparison Matrix

| Requirement | Option A | Option B | Recommendation |
|-------------|----------|----------|----------------|
| **Backend Language** | Rust | Go | Rust (CEO pref, performance) |
| **Web Framework** | Axum | Actix | Axum (ergonomics) |
| **Frontend** | Next.js | Remix | Next.js (CEO pref, ecosystem) |
| **Database** | PostgreSQL | CockroachDB | PostgreSQL (maturity, cost) |
| **OCR Primary** | Tesseract | AWS Textract | Tesseract (local-first) |
| **OCR Fallback** | AWS Textract | Google Vision | AWS Textract (accuracy) |
| **Object Storage** | MinIO | S3 direct | MinIO (self-hosted option) |
| **Workflow Engine** | Custom Rust | Temporal | Custom (simpler, no infra) |

## Appendix B: Database Schema (Core Tables)

```sql
-- Control Plane (public schema)
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) UNIQUE NOT NULL,
    schema_name VARCHAR(100) UNIQUE NOT NULL,
    status VARCHAR(50) DEFAULT 'active',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Tenant Schema Template
CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    vendor_id UUID REFERENCES vendors(id),
    invoice_number VARCHAR(100),
    invoice_date DATE,
    due_date DATE,
    amount DECIMAL(15,2),
    currency VARCHAR(3) DEFAULT 'USD',
    status VARCHAR(50) DEFAULT 'pending_ocr',
    ocr_confidence DECIMAL(5,4),
    ocr_provider VARCHAR(50),
    ocr_raw_data JSONB,
    document_url TEXT,
    queue VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    description TEXT,
    quantity DECIMAL(15,4),
    unit_price DECIMAL(15,4),
    amount DECIMAL(15,2),
    gl_code VARCHAR(50),
    cost_center VARCHAR(50),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE vendors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    normalized_name VARCHAR(255),
    tax_id VARCHAR(50),
    address JSONB,
    payment_terms INTEGER DEFAULT 30,
    status VARCHAR(50) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE approval_workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    is_default BOOLEAN DEFAULT FALSE,
    rules JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE approval_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    workflow_id UUID REFERENCES approval_workflows(id),
    step_order INTEGER NOT NULL,
    approver_id UUID REFERENCES users(id),
    status VARCHAR(50) DEFAULT 'pending',
    decision_at TIMESTAMPTZ,
    comments TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Appendix C: API Endpoint Summary

```yaml
# Authentication
POST   /api/v1/auth/login          # JWT login
POST   /api/v1/auth/refresh        # Refresh token
POST   /api/v1/auth/api-keys       # Create API key

# Tenants (Platform Admin)
GET    /api/v1/tenants             # List tenants
POST   /api/v1/tenants             # Create tenant
GET    /api/v1/tenants/:id         # Get tenant
PATCH  /api/v1/tenants/:id         # Update tenant

# Invoices
POST   /api/v1/{tenant}/invoices/upload     # Upload invoice
GET    /api/v1/{tenant}/invoices            # List invoices
GET    /api/v1/{tenant}/invoices/:id        # Get invoice
PATCH  /api/v1/{tenant}/invoices/:id        # Update invoice
POST   /api/v1/{tenant}/invoices/:id/reocr  # Re-run OCR

# Queues
GET    /api/v1/{tenant}/queues/ap           # AP queue
GET    /api/v1/{tenant}/queues/error        # Error queue
POST   /api/v1/{tenant}/queues/ap/bulk      # Bulk submit

# Approvals
GET    /api/v1/{tenant}/approvals           # My pending approvals
POST   /api/v1/{tenant}/invoices/:id/approve    # Approve
POST   /api/v1/{tenant}/invoices/:id/reject     # Reject
POST   /api/v1/{tenant}/invoices/:id/delegate   # Delegate

# Email Actions (Public, signed)
GET    /api/v1/actions/:token/approve       # Approve via email
GET    /api/v1/actions/:token/reject        # Reject via email

# Vendors
GET    /api/v1/{tenant}/vendors             # List vendors
POST   /api/v1/{tenant}/vendors             # Create vendor
GET    /api/v1/{tenant}/vendors/:id         # Get vendor
PATCH  /api/v1/{tenant}/vendors/:id         # Update vendor
GET    /api/v1/{tenant}/vendors/search      # Fuzzy search

# Workflows
GET    /api/v1/{tenant}/workflows           # List workflows
POST   /api/v1/{tenant}/workflows           # Create workflow
GET    /api/v1/{tenant}/workflows/:id       # Get workflow
PATCH  /api/v1/{tenant}/workflows/:id       # Update workflow

# Analytics
GET    /api/v1/{tenant}/analytics/volume    # Invoice volume
GET    /api/v1/{tenant}/analytics/cycle     # Cycle time
GET    /api/v1/{tenant}/analytics/spend     # Spend analysis
```

---

**Document End**

*This plan is subject to revision based on CEO feedback and market learnings.*
