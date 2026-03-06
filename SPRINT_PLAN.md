# Bill Forge - 12-Week MVP Sprint Plan

**Project:** Bill Forge - Intelligent AP Automation Platform
**Timeline:** 12 Weeks (Q1 2026)
**Sprint Duration:** 2 Weeks
**Total Sprints:** 6
**Last Updated:** March 5, 2026

---

## Sprint Overview

| Sprint | Weeks | Focus Area | Status | Task ID |
|--------|-------|------------|--------|---------|
| 1 | 1-2 | Foundation & Multi-Tenancy | 🟡 In Progress | #4 |
| 2 | 3-4 | Invoice Capture & OCR Integration | ⚪ Pending | #6 |
| 3 | 5-6 | Queue Management & Review UI | ⚪ Pending | #1 |
| 4 | 7-8 | Approval Workflow & Email Actions | ⚪ Pending | #3 |
| 5 | 9-10 | API Client, Dashboard & QuickBooks | ⚪ Pending | #2 |
| 6 | 11-12 | Testing, Polish & Pilot Prep | ⚪ Pending | #5 |

**Legend:** ⚪ Pending | 🟡 In Progress | 🟢 Complete | 🔴 Blocked

---

## Sprint 1: Foundation & Multi-Tenancy (Weeks 1-2) 🟡

**Status:** In Progress
**Task ID:** #4
**Dependencies:** None

### Goal
Fix architectural foundation, implement database-per-tenant pattern, add migrations

### Critical Blocker Resolution
- Migrate from single SQLite to PostgreSQL per-tenant architecture
- Implement connection pooling with tenant context
- Add database migrations framework
- Create core schema for invoices, vendors, users, tenants

### Deliverables
- [ ] PostgreSQL multi-tenant architecture implemented
- [ ] Migration system set up (using sqlx or diesel)
- [ ] Core schema migrations:
  - [ ] tenants (id, name, settings, created_at)
  - [ ] users (id, tenant_id, email, password_hash, roles, created_at)
  - [ ] invoices (full schema from domain model)
  - [ ] vendors (basic fields)
- [ ] Tenant context middleware for Axum
- [ ] Integration test: Create tenant → Create user → Verify isolation

### Success Criteria
- [ ] Can create multiple tenants with isolated databases
- [ ] Migrations run successfully on clean database
- [ ] Integration test passes proving tenant isolation
- [ ] Zero compilation warnings

### Technical Decisions Needed
1. **Database Choice:** PostgreSQL vs SQLite for multi-tenancy
   - Recommendation: PostgreSQL (better connection pooling, production-ready)
2. **Migration Tool:** sqlx vs diesel
   - Recommendation: sqlx (compile-time checked, async-native)
3. **Connection Strategy:** Connection pool per tenant vs connection on-demand
   - Recommendation: Connection pool with LRU eviction

---

## Sprint 2: Invoice Capture & OCR Integration (Weeks 3-4) ⚪

**Status:** Pending
**Task ID:** #6
**Dependencies:** Sprint 1 (database schema for invoices)

### Goal
Implement OCR pipeline with Tesseract, invoice upload workflow

### Core Value Prop
- Integrate Tesseract 5 for local OCR (key differentiator)
- Build invoice upload API + storage
- Extract invoice fields with confidence scoring
- Route to queues based on confidence

### Deliverables
- [ ] File storage service (local S3-compatible or local filesystem)
- [ ] Invoice upload endpoint (multipart/form-data)
- [ ] Tesseract OCR integration in Rust
- [ ] Field extraction logic:
  - [ ] Vendor name, invoice number, amounts, dates
  - [ ] Confidence scoring per field
  - [ ] Bounding box tracking for UI highlighting
- [ ] Queue routing logic:
  - [ ] Confidence >= 85% → AP Queue
  - [ ] Confidence 70-84% → Review Queue
  - [ ] Confidence < 70% → Error Queue
- [ ] Frontend: Upload page with drag-drop + file preview
- [ ] Integration tests for OCR pipeline

### Success Criteria
- [ ] Can upload PDF and extract fields with >= 80% accuracy
- [ ] OCR processing time < 5 seconds (P95)
- [ ] Queue routing working based on confidence
- [ ] Frontend can upload and see processing status

---

## Sprint 3: Queue Management & Review UI (Weeks 5-6) ⚪

**Status:** Pending
**Task ID:** #1
**Dependencies:** Sprint 2 (OCR pipeline)

### Goal
Build queue routing, review interface, manual correction workflow

### User Story
As an AP user, I can review low-confidence invoices and correct fields

### Deliverables
- [ ] Backend queue endpoints:
  - [ ] GET /api/queues (list all queues with counts)
  - [ ] GET /api/queues/:id/invoices (paginated)
  - [ ] PATCH /api/invoices/:id (update fields)
  - [ ] POST /api/invoices/:id/approve (move to processing)
- [ ] Queue management logic:
  - [ ] Auto-assign invoices to users
  - [ ] Track queue metrics (aging, volume)
- [ ] Frontend queue pages:
  - [ ] Queue list view with counts
  - [ ] Review queue with inline editing
  - [ ] Error queue with manual entry form
  - [ ] Confidence indicators (red/yellow/green)
- [ ] Manual correction UI:
  - [ ] Side-by-side PDF viewer + field editor
  - [ ] Field validation with Zod schemas
  - [ ] Save draft functionality
- [ ] Vendor matching service:
  - [ ] Match vendor name to master list
  - [ ] Fuzzy matching algorithm
- [ ] E2E tests: Upload → Route to queue → Edit → Approve

### Success Criteria
- [ ] All three queues (AP/Review/Error) functional
- [ ] Manual correction saves successfully
- [ ] Vendor matching working with 90%+ accuracy
- [ ] Queue page loads in < 200ms

---

## Sprint 4: Approval Workflow & Email Actions (Weeks 7-8) ⚪

**Status:** Pending
**Task ID:** #3
**Dependencies:** Sprint 3 (queue system)

### Goal
Implement approval workflow engine + email approvals (KEY DIFFERENTIATOR)

### Key Differentiator
Approvers can approve via email without logging in

### Deliverables
- [ ] Workflow engine:
  - [ ] Approval rules configuration (amount-based tiers)
  - [ ] Multi-level approval chains
  - [ ] Approval history tracking
- [ ] Approval API:
  - [ ] POST /api/invoices/:id/submit (start workflow)
  - [ ] POST /api/approvals/:id/approve
  - [ ] POST /api/approvals/:id/reject
  - [ ] POST /api/approvals/:id/hold
- [ ] Email service integration (SendGrid/Postmark):
  - [ ] Generate HMAC-secured approval tokens
  - [ ] Send approval request emails with one-click links
  - [ ] Token expiry (72 hours)
  - [ ] IP logging for audit
- [ ] Email approval endpoints:
  - [ ] GET /api/email-approval/:token (validate token)
  - [ ] POST /api/email-approval/:token/approve
  - [ ] POST /api/email-approval/:token/reject
- [ ] Frontend approval pages:
  - [ ] My approvals queue
  - [ ] Approval detail view with actions
  - [ ] Approval history timeline
- [ ] Audit trail:
  - [ ] Log all approval actions with timestamps
  - [ ] Track email link clicks
- [ ] Security tests for email tokens

### Success Criteria
- [ ] Amount-based approval routing working
- [ ] Email approvals functional (approve without login)
- [ ] HMAC tokens secure (no replay attacks)
- [ ] Audit trail complete and queryable
- [ ] Approval turnaround < 24 hours (measured in tests)

---

## Sprint 5: API Client, Dashboard & QuickBooks Integration (Weeks 9-10) ⚪

**Status:** Pending
**Task ID:** #2
**Dependencies:** Sprint 4 (approval workflow)

### Goal
Connect frontend to real APIs, build dashboard, start QuickBooks integration

### Deliverables
- [ ] API Client Implementation:
  - [ ] Type-safe client using shared types
  - [ ] Error handling with toast notifications
  - [ ] Request/response interceptors
  - [ ] Retry logic with exponential backoff
- [ ] Dashboard page:
  - [ ] Invoice volume metrics
  - [ ] Approval turnaround time
  - [ ] OCR accuracy rate
  - [ ] Queue health indicators
- [ ] QuickBooks Online integration (Phase 1):
  - [ ] OAuth 2.0 flow setup
  - [ ] App Store submission prep
  - [ ] Vendor sync from QuickBooks
  - [ ] Export approved invoices to QuickBooks
- [ ] Vendor management CRUD:
  - [ ] Create/edit vendor
  - [ ] Vendor list with search
  - [ ] Vendor detail page with invoice history
- [ ] Settings pages:
  - [ ] Tenant settings (logo, timezone)
  - [ ] Approval rules configuration
  - [ ] User management
- [ ] Performance optimization:
  - [ ] React Query caching strategies
  - [ ] Optimistic updates
  - [ ] Debounced search
- [ ] Accessibility audit + fixes

### Success Criteria
- [ ] All frontend pages use real API (no mock data)
- [ ] Dashboard shows live metrics
- [ ] QuickBooks OAuth flow working
- [ ] Lighthouse accessibility score >= 90

---

## Sprint 6: Testing, Polish & Pilot Prep (Weeks 11-12) ⚪

**Status:** Pending
**Task ID:** #5
**Dependencies:** All previous sprints

### Goal
Comprehensive testing, bug fixes, documentation, pilot customer onboarding prep

### Deliverables
- [ ] Comprehensive Test Suite:
  - [ ] Backend: Unit tests for all crates (target 80% coverage)
  - [ ] Backend: Integration tests for all API endpoints
  - [ ] Backend: Multi-tenant isolation tests
  - [ ] Frontend: Component tests for all pages
  - [ ] E2E: Critical path tests (upload → approve → export)
- [ ] CI/CD Pipeline:
  - [ ] GitHub Actions workflow
  - [ ] Run tests on every PR
  - [ ] Build Docker images
  - [ ] Deploy to staging automatically
- [ ] Monitoring & Observability:
  - [ ] Structured logging (JSON format)
  - [ ] Error tracking (Sentry integration)
  - [ ] Performance monitoring
  - [ ] Alerting rules (OCR failures, queue depth)
- [ ] Documentation:
  - [ ] API documentation (OpenAPI/Swagger)
  - [ ] Developer setup guide
  - [ ] Architecture decision records (ADRs)
  - [ ] Runbook for incidents
- [ ] Performance Testing:
  - [ ] Load test OCR pipeline (100 concurrent uploads)
  - [ ] Database query optimization
  - [ ] Frontend bundle size optimization
- [ ] Security Hardening:
  - [ ] Dependency audit (cargo audit, npm audit)
  - [ ] Rate limiting validation
  - [ ] Input sanitization review
  - [ ] Security headers verification
- [ ] Pilot Customer Prep:
  - [ ] Sandbox environment with seed data
  - [ ] Onboarding playbook
  - [ ] Demo script
  - [ ] Pilot agreement template

### Success Criteria
- [ ] All tests passing (unit + integration + E2E)
- [ ] Test coverage >= 80%
- [ ] CI/CD pipeline green
- [ ] Documentation complete and reviewed
- [ ] Sandbox environment ready for pilots
- [ ] Zero P0/P1 bugs
- [ ] System uptime >= 99.5% in staging

---

## Sprint Execution Guidelines

### Daily Standup Questions
1. What did I complete yesterday?
2. What am I working on today?
3. Any blockers or dependencies?

### Sprint Ceremonies
- **Sprint Planning:** Day 1 of each sprint (2 hours)
- **Daily Standup:** 15 minutes each morning
- **Sprint Review:** Day 10 of each sprint (1 hour demo)
- **Sprint Retrospective:** Day 10 of each sprint (30 minutes)

### Definition of Done
A feature is "done" when:
- [ ] Code implemented and reviewed
- [ ] Unit tests written and passing
- [ ] Integration tests written and passing
- [ ] Documentation updated
- [ ] No P0/P1 bugs
- [ ] Merged to main branch

### Risk Mitigation

#### Sprint 1 Risks
- **Risk:** PostgreSQL migration takes longer than expected
- **Mitigation:** Timebox to 3 days, fall back to SQLite if blocked

#### Sprint 2 Risks
- **Risk:** Tesseract accuracy < 80%
- **Mitigation:** Have AWS Textract as fallback provider

#### Sprint 4 Risks
- **Risk:** Email delivery delays/spam filters
- **Mitigation:** Test with multiple providers (SendGrid + Postmark)

#### General Risks
- **Scope Creep:** Strictly enforce "Phase 2" label for non-MVP features
- **Technical Debt:** Allocate 20% of each sprint to refactoring/tests

---

## Progress Tracking

### Velocity Tracking
| Sprint | Planned Points | Completed Points | Velocity |
|--------|---------------|------------------|----------|
| 1 | - | - | - |
| 2 | - | - | - |
| 3 | - | - | - |
| 4 | - | - | - |
| 5 | - | - | - |
| 6 | - | - | - |

### Key Metrics
- **Overall Completion:** 0% (0/6 sprints)
- **Critical Path Status:** Sprint 1 in progress
- **Estimated Ship Date:** Week 12 (May 28, 2026)

---

## Notes
- All sprints build incrementally on previous work
- Each sprint has a demo-able deliverable
- Email approvals (Sprint 4) is the key differentiator - prioritize highly
- QuickBooks integration (Sprint 5) unlocks 70% of target market

**Next Action:** Begin Sprint 1 implementation - database architecture migration
