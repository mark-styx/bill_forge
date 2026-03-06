# Bill Forge - CTO Strategic Technical Plan

**Version:** 1.0
**Date:** February 2026
**Author:** CTO, Bill Forge
**Status:** Approved for Execution

---

## Executive Summary

Bill Forge is a modular B2B SaaS platform for mid-market invoice processing automation. Based on comprehensive codebase analysis, the platform is approximately **35% complete** with solid architectural foundations but critical gaps in database layer, workflow execution, and frontend integration.

**Key Finding:** The architecture is sound and supports the vision. The constraint is execution velocity on core features, not architectural refactoring.

**Recommendation:** Focus the next 3 months on completing the Invoice Capture and Invoice Processing modules to production quality, with 5 pilot customers validating product-market fit before expanding scope.

---

## 1. Technical Architecture Recommendations

### Current State Assessment

| Component | Status | Assessment |
|-----------|--------|------------|
| **Backend (Rust/Axum)** | 65% | Well-structured modular crates with trait-based dependency injection |
| **Frontend (Next.js)** | 50% | Pages exist but not connected to real APIs |
| **Database Layer** | 40% | Schema exists in code but not migrated; **synchronous driver is a blocker** |
| **OCR Integration** | 50% | Tesseract works; cloud providers stubbed only |
| **Workflow Engine** | 60% | Rule evaluation works; approval execution missing |
| **Multi-Tenancy** | 75% | Database-per-tenant architecture implemented |

### Architecture Decisions to Affirm

These architectural choices are correct and should be maintained:

1. **Database-per-Tenant Isolation** ✅
   - Complete data isolation at database level
   - Supports data residency requirements
   - Enables per-tenant backup/restore
   - *Trade-off accepted:* No cross-tenant transactions (acceptable for AP domain)

2. **Modular Crate Architecture** ✅
   - Clean separation of concerns
   - Each module independently testable
   - Supports module-based subscription model
   - Enables parallel development

3. **Trait-Based Abstractions** ✅
   - `OcrService`, `StorageService`, `*Repository` traits
   - Enables provider switching (Tesseract → Textract)
   - Facilitates testing with mocks

4. **Local-First Development** ✅
   - SQLite for development, production flexibility
   - S3-compatible storage abstraction
   - Supports air-gapped deployments if needed

### Architecture Changes Required

#### Critical: Migrate to Async Database Driver

**Current State:** Using `rusqlite` (synchronous) which blocks the Tokio runtime.

**Problem:** Cannot handle concurrent requests efficiently. Under load, the application will become unresponsive.

**Solution:**
```rust
// Before (synchronous, blocking)
rusqlite::Connection

// After (async, non-blocking)
sqlx::SqlitePool  // or tokio-rusqlite for minimal changes
```

**Recommendation:** Use `sqlx` with SQLite backend for:
- Compile-time verified queries
- Built-in migration system
- Async connection pooling
- Production-ready PostgreSQL migration path

**Effort:** 2-3 weeks, affects `db`, `auth`, `invoice-capture`, `invoice-processing` crates

#### Required: Implement Migration System

**Current State:** `MigrationRunner` exists but no actual SQL migrations.

**Problem:** Cannot deploy schema changes. Schema exists only in Rust structs.

**Solution:** Implement sqlx migrations:
```
backend/migrations/
├── 20260201000001_create_metadata_schema.sql
├── 20260201000002_create_tenant_schema.sql
├── 20260201000003_create_workflow_tables.sql
└── ...
```

**Effort:** 1 week for migration infrastructure + 1 week for all schemas

#### Required: Complete Approval Workflow Execution

**Current State:** Rules can be evaluated but approval requests are never created.

**Missing Pieces:**
- `ApprovalRequest` creation when invoice matches approval rule
- Approval queue persistence
- Email trigger integration
- Status transitions on approval/rejection

**Effort:** 2 weeks for core implementation + 1 week for email integration

### Target Architecture (3-Month Horizon)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Load Balancer                            │
└─────────────────────────────────────────────────────────────────┘
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
        ┌───────────────────┐   ┌───────────────────┐
        │   API Server 1    │   │   API Server 2    │
        │   (Rust/Axum)     │   │   (Rust/Axum)     │
        └─────────┬─────────┘   └─────────┬─────────┘
                  │                       │
                  └───────────┬───────────┘
                              ▼
        ┌─────────────────────────────────────────────┐
        │              Shared Services                 │
        ├─────────────┬─────────────┬─────────────────┤
        │ Auth (JWT)  │ OCR Queue   │ Email Service   │
        └──────┬──────┴──────┬──────┴────────┬────────┘
               │             │               │
        ┌──────┴──────┐ ┌────┴────┐   ┌──────┴──────┐
        │ Metadata DB │ │   S3    │   │ SMTP/SES    │
        │  (SQLite)   │ │ Storage │   │             │
        └─────────────┘ └─────────┘   └─────────────┘
               │
    ┌──────────┴──────────────────────────────┐
    │         Tenant Databases                 │
    │  ┌─────────┐ ┌─────────┐ ┌─────────┐    │
    │  │Tenant 1 │ │Tenant 2 │ │Tenant N │    │
    │  │ SQLite  │ │ SQLite  │ │ SQLite  │    │
    │  └─────────┘ └─────────┘ └─────────┘    │
    └─────────────────────────────────────────┘
```

---

## 2. Technology Stack Decisions

### Affirmed Stack (No Changes)

| Layer | Technology | Rationale |
|-------|------------|-----------|
| **Backend Framework** | Rust + Axum | Performance, safety, existing investment |
| **Frontend Framework** | Next.js 14 + App Router | SSR, file-based routing, React ecosystem |
| **UI Components** | Tailwind CSS + shadcn/ui | Rapid development, consistent design |
| **State Management** | Zustand + React Query | Simple, performant, server state handling |
| **Auth** | JWT with Argon2 | Stateless, secure password hashing |
| **OCR (Dev)** | Tesseract | Local-first, no cloud dependency |
| **Storage Abstraction** | S3-compatible API | Vendor-neutral, MinIO for dev |

### Required Changes

| Current | Change To | Rationale |
|---------|-----------|-----------|
| rusqlite (sync) | **sqlx** (async) | Non-blocking I/O, connection pooling |
| No migrations | **sqlx migrate** | Schema version control, CI/CD compatible |
| No cloud OCR | **AWS Textract** | 85-95% accuracy, table extraction, async |
| No email | **AWS SES** | Transactional email for approvals |
| No monitoring | **OpenTelemetry** | Distributed tracing, metrics, logs |

### Future Additions (Post-MVP)

| Technology | Use Case | Timeline |
|------------|----------|----------|
| DuckDB | Analytics engine for reporting | Month 3 |
| Redis | Session cache, rate limiting | Month 3 |
| Temporal.io | Workflow orchestration at scale | Month 4+ |
| PostgreSQL | Production tenant databases (optional) | Month 6+ |

### Technology Selection Criteria

For any new technology additions, evaluate:

1. **Operational Simplicity** - Can a small team run it?
2. **Rust Ecosystem Support** - Quality of Rust clients/SDKs
3. **Local Development** - Can it run locally without cloud access?
4. **Cost at Scale** - Pricing model alignment with usage-based business

---

## 3. Development Priorities and Phases

### Phase 1: Foundation & Core MVP (Weeks 1-4)

**Goal:** Complete end-to-end invoice processing workflow

**Week 1-2: Database Layer**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| Migrate rusqlite → sqlx | Backend | 5d | P0 |
| Implement migration system | Backend | 3d | P0 |
| Create all schema migrations | Backend | 3d | P0 |
| Connection pool configuration | Backend | 1d | P0 |
| Multi-tenant isolation tests | Backend | 2d | P0 |

**Week 2-3: Approval Workflow**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| ApprovalRequest creation | Backend | 2d | P0 |
| Queue assignment logic | Backend | 2d | P0 |
| Email service integration | Backend | 3d | P0 |
| Email approval endpoints | Backend | 2d | P0 |
| Approval status transitions | Backend | 2d | P0 |

**Week 3-4: Frontend Integration**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| Invoice list + detail pages | Frontend | 3d | P0 |
| Upload flow with progress | Frontend | 2d | P0 |
| Approval queue UI | Frontend | 3d | P0 |
| Field correction UI | Frontend | 3d | P0 |
| E2E test suite | QA | 3d | P0 |

**Phase 1 Exit Criteria:**
- [ ] Invoice upload → OCR → approval → payment-ready (end-to-end working)
- [ ] 70%+ test coverage on critical paths
- [ ] Multi-tenant isolation validated
- [ ] Async database driver deployed
- [ ] Email approvals functional

### Phase 2: Production Hardening (Weeks 5-8)

**Goal:** Production-ready for pilot customers

**Week 5-6: OCR Enhancement**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| AWS Textract implementation | Backend | 4d | P1 |
| OCR provider fallback logic | Backend | 2d | P1 |
| Confidence-based routing | Backend | 2d | P1 |
| Line item extraction | Backend | 3d | P1 |
| Vendor name matching | Backend | 2d | P1 |

**Week 6-7: Observability & Security**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| OpenTelemetry integration | Backend | 3d | P1 |
| Structured logging | Backend | 2d | P1 |
| Rate limiting middleware | Backend | 2d | P1 |
| Security audit (OWASP) | Security | 3d | P1 |
| Error tracking (Sentry) | DevOps | 1d | P1 |

**Week 7-8: Vendor Management**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| Vendor CRUD operations | Backend | 3d | P1 |
| Contact management | Backend | 2d | P1 |
| Tax document storage | Backend | 2d | P1 |
| Vendor list UI | Frontend | 2d | P1 |
| Vendor detail page | Frontend | 2d | P1 |

**Phase 2 Exit Criteria:**
- [ ] AWS Textract achieving 90%+ accuracy on standard invoices
- [ ] Full observability stack deployed
- [ ] Vendor management functional
- [ ] Security audit passed
- [ ] Ready for 5 pilot customers

### Phase 3: Market Differentiation (Weeks 9-12)

**Goal:** Competitive features for market positioning

**Week 9-10: Analytics & Reporting**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| DuckDB integration | Backend | 3d | P2 |
| Invoice processing metrics | Backend | 3d | P2 |
| Approval performance reports | Backend | 3d | P2 |
| Dashboard UI | Frontend | 4d | P2 |
| Export functionality | Backend | 2d | P2 |

**Week 10-11: ERP Integration**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| Integration framework | Backend | 3d | P2 |
| QuickBooks Online connector | Backend | 5d | P2 |
| Payment batch export | Backend | 3d | P2 |
| Integration settings UI | Frontend | 2d | P2 |

**Week 11-12: Polish & Scale Prep**
| Task | Owner | Effort | Priority |
|------|-------|--------|----------|
| Performance optimization | Backend | 3d | P2 |
| UI/UX refinements | Frontend | 5d | P2 |
| Documentation | Tech Writing | 3d | P2 |
| Load testing | QA | 2d | P2 |
| Pilot customer onboarding | Customer Success | ongoing | P1 |

**Phase 3 Exit Criteria:**
- [ ] 5 pilot customers actively using platform
- [ ] 90%+ OCR accuracy validated
- [ ] QuickBooks integration functional
- [ ] Dashboard with key KPIs
- [ ] Product-market fit validated

---

## 4. Risk Assessment

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Async DB migration breaks functionality** | Medium | High | Incremental migration with feature flags; comprehensive test coverage before migration |
| **Tesseract accuracy insufficient** | High | Medium | Implement Textract early in Phase 2; maintain Tesseract as fallback |
| **Multi-tenant data leak** | Low | Critical | Isolation tests in CI; security audit; database-level enforcement |
| **Email delivery issues** | Medium | Medium | Use SES with proper DKIM/SPF; implement retry logic; monitor bounce rates |
| **Schema migration failures** | Medium | High | Test migrations on production-like data; implement rollback procedures |

### Business Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Pilot customer churn** | Medium | High | Weekly check-ins; fast bug fix SLA; feature prioritization based on feedback |
| **Competitor feature parity** | Medium | Medium | Focus on differentiators (local-first OCR, modular pricing, simplicity) |
| **OCR cost overruns** | Medium | Medium | Usage-based pricing passes cost to customers; implement cost monitoring |
| **Scope creep to enterprise** | High | Medium | Strict adherence to anti-goals; defer SSO, complex integrations |

### Dependency Risks

| Dependency | Risk Level | Contingency |
|------------|------------|-------------|
| AWS Textract | Low | Google Vision as alternative; Tesseract as fallback |
| AWS SES | Low | SendGrid, Mailgun as alternatives |
| Rust ecosystem | Low | Well-established for production use |
| Next.js | Low | Large community, active maintenance |

### Risk Monitoring

Implement weekly risk review with these metrics:
- OCR accuracy rate (target: 90%+)
- System uptime (target: 99.5%)
- Error rate by module
- Customer support ticket volume
- Feature completion velocity

---

## 5. Resource Requirements

### Team Structure (Minimum Viable)

| Role | Count | Responsibility |
|------|-------|----------------|
| **Backend Engineer (Rust)** | 2 | Database migration, workflow engine, integrations |
| **Frontend Engineer** | 1-2 | Next.js pages, UI components, API integration |
| **DevOps/Platform** | 0.5 | CI/CD, infrastructure, monitoring |
| **QA Engineer** | 0.5-1 | Test automation, manual testing, security |
| **Product Manager** | 1 | Requirements, prioritization, pilot management |

**Note:** Roles can be combined for a smaller team. Minimum effective team size: 3 engineers + 1 PM.

### Infrastructure Requirements

**Development Environment:**
- Local SQLite databases
- MinIO for S3-compatible storage
- Tesseract OCR locally installed
- Docker Compose for local services

**Staging Environment:**
- Single VM or small Kubernetes cluster
- AWS Textract enabled
- AWS SES for email
- S3 for storage
- CloudWatch for logging

**Production Environment (Phase 2+):**
| Service | Specification | Est. Monthly Cost |
|---------|---------------|-------------------|
| Compute (ECS/EKS) | 2x t3.medium | $80-150 |
| RDS/SQLite hosting | db.t3.small | $30-50 |
| S3 Storage | 100GB | $3 |
| Textract | 10K pages/month | $150-300 |
| SES | 50K emails/month | $5 |
| CloudWatch | Standard logging | $20-50 |
| **Total** | | **$300-600/month** |

### Tool Requirements

| Category | Tool | Purpose |
|----------|------|---------|
| CI/CD | GitHub Actions | Build, test, deploy |
| Monitoring | Grafana Cloud or DataDog | Metrics, tracing, logs |
| Error Tracking | Sentry | Exception monitoring |
| Documentation | Notion or Confluence | Internal docs |
| Support | Intercom or Zendesk | Customer communication |

---

## 6. Success Metrics

### Phase 1 Metrics (Weeks 1-4)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Core workflow completion | 100% | Invoice upload → approval → payment ready |
| Test coverage | 70%+ | Jest/Vitest + Rust tests |
| CI build time | <10 min | GitHub Actions |
| P0 bug count | 0 | Issue tracker |

### Phase 2 Metrics (Weeks 5-8)

| Metric | Target | Measurement |
|--------|--------|-------------|
| OCR accuracy | 90%+ | Sample invoice benchmark |
| System uptime | 99%+ | Monitoring |
| API response time (p95) | <500ms | APM |
| Security vulnerabilities | 0 critical/high | Security scan |

### Phase 3 Metrics (Weeks 9-12)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Pilot customers | 5 | Active usage |
| Invoice processing volume | 1000+/month | Analytics |
| Customer satisfaction | >4.0/5 | Survey |
| Feature completion | 90% of MVP | Roadmap tracking |

### Business KPIs (Ongoing)

| KPI | Target | Timeframe |
|-----|--------|-----------|
| Time to process invoice | <5 min average | Month 3 |
| Automation rate | 60%+ auto-approved | Month 3 |
| Customer acquisition cost | <$500 | Month 6 |
| Monthly recurring revenue | $10K+ | Month 6 |

---

## 7. Leveraging the Existing Codebase

### What to Keep (High-Quality Components)

1. **Modular crate architecture** - Clean separation, maintain this structure
2. **Trait-based abstractions** - `OcrService`, `StorageService`, `*Repository` patterns
3. **JWT authentication** - Working implementation in `auth` crate
4. **Workflow rule evaluation** - Core logic in `invoice-processing` is sound
5. **Frontend component library** - shadcn/ui components are well-integrated
6. **Type-safe ID patterns** - Newtype wrappers prevent ID confusion

### What to Refactor

| Component | Issue | Refactor Plan |
|-----------|-------|---------------|
| Database layer (`db/`) | Synchronous rusqlite | Migrate to sqlx with async pools |
| Tesseract OCR | Regex-based extraction | Improve patterns, add structured extraction |
| API integration tests | Placeholder only | Implement comprehensive suite |
| Frontend pages | UI scaffolds, no API calls | Wire to real backend endpoints |

### What to Delete

- Remove unused placeholder code (empty test files, commented-out code)
- Clean up duplicate type definitions between crates
- Remove mock data that's no longer needed after API integration

### Improvement Priorities

1. **Add missing error handling** - Some functions use `unwrap()` instead of proper error propagation
2. **Implement proper logging** - Replace `println!` with structured `tracing` calls
3. **Add request correlation** - Trace IDs through the entire request lifecycle
4. **Improve type safety** - Use more specific types (e.g., `Email`, `Amount`) instead of primitives

---

## 8. Answering CEO's Questions

### Q1: What are Palette/Rillion's main strengths and weaknesses? How do we differentiate?

**Palette/Rillion Strengths:**
- Established market presence in Europe
- Strong AP automation features
- Enterprise integrations

**Palette/Rillion Weaknesses:**
- Complex, enterprise-focused pricing
- Slow, legacy user interface
- Rigid workflows

**Bill Forge Differentiation:**
1. **Modular pricing** - Buy only what you need vs. monolithic suites
2. **Local-first OCR** - Data privacy option competitors lack
3. **Modern UX** - Fast, clean interface vs. legacy enterprise tools
4. **Mid-market focus** - Right-sized for 10-1000 employee companies
5. **Email approvals** - Approve without logging in

### Q2: What's the ideal OCR accuracy threshold before routing to error queue?

**Recommendation:** 85% confidence threshold

- **≥85% confidence** → Auto-process with optional human review
- **70-84% confidence** → Route to review queue for verification
- **<70% confidence** → Route to error queue for manual entry

**Rationale:** Based on industry benchmarks, 85% captures most well-formatted invoices. Lower threshold catches poor scans, unusual formats, and damaged documents.

**Implementation:** Make threshold configurable per tenant (some may prefer 90% for critical vendors).

### Q3: Which ERP integration should we prioritize first for mid-market?

**Recommendation:** QuickBooks Online first, then NetSuite

**Rationale:**
| ERP | Mid-Market Usage | API Quality | Integration Effort |
|-----|------------------|-------------|-------------------|
| QuickBooks Online | Very High | Excellent REST API | 2-3 weeks |
| NetSuite | High | Good REST API | 3-4 weeks |
| Sage | Medium | Fair API | 4-5 weeks |
| Dynamics 365 | Medium | Good API | 4-5 weeks |

QuickBooks Online covers the largest segment of our target market (10-200 employees) and has the most developer-friendly API.

### Q4: What approval workflow patterns are most common in mid-market companies?

**Top 3 Patterns (implement in order):**

1. **Amount-based escalation** (most common)
   - <$1K: Auto-approve
   - $1K-$10K: Manager approval
   - $10K-$50K: Finance director
   - >$50K: CFO/Controller

2. **Department routing**
   - Invoice assigned to department based on cost center
   - Department head approves within budget
   - Finance reviews all before payment

3. **Vendor-based rules**
   - Pre-approved vendors: Auto-approve up to threshold
   - New vendors: Additional review required
   - High-risk categories: Always require dual approval

### Q5: How do competitors handle multi-currency and international invoices?

**Industry Standard Approach:**
- Currency detection from invoice OCR
- Real-time exchange rate lookup (via API like Open Exchange Rates)
- Store in original currency + converted amount
- Reporting in tenant's base currency

**Recommendation for Bill Forge:**
- **Phase 1 (MVP):** Support USD only, store currency code for future
- **Phase 2:** Add multi-currency with manual rate entry
- **Phase 3:** Integrate real-time exchange rate API

This matches typical mid-market needs—most start domestic, expand later.

### Q6: What's the pricing model that resonates with mid-market buyers?

**Recommended: Tiered Usage-Based Pricing**

| Tier | Monthly Fee | Included | Overage |
|------|-------------|----------|---------|
| Starter | $299/month | 500 invoices | $0.50/invoice |
| Growth | $699/month | 2,000 invoices | $0.35/invoice |
| Scale | $1,499/month | 5,000 invoices | $0.25/invoice |

**Why This Model:**
1. **Predictable base** - CFOs prefer known minimums for budgeting
2. **Usage alignment** - Cost scales with value delivered
3. **No per-seat fees** - Differentiator from Coupa/SAP
4. **Module add-ons** - Reporting: +$199/mo, Winston AI: +$299/mo

**Competitor Comparison:**
- BILL.com: Per-user + per-transaction (complex)
- Tipalti: Per-payee + volume (unpredictable)
- AvidXchange: Per-invoice + implementation fee (enterprise)

Bill Forge's simpler model appeals to mid-market buyers frustrated with enterprise complexity.

---

## Appendix A: Technical Debt Register

| ID | Debt Item | Priority | Estimated Effort | Phase |
|----|-----------|----------|------------------|-------|
| TD-001 | Synchronous database driver | Critical | 2-3 weeks | 1 |
| TD-002 | No database migrations | Critical | 1 week | 1 |
| TD-003 | Low test coverage (3%) | High | Ongoing | 1-2 |
| TD-004 | Frontend not connected to APIs | High | 2 weeks | 1 |
| TD-005 | No structured logging | Medium | 1 week | 2 |
| TD-006 | Missing rate limiting | Medium | 2 days | 2 |
| TD-007 | No distributed tracing | Medium | 3 days | 2 |
| TD-008 | Tesseract regex extraction brittle | Medium | 1 week | 2 |
| TD-009 | No error tracking integration | Low | 1 day | 2 |
| TD-010 | Missing API documentation | Low | 1 week | 3 |

## Appendix B: Module Completion Status

```
Invoice Capture    [████████░░░░░░░░░░░░] 40%
Invoice Processing [████████████░░░░░░░░] 60%
Vendor Management  [████████░░░░░░░░░░░░] 40%
Reporting          [████░░░░░░░░░░░░░░░░] 20%
Auth               [████████████████░░░░] 80%
Database Layer     [████████████░░░░░░░░] 60%
Frontend           [██████████░░░░░░░░░░] 50%
DevOps/CI          [████████░░░░░░░░░░░░] 40%
```

## Appendix C: Decision Log

| Date | Decision | Rationale | Alternatives Considered |
|------|----------|-----------|------------------------|
| 2026-02-01 | Use sqlx over tokio-rusqlite | Compile-time query checking, built-in migrations | tokio-rusqlite (less features), sea-orm (heavier) |
| 2026-02-01 | AWS Textract for production OCR | Best accuracy, table extraction, async API | Google Vision (similar), Azure (less Rust support) |
| 2026-02-01 | QuickBooks first ERP integration | Largest mid-market usage, best API | NetSuite (second priority), Sage (lower priority) |
| 2026-02-01 | 85% OCR confidence threshold | Industry benchmark, configurable per tenant | 90% (too strict), 80% (too lenient) |
| 2026-02-01 | Maintain SQLite for MVP | Simpler ops, good enough for pilot scale | PostgreSQL (premature for pilot) |

---

**Document Approval:**

- [ ] CTO Review Complete
- [ ] CEO Alignment
- [ ] Engineering Lead Review
- [ ] Product Manager Review

**Next Steps:**
1. Share with engineering team
2. Create sprint tickets from Phase 1 tasks
3. Set up weekly progress reviews
4. Begin database migration work

---

*This document will be updated as decisions evolve and new information emerges.*
