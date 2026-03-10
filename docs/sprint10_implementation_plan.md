# Sprint 10: AI Integration & Customer Insights - Implementation Plan

**Status:** 🟡 Planning
**Sprint Duration:** Weeks 19-20 (March 10 - March 23, 2026)
**Branch:** sprint10-ai-customer-insights
**Task ID:** #10

---

## Sprint Goals

Transition from post-pilot to AI-enhanced platform with deep customer insights and intelligent automation.

### Strategic Context
- ✅ Sprint 1-6: MVP complete (OCR, workflows, approvals, QuickBooks)
- ✅ Sprint 7: Production deployment & pilot
- ✅ Sprint 8: Post-pilot optimization (scaling, DR, performance)
- ✅ Sprint 9: Growth & scale (chaos engineering, analytics, feedback)
- 🎯 **Sprint 10**: AI integration & customer insights

---

## P0 - Must Have (3-4 days)

### 1. Winston AI Foundation (2 days)
**Effort:** 2 days
**Priority:** P0 - Key differentiator

**Goal:** Adapt Locust's LangGraph agent framework for invoice assistance

**Components:**
- [ ] **Agent Framework Setup**
  - Install LangGraph dependencies
  - Create agent service crate: `backend/crates/ai-agent/`
  - Configure OpenAI/Claude API integration
  - Set up conversation memory (Redis-backed)

- [ ] **Core Agent Capabilities**
  - Invoice status queries ("What's the status of invoice #1234?")
  - Vendor queries ("Show me all invoices from Acme Corp")
  - Approval assistance ("Who needs to approve this $5,000 invoice?")
  - Summary generation ("Summarize this invoice")

- [ ] **API Integration**
  - POST `/api/ai/chat` - Send message to AI agent
  - GET `/api/ai/conversations` - List conversation history
  - POST `/api/ai/conversations/:id/messages` - Continue conversation

- [ ] **Context Injection**
  - Inject tenant context, user permissions
  - Load recent invoices, approval history
  - Provide schema documentation to LLM

**Files to Create:**
- `backend/crates/ai-agent/Cargo.toml`
- `backend/crates/ai-agent/src/lib.rs`
- `backend/crates/ai-agent/src/agent.rs` - LangGraph agent implementation
- `backend/crates/ai-agent/src/context.rs` - Context injection
- `backend/crates/ai-agent/src/tools.rs` - Invoice query tools
- `backend/crates/ai-agent/src/handlers.rs` - HTTP handlers
- `backend/migrations/012_create_ai_conversations_table/up.sql`

**Dependencies:**
- `langchain-rust` or direct OpenAI/Anthropic SDK
- Redis for conversation memory

---

### 2. Customer Health Scoring (1 day)
**Effort:** 1 day
**Priority:** P0 - Churn prevention

**Goal:** Automatically identify at-risk customers and power users

**Components:**
- [ ] **Health Score Model**
  - Usage frequency (daily/weekly active users)
  - Feature adoption (OCR, approvals, QuickBooks sync)
  - Error rates (OCR failures, workflow failures)
  - Support tickets (from feedback module)
  - Payment metrics (on-time payments, subscription status)

- [ ] **Scoring Algorithm**
  ```
  health_score = (
    usage_score * 0.3 +
    feature_adoption_score * 0.25 +
    error_rate_score * 0.2 +
    sentiment_score * 0.15 +
    payment_score * 0.1
  )
  ```

- [ ] **Risk Classification**
  - 🔴 At Risk (score < 50): Immediate intervention
  - 🟡 Needs Attention (50-70): Proactive outreach
  - 🟢 Healthy (70+): Monitor only

- [ ] **API Endpoints**
  - GET `/api/admin/tenants/health` - List all tenant health scores
  - GET `/api/admin/tenants/:id/health` - Detailed health breakdown
  - POST `/api/admin/tenants/:id/health/refresh` - Recalculate score

- [ ] **Background Job**
  - Daily health score calculation (reuse worker crate)
  - Alert on score drops > 20 points
  - Store historical scores for trend analysis

**Files to Create:**
- `backend/crates/health/src/lib.rs`
- `backend/crates/health/src/models.rs` - HealthScore model
- `backend/crates/health/src/scoring.rs` - Scoring algorithm
- `backend/crates/health/src/repository.rs` - Database queries
- `backend/crates/health/src/handlers.rs` - HTTP handlers
- `backend/migrations/013_create_health_scores_table/up.sql`

---

### 3. Intelligent Invoice Categorization (1 day)
**Effort:** 1 day
**Priority:** P0 - Automation improvement

**Goal:** Auto-categorize invoices by department, cost center, GL code

**Components:**
- [ ] **Rule-Based Categorization** (Phase 1)
  - Vendor → Department mapping (e.g., "Office Depot" → Office Supplies)
  - Amount thresholds (e.g., < $100 → Auto-approve category)
  - Historical patterns (match similar invoices)

- [ ] **GL Code Prediction**
  - Match vendor to most common GL code
  - Use vendor name keywords (e.g., "Gas" → Fuel Expense)
  - Confidence scoring (require manual review if < 80%)

- [ ] **Learning System** (Basic)
  - Track manual corrections
  - Update vendor → GL mappings
  - Feedback loop for improvement

- [ ] **API Integration**
  - Automatic categorization on invoice upload
  - GET `/api/invoices/:id/categorization` - Get suggested categories
  - PATCH `/api/invoices/:id/categorization` - Accept/modify categorization

**Files to Modify:**
- `backend/crates/invoice-processing/src/categorization.rs` - New module
- `backend/crates/invoice-processing/src/lib.rs` - Export categorization
- `backend/crates/db/src/repositories/invoice_repo.rs` - Add categorization queries

**Database Changes:**
- Add columns to `invoices` table:
  - `department_id` (nullable)
  - `gl_code` (nullable)
  - `categorization_confidence` (float)
  - `categorization_source` (enum: 'rule', 'ml', 'manual')

---

## P1 - Nice to Have (2-3 days)

### 4. Advanced Reporting API (1.5 days)
**Effort:** 1.5 days
**Priority:** P1 - Customer value

**Goal:** Build flexible reporting for AP analytics

**Components:**
- [ ] **Report Templates**
  - Spend by vendor (top 10 vendors by amount)
  - Spend by category (GL code breakdown)
  - Approval turnaround time (average days to approve)
  - Invoice volume trends (daily/weekly/monthly)
  - OCR accuracy report (confidence score distribution)

- [ ] **Dynamic Query Builder**
  - Date range filters
  - Vendor filters
  - Amount range filters
  - Group by: vendor, category, department, time period
  - Aggregations: sum, count, average

- [ ] **Export Formats**
  - JSON (default)
  - CSV export
  - PDF report generation (optional)

- [ ] **API Endpoints**
  - POST `/api/reports/generate` - Generate custom report
  - GET `/api/reports/templates` - List available templates
  - POST `/api/reports/templates/:id/generate` - Generate from template

**Files to Create:**
- `backend/crates/reporting/src/report_builder.rs` - Query builder
- `backend/crates/reporting/src/templates.rs` - Report templates
- `backend/crates/reporting/src/export.rs` - Export utilities
- Extend existing `backend/crates/reporting/src/handlers.rs`

---

### 5. Email Digest System (1 day)
**Effort:** 1 day
**Priority:** P1 - Engagement

**Goal:** Send periodic email summaries to users

**Components:**
- [ ] **Digest Types**
  - Daily summary (new invoices, pending approvals)
  - Weekly summary (approval stats, queue health)
  - Monthly summary (spend analysis, vendor insights)

- [ ] **Personalization**
  - Role-based content (AP Clerk vs Controller vs CFO)
  - Include only relevant pending items
  - Highlight actionable items

- [ ] **Email Templates**
  - HTML email templates (using existing email crate)
  - Responsive design
  - Quick action links (approve, review)

- [ ] **Background Jobs**
  - Daily digest job (runs at 6 AM local time)
  - Weekly digest job (runs Monday 8 AM)
  - Monthly digest job (runs 1st of month)

- [ ] **User Preferences**
  - Opt-in/opt-out per digest type
  - Timezone selection
  - Frequency customization

**Files to Create:**
- `backend/crates/email/src/digest/` - Digest generation logic
- `backend/crates/email/src/digest/templates/` - Email templates
- `backend/migrations/014_create_digest_preferences_table/up.sql`
- Extend existing `backend/crates/email/src/lib.rs`

---

## Implementation Order

### Week 1 (March 10-14)
1. **Day 1-2**: Winston AI Foundation
   - Set up AI agent crate
   - Implement basic conversation flow
   - Create invoice query tools

2. **Day 3**: Customer Health Scoring
   - Build scoring algorithm
   - Create health API endpoints
   - Set up background job

3. **Day 4-5**: Intelligent Categorization
   - Implement rule-based categorization
   - Add GL code prediction
   - Integrate with invoice processing

### Week 2 (March 16-20)
4. **Day 6-7**: Advanced Reporting API
   - Build report templates
   - Implement query builder
   - Add CSV export

5. **Day 8-9**: Email Digest System
   - Create digest templates
   - Implement background jobs
   - Add user preferences

6. **Day 10**: Integration & Testing
   - Integration testing
   - Documentation
   - Sprint summary

---

## Technical Requirements

### New Dependencies
```toml
# AI Agent
async-openai = "0.20"  # or anthropic SDK
tiktoken-rs = "0.5"  # Token counting

# Reporting
csv = "1.3"
pdfgen = { version = "0.2", optional = true }  # Optional PDF export
```

### Database Changes
- Migration 012: AI conversations table
- Migration 013: Health scores table
- Migration 014: Digest preferences table
- Update invoices table (categorization fields)

### External Services
- OpenAI API or Anthropic API (for Winston AI)
- Redis (for conversation memory - already available)

---

## Success Criteria

### P0 Deliverables
- [ ] Winston AI can answer invoice status queries
- [ ] Health scores calculated daily for all tenants
- [ ] Invoices auto-categorized with confidence scores
- [ ] All code compiles without errors
- [ ] Integration tests pass for all new features

### P1 Deliverables
- [ ] Report API generates custom reports
- [ ] Email digests sent on schedule
- [ ] User preferences respected
- [ ] Documentation complete

### Quality Gates
- [ ] Test coverage ≥ 80% for new code
- [ ] API response time < 500ms (P95)
- [ ] AI response time < 3s (P95)
- [ ] Zero P0/P1 bugs
- [ ] All migrations reversible

---

## Risks & Mitigations

### Risk 1: AI API Costs
**Impact:** High spend on LLM tokens
**Mitigation:**
- Implement aggressive caching for common queries
- Use smaller models (GPT-3.5 / Claude Haiku) for simple queries
- Set per-tenant token limits
- Monitor costs daily

### Risk 2: AI Hallucination
**Impact:** Incorrect invoice data in responses
**Mitigation:**
- Strict context injection (only provide actual data)
- Require citation (invoice ID, vendor name) in responses
- Add "verify with database" disclaimer
- Log all AI interactions for audit

### Risk 3: Health Score Accuracy
**Impact:** False positives/negatives in risk detection
**Mitigation:**
- Start with conservative thresholds
- Manual review for low-confidence scores
- Iterate based on actual churn data
- Add feedback mechanism for admins

---

## Rollout Plan

### Phase 1: Internal Testing (March 10-16)
- Deploy to staging
- Test with pilot customers' anonymized data
- Validate AI responses, health scores, categorization

### Phase 2: Limited Beta (March 17-20)
- Enable for 2-3 friendly pilot customers
- Monitor usage and feedback
- Adjust scoring algorithms, AI prompts

### Phase 3: Full Rollout (March 21+)
- Enable for all customers
- Activate email digests
- Begin cost monitoring

---

## Dependencies & Blockers

### Dependencies
- ✅ Analytics module (Sprint 9) - for health scoring data
- ✅ Feedback module (Sprint 9) - for sentiment analysis
- ✅ Email service (Sprint 4) - for digest system
- ✅ Worker crate (Sprint 6) - for background jobs
- 🟡 OpenAI/Anthropic API keys - **ACTION: Obtain API credentials**

### Potential Blockers
- **API Key Access**: Need OpenAI or Anthropic API access
  - **Action**: Request API access ASAP
  - **Fallback**: Use local LLM (Ollama) for development

---

## Documentation Deliverables

- [ ] Sprint 10 implementation summary
- [ ] Winston AI usage guide (for end users)
- [ ] Health scoring methodology (for internal team)
- [ ] Reporting API documentation
- [ ] Email digest configuration guide

---

## Next Steps After Sprint 10

### Sprint 11 Preview (Weeks 21-22)
- ML-based OCR confidence improvement
- Advanced workflow automation (conditional routing)
- Customer-facing analytics dashboard
- API rate limiting per tenant tier
- Webhook system for external integrations

### Long-term (Sprint 12+)
- Multi-currency support
- International OCR (non-English invoices)
- Mobile app (React Native)
- Advanced permissions (RBAC v2)
- White-label customization
