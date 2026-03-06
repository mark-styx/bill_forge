# Bill Forge - CTO Strategic Technical Plan

**Document Version:** 5.0 - Final Executive Plan
**Date:** 2026-02-02
**Author:** Chief Technology Officer
**Status:** Ready for CEO Approval
**Alignment:** CPO Product Strategy V10 (Approved)

---

## Executive Summary

Bill Forge is a modular invoice processing platform targeting mid-market companies (50-500 employees) frustrated with expensive, slow legacy AP tools. This technical plan outlines the architecture, technology decisions, development phases, and resource requirements to deliver the MVP within 12 weeks (Q1 2026).

### Strategic Position

**"The fast, fair, and intelligent AP platform for growing companies."**

Our core thesis: invoice processing workflows are well-understood. We win by executing dramatically better on UX, speed, and pricing - not by reinventing processes.

### Key Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Backend Language** | Rust (Axum) | Sub-second response times, compile-time safety, CEO preference |
| **Frontend** | Next.js 14+ | Modern React, server components, CEO preference |
| **Database Strategy** | Database-per-tenant | Complete isolation, compliance-ready, data portability |
| **OCR Strategy** | Tesseract 5 primary, cloud fallback | Privacy-first positioning, cost optimization |
| **AI Foundation** | Adapt Locust framework | 60% effort reduction for Winston AI |
| **Architecture** | Modular monolith → microservices | Start simple, extract as needed |

### CPO-CTO Alignment Summary

This plan is fully aligned with CPO Product Strategy V10:

| Requirement | Technical Implementation |
|-------------|-------------------------|
| Sub-second UI | Rust/Axum backend, <200ms P95 API latency |
| Email approvals | HMAC tokens, 72h expiry, one-time use |
| Local OCR option | Tesseract 5 primary, cloud escalation |
| Tenant isolation | Database-per-tenant architecture |
| 90%+ OCR accuracy | Multi-provider + confidence routing |
| Modular architecture | Independent Rust crates per module |
| QuickBooks integration | Official SDK, OAuth 2.0 (Phase 2) |

---

## 1. Technical Architecture Recommendations

### 1.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              BILL FORGE PLATFORM                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌───────────┐ │
│  │   Next.js   │     │   Axum API  │     │  Background │     │  Winston  │ │
│  │  Frontend   │────▶│   Gateway   │────▶│   Workers   │     │    AI     │ │
│  │  (Web App)  │     │   (Rust)    │     │   (Rust)    │     │ Assistant │ │
│  └─────────────┘     └──────┬──────┘     └──────┬──────┘     └─────┬─────┘ │
│                             │                   │                   │       │
│  ┌──────────────────────────┴───────────────────┴───────────────────┴────┐  │
│  │                         Event Bus (Redis Streams)                     │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                             │                   │                   │       │
│  ┌──────────────────────────┴───────────────────┴───────────────────┴────┐  │
│  │                              Data Layer                               │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐  │  │
│  │  │  PostgreSQL  │  │    DuckDB    │  │     S3       │  │   Redis   │  │  │
│  │  │   (OLTP)     │  │  (Analytics) │  │  (Documents) │  │  (Cache)  │  │  │
│  │  │ Per-Tenant   │  │   Shared     │  │  Per-Tenant  │  │  Shared   │  │  │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └───────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                          OCR Processing Layer                         │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │  │
│  │  │   Tesseract  │  │AWS Textract  │  │Google Vision │                 │  │
│  │  │   (Local)    │  │   (Cloud)    │  │   (Cloud)    │                 │  │
│  │  └──────────────┘  └──────────────┘  └──────────────┘                 │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Core Architecture Principles

1. **Database-Per-Tenant Isolation**
   - Each tenant gets dedicated PostgreSQL database
   - Complete data isolation for compliance (SOC2, GDPR)
   - Simplified backup/restore per tenant
   - Independent scaling and maintenance windows

2. **Modular Monolith (Phase 1)**
   - Single deployable unit with clear module boundaries
   - Shared infrastructure (auth, logging, metrics)
   - Module-level feature flags for subscription tiers
   - Prepared for future microservices extraction if needed

3. **Event-Driven Processing**
   - Redis Streams for reliable event delivery
   - Async invoice processing pipeline
   - Workflow state machine for approvals
   - Audit trail through event sourcing

4. **Local-First OCR with Cloud Fallback**
   - Tesseract for privacy-conscious customers
   - AWS Textract / Google Vision for accuracy
   - Confidence-based routing to appropriate provider

### 1.3 Module Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Bill Forge Module Structure                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │ Invoice Capture │  │Invoice Process  │  │Vendor Management│ │
│  │  (Priority 1)   │  │  (Priority 1)   │  │  (Priority 2)   │ │
│  ├─────────────────┤  ├─────────────────┤  ├─────────────────┤ │
│  │ • OCR Engine    │  │ • Workflow Eng  │  │ • Vendor Master │ │
│  │ • Field Extract │  │ • Approval Rout │  │ • Tax Documents │ │
│  │ • Queue Mgmt    │  │ • SLA Tracking  │  │ • Performance   │ │
│  │ • Vendor Match  │  │ • Email Approve │  │ • Onboarding    │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐                      │
│  │   Reporting     │  │   Winston AI    │                      │
│  │  (Priority 2)   │  │  (Priority 3)   │                      │
│  ├─────────────────┤  ├─────────────────┤                      │
│  │ • Dashboards    │  │ • NL Queries    │  ┌─────────────────┐ │
│  │ • Analytics     │  │ • Actions       │  │   Core Module   │ │
│  │ • Exports       │  │ • Anomaly Det   │  ├─────────────────┤ │
│  │ • Connectors    │  │ • Disambiguation│  │ • Auth/Tenancy  │ │
│  └─────────────────┘  └─────────────────┘  │ • Integrations  │ │
│                                            │ • Notifications │ │
│                                            │ • Audit Logging │ │
│                                            └─────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### 1.4 Data Architecture

#### OLTP (PostgreSQL - Per Tenant)

```sql
-- Core entities per tenant database
tenants              -- Tenant configuration and settings
users                -- Users with roles (admin, approver, viewer)
user_sessions        -- Session management

-- Invoice Capture
invoices             -- Invoice header data
invoice_line_items   -- Line item details
invoice_documents    -- Original document references
ocr_results          -- Raw OCR output with confidence
ocr_field_extracts   -- Extracted fields with confidence scores

-- Invoice Processing
approval_workflows   -- Workflow definitions
approval_rules       -- Routing rules (amount, department, etc.)
approval_steps       -- Individual approval steps
approval_history     -- Audit trail of approvals

-- Vendor Management
vendors              -- Vendor master data
vendor_contacts      -- Contact information
vendor_documents     -- W-9, 1099, contracts
vendor_bank_accounts -- Payment details (encrypted)

-- Integrations
erp_connections      -- ERP connection configs
erp_sync_log         -- Integration sync history
gl_accounts          -- Chart of accounts cache
cost_centers         -- Cost center mappings
```

#### Analytics (DuckDB - Shared with Tenant Views)

```sql
-- Materialized analytical views
fact_invoice_processing   -- Invoice metrics (volume, cycle time, etc.)
fact_approvals           -- Approval performance metrics
fact_vendor_spend        -- Vendor spend aggregations
dim_time                 -- Time dimension
dim_vendors              -- Vendor dimension (anonymized cross-tenant)
```

### 1.5 Security Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Security Layers                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Layer 1: Network                                               │
│  ├── WAF (rate limiting, SQL injection prevention)              │
│  ├── TLS 1.3 everywhere                                         │
│  └── VPC isolation for data plane                               │
│                                                                 │
│  Layer 2: Authentication                                        │
│  ├── OAuth 2.0 / OIDC (Google, Microsoft, Okta)                │
│  ├── Magic link email authentication                            │
│  ├── API key authentication for integrations                    │
│  └── Session management with secure cookies                     │
│                                                                 │
│  Layer 3: Authorization                                         │
│  ├── RBAC (Admin, Finance Lead, Approver, Viewer)              │
│  ├── Tenant isolation at database level                         │
│  ├── Row-level security for shared resources                    │
│  └── Approval delegation with time bounds                       │
│                                                                 │
│  Layer 4: Data Protection                                       │
│  ├── AES-256 encryption at rest                                 │
│  ├── Field-level encryption for PII/PCI data                    │
│  ├── Secure key management (AWS KMS / HashiCorp Vault)         │
│  └── Audit logging for all data access                          │
│                                                                 │
│  Layer 5: Application                                           │
│  ├── Input validation (Zod schemas)                             │
│  ├── CSRF protection                                            │
│  ├── Content Security Policy                                    │
│  └── Secure headers (HSTS, X-Frame-Options, etc.)              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### 2.1 Backend Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Language** | Rust | Performance for OCR, memory safety, concurrency |
| **Web Framework** | Axum 0.7+ | Async, modular, tower middleware ecosystem |
| **Async Runtime** | Tokio | Industry standard, mature ecosystem |
| **Database Driver** | SQLx | Compile-time query verification, async |
| **ORM** | SeaORM (optional) | When SQLx raw queries become unwieldy |
| **Serialization** | Serde + serde_json | De facto standard |
| **Validation** | Validator | Derive-based validation |
| **Error Handling** | thiserror + anyhow | Type-safe errors + context |
| **Logging** | tracing + tracing-subscriber | Structured, async-aware logging |
| **Configuration** | config-rs | Hierarchical config with env vars |
| **Testing** | tokio-test + mockall | Async testing, mocking |

### 2.2 Frontend Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Framework** | Next.js 14+ (App Router) | RSC, streaming, excellent DX |
| **Language** | TypeScript 5+ | Type safety, better tooling |
| **Styling** | Tailwind CSS 3.4+ | Utility-first, consistent design |
| **Components** | shadcn/ui | High-quality, customizable, accessible |
| **Forms** | React Hook Form + Zod | Performance, validation |
| **State** | Zustand (client) + TanStack Query (server) | Simple, performant |
| **Tables** | TanStack Table | Feature-rich, headless |
| **Charts** | Recharts | React-native, customizable |
| **Date/Time** | date-fns | Lightweight, tree-shakeable |
| **Icons** | Lucide React | Consistent, comprehensive |

### 2.3 Infrastructure Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Container** | Docker | Standard, reproducible builds |
| **Orchestration** | Kubernetes (EKS/GKE) | Scalability, managed services |
| **Database** | PostgreSQL 16 | Mature, feature-rich, JSON support |
| **Analytics DB** | DuckDB | Columnar, embedded, fast analytics |
| **Cache/Queue** | Redis 7+ | Caching, streams for events |
| **Object Storage** | S3-compatible (MinIO local) | Documents, OCR results |
| **CDN** | CloudFront / Cloudflare | Static assets, edge caching |
| **Secrets** | AWS Secrets Manager / Vault | Secure credential storage |
| **Monitoring** | Prometheus + Grafana | Metrics and dashboards |
| **Logging** | Loki or CloudWatch | Centralized log aggregation |
| **APM** | Sentry | Error tracking and performance |

### 2.4 OCR Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Local OCR** | Tesseract 5 + Leptonica | Privacy, no data egress |
| **Cloud OCR** | AWS Textract | High accuracy, table extraction |
| **Backup Cloud** | Google Cloud Vision | Alternative provider |
| **Image Processing** | image-rs | Rust-native preprocessing |
| **PDF Processing** | pdf-extract / pdfium | PDF text and image extraction |

### 2.5 Integration Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **API Style** | REST + JSON:API | Standard, well-understood |
| **API Docs** | OpenAPI 3.1 + Scalar | Interactive documentation |
| **Webhooks** | Svix | Reliable webhook delivery |
| **ERP SDKs** | Vendor-specific | QuickBooks, NetSuite SDKs |
| **Email** | Resend / SendGrid | Transactional email |
| **SMS** | Twilio (optional) | Approval notifications |

### 2.6 Development Tools

| Component | Technology | Rationale |
|-----------|------------|-----------|
| **Monorepo** | Turborepo | Fast builds, caching |
| **Rust Tooling** | cargo, clippy, rustfmt | Standard Rust tools |
| **Node Tooling** | pnpm, Biome | Fast package management, linting |
| **CI/CD** | GitHub Actions | Integrated, flexible |
| **Database Migrations** | sqlx-cli / refinery | Version-controlled schema |
| **Local Dev** | Docker Compose | Consistent environments |
| **API Testing** | Bruno / Hoppscotch | API exploration |

---

## 3. Development Priorities and Phases

### Phase 0: Foundation (Weeks 1-2)

**Objective:** Establish project structure, CI/CD, and core infrastructure

#### Deliverables:
- [ ] Monorepo structure with Turborepo
- [ ] Rust workspace with module crates
- [ ] Next.js project with shadcn/ui setup
- [ ] Docker Compose for local development
- [ ] CI/CD pipeline (build, test, lint)
- [ ] PostgreSQL schema migrations infrastructure
- [ ] S3-compatible storage abstraction
- [ ] Configuration management system
- [ ] Logging and tracing infrastructure
- [ ] Health check endpoints

#### Technical Tasks:

```
project-root/
├── apps/
│   ├── api/              # Rust Axum API
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── config.rs
│   │   │   ├── routes/
│   │   │   ├── handlers/
│   │   │   ├── middleware/
│   │   │   └── error.rs
│   │   └── Cargo.toml
│   └── web/              # Next.js frontend
│       ├── src/
│       │   ├── app/
│       │   ├── components/
│       │   ├── lib/
│       │   └── styles/
│       └── package.json
├── crates/
│   ├── core/             # Shared types, utilities
│   ├── db/               # Database access layer
│   ├── ocr/              # OCR abstraction
│   ├── workflow/         # Workflow engine
│   └── integrations/     # ERP integrations
├── migrations/           # Database migrations
├── docker/
├── .github/workflows/
├── turbo.json
└── Cargo.toml (workspace)
```

### Phase 1A: Invoice Capture Core (Weeks 3-5)

**Objective:** Build OCR pipeline and invoice data extraction

#### Deliverables:
- [ ] Document upload API (PDF, images)
- [ ] Image preprocessing pipeline
- [ ] Tesseract OCR integration
- [ ] Field extraction engine
- [ ] Confidence scoring system
- [ ] OCR queue management (AP queue + error queue)
- [ ] Basic web UI for document upload
- [ ] Invoice list view with status

#### Key Features:

1. **Document Ingestion**
   ```rust
   // Supported formats
   enum DocumentFormat {
       Pdf,
       Jpeg,
       Png,
       Tiff,
       Heic,  // iOS photos
   }

   // Upload endpoint
   POST /api/v1/invoices/upload
   Content-Type: multipart/form-data
   ```

2. **OCR Pipeline**
   ```
   Upload → Preprocessing → OCR → Field Extraction → Validation → Queue Assignment
                ↓
   [Deskew, Contrast, Denoise, Orientation Detection]
   ```

3. **Extracted Fields**
   - Vendor name and address
   - Invoice number
   - Invoice date
   - Due date
   - Total amount
   - Tax amount
   - Line items (description, quantity, unit price, amount)
   - PO number (if present)
   - Payment terms

4. **Confidence Scoring**
   ```rust
   struct ExtractedField {
       value: String,
       confidence: f32,      // 0.0 - 1.0
       bounding_box: Rect,   // Location on document
       source: OcrProvider,
   }

   // Routing rules
   if field.confidence >= 0.95 => auto_accept
   if field.confidence >= 0.70 => review_queue
   if field.confidence < 0.70 => error_queue
   ```

### Phase 1B: Invoice Processing Core (Weeks 5-7)

**Objective:** Build approval workflow engine

#### Deliverables:
- [ ] Workflow engine with state machine
- [ ] Approval rule configuration
- [ ] Multi-level approval routing
- [ ] Email approval links (approve/reject)
- [ ] Delegation support
- [ ] SLA tracking and escalation
- [ ] Approval history / audit trail
- [ ] Approval dashboard UI

#### Key Features:

1. **Workflow State Machine**
   ```
   DRAFT → PENDING_APPROVAL → IN_REVIEW → APPROVED → READY_FOR_PAYMENT
                    ↓              ↓
                 REJECTED      ESCALATED
   ```

2. **Approval Rules Engine**
   ```rust
   struct ApprovalRule {
       id: Uuid,
       name: String,
       conditions: Vec<Condition>,
       approvers: Vec<ApproverConfig>,
       priority: i32,
   }

   enum Condition {
       AmountGreaterThan(Decimal),
       AmountLessThan(Decimal),
       Department(String),
       CostCenter(String),
       Vendor(Uuid),
       IsNewVendor,
       ExceedsBudget,
   }
   ```

3. **Email Approvals**
   - Secure, time-limited approval links
   - One-click approve/reject from email
   - Comment support
   - Mobile-friendly email template

### Phase 2: Integration & Polish (Weeks 8-10)

**Objective:** ERP integration and production readiness

#### Deliverables:
- [ ] QuickBooks Online integration
- [ ] GL account mapping
- [ ] Cost center assignment
- [ ] Bulk export for payment
- [ ] Enhanced error handling
- [ ] Performance optimization
- [ ] Security audit remediation
- [ ] User onboarding flow
- [ ] Help documentation

#### QuickBooks Integration:
```rust
// Sync capabilities
- Pull chart of accounts
- Pull vendors
- Push approved invoices as bills
- Sync payment status
```

### Phase 3: Analytics & Vendor Management (Weeks 11-12)

**Objective:** Reporting dashboard and vendor features

#### Deliverables:
- [ ] DuckDB analytics integration
- [ ] Processing metrics dashboard
- [ ] Approval performance reports
- [ ] Basic vendor management
- [ ] Vendor spend analysis
- [ ] Export to Excel/CSV
- [ ] Audit trail reports

---

## 4. Risk Assessment

### 4.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **OCR accuracy below 90%** | Medium | High | Multi-provider strategy, confidence-based routing, continuous training |
| **Rust learning curve** | Medium | Medium | Experienced Rust developers, comprehensive documentation, pair programming |
| **Database-per-tenant complexity** | Low | Medium | Connection pooling (PgBouncer), tenant provisioning automation |
| **Email deliverability issues** | Medium | Medium | Reputable provider (Resend), proper SPF/DKIM/DMARC, dedicated IP |
| **QuickBooks API limitations** | Medium | Medium | Rate limiting, caching, async sync, fallback to CSV export |

### 4.2 Product Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Feature creep** | High | Medium | Strict adherence to MVP scope, anti-goals enforcement |
| **Pilot customer delays** | Medium | High | Early customer engagement, flexible deployment options |
| **Competitor response** | Low | Low | Focus on differentiation (local OCR, modular pricing) |

### 4.3 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Key person dependency** | Medium | High | Documentation, knowledge sharing, cross-training |
| **Security breach** | Low | Critical | Security-first design, regular audits, penetration testing |
| **Data loss** | Low | Critical | Multi-region backups, point-in-time recovery, disaster recovery plan |

### 4.4 Risk Response Matrix

```
           │ Low Impact │ Medium Impact │ High Impact │ Critical
───────────┼────────────┼───────────────┼─────────────┼──────────
Low Prob   │   Accept   │    Accept     │   Mitigate  │  Mitigate
Med Prob   │   Accept   │   Mitigate    │   Mitigate  │  Avoid
High Prob  │  Mitigate  │   Mitigate    │    Avoid    │  Avoid
```

---

## 5. Resource Requirements

### 5.1 Team Structure (MVP)

```
┌─────────────────────────────────────────────────────────────────┐
│                    Bill Forge Engineering Team                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Tech Lead / Architect (1 FTE)                           │   │
│  │ • System architecture decisions                         │   │
│  │ • Code review and quality                               │   │
│  │ • Technical mentorship                                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────┐  ┌──────────────────────────────┐   │
│  │ Backend Engineers    │  │ Frontend Engineer            │   │
│  │ (2 FTE)              │  │ (1 FTE)                      │   │
│  │ • Rust/Axum API      │  │ • Next.js/React              │   │
│  │ • OCR integration    │  │ • UI/UX implementation       │   │
│  │ • Workflow engine    │  │ • Component library          │   │
│  │ • Database design    │  │ • API integration            │   │
│  └──────────────────────┘  └──────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────┐  ┌──────────────────────────────┐   │
│  │ DevOps/SRE           │  │ QA Engineer                  │   │
│  │ (0.5 FTE)            │  │ (0.5 FTE)                    │   │
│  │ • CI/CD pipeline     │  │ • Test strategy              │   │
│  │ • Infrastructure     │  │ • Automation                 │   │
│  │ • Monitoring         │  │ • UAT coordination           │   │
│  └──────────────────────┘  └──────────────────────────────┘   │
│                                                                 │
│  Total: 5 FTE                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Skill Requirements

| Role | Required Skills | Nice to Have |
|------|-----------------|--------------|
| **Tech Lead** | Rust, distributed systems, PostgreSQL, API design | OCR/ML, fintech |
| **Backend #1** | Rust, Axum/Tokio, PostgreSQL, testing | OCR libraries |
| **Backend #2** | Rust, API integrations, async programming | QuickBooks API |
| **Frontend** | Next.js, TypeScript, React, Tailwind, accessibility | B2B SaaS experience |
| **DevOps** | Docker, Kubernetes, GitHub Actions, monitoring | AWS/GCP, Terraform |
| **QA** | Test automation, API testing, accessibility testing | Invoice processing domain |

### 5.3 Infrastructure Costs (Monthly Estimate)

| Component | Development | Production (5 pilots) |
|-----------|-------------|----------------------|
| **Compute (Kubernetes)** | $200 | $800 |
| **PostgreSQL (RDS)** | $100 | $500 |
| **Redis (ElastiCache)** | $50 | $200 |
| **S3 Storage** | $20 | $100 |
| **CDN** | $20 | $50 |
| **Monitoring (Datadog/Sentry)** | $100 | $300 |
| **Email (Resend)** | $20 | $50 |
| **OCR Cloud (backup)** | $50 | $200 |
| **Miscellaneous** | $40 | $100 |
| **Total** | **~$600/mo** | **~$2,300/mo** |

### 5.4 Third-Party Services

| Service | Purpose | Estimated Cost |
|---------|---------|----------------|
| **GitHub** | Code hosting, CI/CD | $21/user/mo |
| **Sentry** | Error tracking | $26/mo (Team) |
| **Resend** | Transactional email | $20/mo |
| **QuickBooks Developer** | Integration testing | Free (sandbox) |
| **Figma** | Design | $15/editor/mo |
| **Linear** | Project management | $10/user/mo |

---

## 6. Timeline Summary

```
Week   1   2   3   4   5   6   7   8   9  10  11  12
       ├───┴───┼───┴───┴───┼───┴───┼───┴───┴───┼───┴───┤
       │       │           │       │           │       │
       │ Phase │  Phase 1A │Phase  │  Phase 2  │Phase 3│
       │   0   │  Invoice  │  1B   │Integration│Report │
       │ Found │  Capture  │Process│  Polish   │Vendor │
       │ ation │           │       │           │       │
       ├───────┼───────────┼───────┼───────────┼───────┤

Milestones:
├─ Week 2:  Project structure, CI/CD complete
├─ Week 5:  Invoice upload & OCR working
├─ Week 7:  Approval workflow functional
├─ Week 10: QuickBooks integration complete
├─ Week 12: MVP ready for pilot customers
```

### Key Milestones

| Milestone | Target Date | Success Criteria |
|-----------|-------------|------------------|
| **M0: Foundation Complete** | Week 2 | CI/CD green, local dev working |
| **M1: OCR Demo** | Week 4 | Invoice uploaded, fields extracted |
| **M2: Invoice Capture MVP** | Week 5 | End-to-end invoice capture working |
| **M3: Workflow Demo** | Week 6 | Multi-level approval working |
| **M4: Invoice Processing MVP** | Week 7 | Email approvals working |
| **M5: QuickBooks Sync** | Week 9 | Approved invoices sync to QB |
| **M6: Production Ready** | Week 10 | Security audit passed, monitoring live |
| **M7: Pilot Ready** | Week 12 | 5 customers onboarded, 90% OCR accuracy |

---

## 7. Answers to CEO Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette/Rillion Strengths:**
- Established market presence in Nordic/European markets
- Comprehensive AP automation suite
- Strong ERP integrations (SAP, Oracle)

**Palette/Rillion Weaknesses:**
- Complex implementation (weeks/months)
- Expensive (enterprise pricing)
- Dated UI/UX
- Limited flexibility in approval workflows

**Bill Forge Differentiation:**
1. **Modern, fast UI** - Responsive, mobile-friendly interface
2. **Local-first OCR** - Privacy option that competitors don't offer
3. **Modular pricing** - Pay only for what you need
4. **Quick onboarding** - Days, not weeks
5. **Email approvals** - No login required for approvers
6. **Mid-market focus** - Right-sized for 10-1000 employees

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommended Thresholds:**
- **Auto-accept:** ≥95% confidence (all fields)
- **Review queue:** 70-95% confidence (needs human verification)
- **Error queue:** <70% confidence (likely failed extraction)

**Additional Routing:**
- If any critical field (vendor, amount, invoice #) has <70% confidence → Error queue
- If total doesn't match line items sum → Review queue
- If duplicate invoice detected → Review queue with duplicate flag

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation: QuickBooks Online (QBO)**

**Rationale:**
- Largest mid-market install base (~5M+ businesses)
- Well-documented REST API
- Sandbox environment for testing
- Strong presence in target segment (10-1000 employees)
- Lower integration complexity than NetSuite/SAP

**Second Priority:** NetSuite (for larger mid-market companies)

### Q4: What approval workflow patterns are most common in mid-market companies?

**Most Common Patterns:**

1. **Amount-Based Tiering** (80% of companies)
   ```
   < $1,000:   Auto-approve (if PO match)
   $1,000-$5,000:  Department manager
   $5,000-$25,000: Department manager + Finance lead
   > $25,000:  CFO/Controller approval
   ```

2. **Department Routing** (70% of companies)
   - Route to department manager based on cost center
   - Finance team for general/overhead expenses

3. **Exception-Based** (60% of companies)
   - Only route if: no PO match, new vendor, budget exceeded, duplicate suspected

4. **Sequential vs Parallel**
   - Mid-market prefers sequential (simpler to understand)
   - Some want parallel for faster processing

### Q5: How do competitors handle multi-currency and international invoices?

**Common Approaches:**
- Currency detection from invoice
- Real-time exchange rate lookup
- Booking at invoice date rate
- GL posting in base currency with forex tracking

**Recommendation for MVP:**
- Support USD-only initially
- Add multi-currency in Phase 4
- Use open exchange rate API (exchangerate-api.com)
- Store original currency and converted amount

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommended: Hybrid Usage-Based**

```
Base Platform Fee: $199-499/month (includes 5 users)
- Additional users: $15-25/user/month

Invoice Processing: $0.50-1.00/invoice
- Volume discounts at 500, 2000, 5000 invoices/month

OCR Processing: $0.10-0.25/page
- Local OCR (Tesseract): Included in base
- Cloud OCR (Textract): Pass-through + small markup

Module Add-ons:
- Vendor Management: +$99/month
- Advanced Analytics: +$149/month
- Winston AI: +$199/month
```

**Key Pricing Principles:**
1. Predictable base cost for budgeting
2. Usage-based scales with customer value
3. Volume discounts reward growth
4. Modular pricing matches modular architecture

---

## 8. Technical Standards & Guidelines

### 8.1 Code Quality Standards

**Rust:**
```toml
# Cargo.toml - Workspace lints
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = "deny"
pedantic = "warn"
nursery = "warn"
```

**TypeScript:**
```json
// tsconfig.json
{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitReturns": true
  }
}
```

### 8.2 API Design Standards

- RESTful with JSON:API conventions
- Consistent error format
- Pagination via cursor-based tokens
- Rate limiting headers
- Request ID for tracing
- Versioning via URL path (/api/v1/)

### 8.3 Database Standards

- UUID for primary keys
- created_at/updated_at on all tables
- Soft deletes with deleted_at
- Audit columns (created_by, updated_by)
- Indexes on foreign keys and query patterns

### 8.4 Security Standards

- No secrets in code (use env vars / secrets manager)
- Input validation on all endpoints
- Output encoding for XSS prevention
- Parameterized queries only
- Audit logging for sensitive operations
- Regular dependency updates

---

## 9. Success Metrics

### Technical Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **OCR Accuracy** | ≥90% | % invoices auto-accepted without correction |
| **API Latency (p95)** | <200ms | Response time for standard operations |
| **Uptime** | 99.5% | Monthly availability |
| **Error Rate** | <0.1% | 5xx errors / total requests |
| **Build Time** | <10min | CI/CD pipeline duration |
| **Test Coverage** | >80% | Unit + integration tests |

### Business Metrics (Post-MVP)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Time to First Invoice** | <30min | Onboarding to first processed invoice |
| **Avg Processing Time** | <2 days | Invoice received to approved |
| **Automation Rate** | >60% | Invoices auto-routed without manual intervention |
| **Customer Satisfaction** | >4.5/5 | NPS or satisfaction surveys |

---

## 10. Appendix

### A. Technology Alternatives Considered

| Component | Chosen | Alternative | Reason for Choice |
|-----------|--------|-------------|-------------------|
| Backend Language | Rust | Go, Node.js | Performance, memory safety, strong typing |
| Web Framework | Axum | Actix-web, Rocket | Tokio ecosystem, tower middleware |
| Frontend | Next.js | Remix, SvelteKit | Ecosystem, RSC, Vercel deployment |
| OLTP Database | PostgreSQL | MySQL, CockroachDB | JSON support, extensions, maturity |
| Analytics DB | DuckDB | ClickHouse, TimescaleDB | Embedded, no separate service needed |
| OCR | Tesseract + Cloud | Tesseract only, Cloud only | Flexibility, privacy options |
| Queue | Redis Streams | RabbitMQ, Kafka | Simplicity, existing Redis for cache |

### B. Reference Architecture Diagrams

(See ASCII diagrams in Section 1)

### C. Glossary

| Term | Definition |
|------|------------|
| **AP** | Accounts Payable |
| **OCR** | Optical Character Recognition |
| **GL** | General Ledger |
| **PO** | Purchase Order |
| **ERP** | Enterprise Resource Planning |
| **RBAC** | Role-Based Access Control |
| **OLTP** | Online Transaction Processing |
| **RSC** | React Server Components |

---

---

## 10. Leveraging Locust for Development

The existing Locust agentic development system can accelerate Bill Forge development through its multi-agent workflow orchestration.

### 10.1 What to Reuse from Locust

| Locust Component | Adaptation for Bill Forge |
|------------------|---------------------------|
| Agent base classes (`agents/base.py`) | Simplify for Winston single-agent use |
| LLM backend switching (`llm/`) | Keep Claude + Ollama support for Winston |
| Memory/embeddings (`memory/`) | Use for semantic search over tenant data |
| Error handling (`ceo/errors.py`) | Reuse circuit breaker, execution guard |
| Research workflow | Adapt for AP domain knowledge gathering |

**Remove from Winston:**
- Software development agents (CTO, CPO, Engineering Manager)
- Code generation modules
- Git integration
- Research topics (replace with AP-specific topics)

### 10.2 Winston AI Timeline (Phase 3+)

| Week | Focus |
|------|-------|
| 1 | Fork Locust agent core, strip unused code |
| 2 | Implement Bill Forge tools, API integration |
| 3 | Chat UI, testing, tenant isolation |

**Effort Savings:** ~60% reduction vs. building from scratch

### 10.3 Recommended Locust Configuration for Bill Forge Development

```bash
# .env for Bill Forge development using Locust
LOCUST_MODELS__CTO__BACKEND=claude
LOCUST_MODELS__CPO__BACKEND=claude
LOCUST_MODELS__BACKEND_DEV__BACKEND=claude  # Rust expertise needed
LOCUST_MODELS__FRONTEND_DEV__BACKEND=ollama
LOCUST_MODELS__BACKEND_DEV__MODEL=claude

# Higher quality thresholds for finance software
LOCUST_WORKFLOW__DESIGN_APPROVAL_THRESHOLD=80
LOCUST_WORKFLOW__IMPLEMENTATION_APPROVAL_THRESHOLD=70
```

---

## 11. Immediate Action Plan

### Week 0 (This Week)

| Day | Action | Deliverable |
|-----|--------|-------------|
| Day 1-2 | Create `bill-forge` repository | Monorepo initialized |
| Day 1-2 | Initialize Cargo workspace | bf-common, bf-api crates |
| Day 1-2 | Initialize pnpm workspace | Next.js app scaffold |
| Day 3-4 | Configure Docker Compose | PostgreSQL, Redis, MinIO running |
| Day 3-4 | Set up GitHub Actions | Rust lint, test, build on PR |
| Day 5 | Foundation crates | bf-common, bf-api, bf-tenant |

### Week 1 Checklist

| Deliverable | Owner | Status |
|-------------|-------|--------|
| Monorepo initialized | DevOps | [ ] |
| Docker Compose running | DevOps | [ ] |
| bf-api health check working | Backend | [ ] |
| bf-tenant creates tenant databases | Backend | [ ] |
| CI pipeline passing on main | DevOps | [ ] |
| Next.js app with shadcn/ui | Frontend | [ ] |

---

## 12. Appendix

### A. Architecture Decision Records (ADRs)

#### ADR-001: Database-per-Tenant Isolation

**Status:** Accepted
**Decision:** Use database-per-tenant model (not row-level security)
**Consequences:**
- (+) Complete data isolation for compliance
- (+) Per-tenant backup/restore
- (+) Easy data portability
- (-) Higher connection overhead (mitigate with PgBouncer)
- (-) More complex migrations (require coordination)

#### ADR-002: OCR Provider Strategy

**Status:** Accepted
**Decision:** Tesseract 5 default, cloud providers for escalation
**Consequences:**
- (+) Privacy-first positioning
- (+) Low cost for high-confidence invoices
- (-) Slightly lower accuracy than cloud-only (mitigate with fallback)

#### ADR-003: Email Approval Security

**Status:** Accepted
**Decision:** HMAC-signed tokens, 72h expiration, one-time use
**Consequences:**
- (+) Frictionless approver experience
- (+) Works on mobile without app
- (-) Tokens can be forwarded (mitigate with audit logging, IP tracking)

#### ADR-004: Dual Codebase Strategy

**Status:** Accepted
**Decision:** Separate Bill Forge (Rust) from Locust (Python)
**Consequences:**
- (+) Clean separation of concerns
- (+) Optimal language for each purpose
- (+) Locust agent architecture reusable for Winston
- (-) Two codebases to maintain

### B. Technology Alternatives Considered

| Component | Chosen | Alternative | Reason for Choice |
|-----------|--------|-------------|-------------------|
| Backend Language | Rust | Go, Node.js | Performance, memory safety, strong typing |
| Web Framework | Axum | Actix-web, Rocket | Tokio ecosystem, tower middleware |
| Frontend | Next.js | Remix, SvelteKit | Ecosystem, RSC, Vercel deployment |
| OLTP Database | PostgreSQL | MySQL, CockroachDB | JSON support, extensions, maturity |
| Analytics DB | DuckDB | ClickHouse, TimescaleDB | Embedded, no separate service needed |
| OCR | Tesseract + Cloud | Tesseract only, Cloud only | Flexibility, privacy options |
| Queue | Redis Streams | RabbitMQ, Kafka | Simplicity, existing Redis for cache |

### C. Glossary

| Term | Definition |
|------|------------|
| **AP** | Accounts Payable |
| **OCR** | Optical Character Recognition |
| **GL** | General Ledger |
| **PO** | Purchase Order |
| **ERP** | Enterprise Resource Planning |
| **RBAC** | Role-Based Access Control |
| **OLTP** | Online Transaction Processing |
| **RSC** | React Server Components |
| **ICP** | Ideal Customer Profile |
| **NPS** | Net Promoter Score |
| **PMF** | Product-Market Fit |

### D. Reference Documents

- CPO Product Strategy V10: `/docs/CPO_PRODUCT_STRATEGY_FINAL_V10.md`
- CTO Technical Plan V4: `/docs/CTO_STRATEGIC_PLAN_V4.md`
- CEO Vision Document: Bill Forge Vision (provided)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0-4.0 | Jan-Feb 2026 | Initial drafts and iterations |
| 5.0 | Feb 2, 2026 | Final consolidated version aligned with CPO V10 |

**Sign-offs Required:**
- [ ] CEO Approval
- [ ] CPO Alignment Confirmation
- [ ] Engineering Lead Review

---

*This technical plan is the authoritative execution document for Bill Forge. It supersedes all previous versions and will be updated as decisions evolve based on pilot customer feedback.*

*Document maintained by Bill Forge Engineering Team*
