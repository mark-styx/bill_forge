# Feedback Round 1 - Issues & Sprint Plan

**Source:** `feedback.jsonl` (14 items, collected 2026-03-13)
**Addressed:** 2026-03-12
**Status:** ADDRESSED - All items triaged and planned below

---

## Feedback Items (Addressed)

| # | Feedback ID | Summary | Severity | Sprint |
|---|-------------|---------|----------|--------|
| 1 | 5d4c739b | Esc key should close detail panes | UX | 11 |
| 2 | 68cbb815 | Should be able to update on-hold invoices | Bug | 11 |
| 3 | b3a47f82 | Invoice statuses should be customizable per org | Feature | 12 |
| 4 | 4894677d | Need to see the document in the details pane | Feature | 11 |
| 5 | e0ce9897 | OCR should be working locally | Bug | 11 |
| 6 | f8bb5858 | Should be a default set of queues | Bug | 11 |
| 7 | 689e3822 | Refresh button takes back to login | Bug | 11 |
| 8 | 249acc60 | Created queue not showing, queues need to exist even if mocked | Bug | 11 |
| 9 | 369fc3f3 | Error creating a vendor | Bug | 11 |
| 10 | 46af42a1 | Queue settings button doesn't do anything | Bug | 11 |
| 11 | a6a3df79 | No way to manually move invoices through queues | UX | 11 |
| 12 | fe9f4df6 | More conditions on assignment rules | Feature | 12 |
| 13 | 1036b2e1 | Error queue doesn't exist | Bug | 11 |
| 14 | e01daeb7 | Workflows need to be fleshed out deeper | Feature | 12 |

---

## Issue Breakdown

### ISSUE-01: Esc key to close detail/side panes
**Feedback:** #1 (5d4c739b)
**Type:** UX improvement
**Severity:** Low
**Effort:** Small (1-2 hours)

**Root Cause:** `InvoicePanel.tsx` and other detail panes only have a click-to-close X button. No `keydown` event listener for Escape.

**Fix:**
- Add `useEffect` with `keydown` listener for `Escape` in all panel/sheet components
- Files to modify:
  - `apps/web/src/components/InvoicePanel.tsx`
  - Any other detail pane components (vendor panel, queue item panel)
- Pattern: `document.addEventListener('keydown', (e) => { if (e.key === 'Escape') onClose() })`

---

### ISSUE-02: Allow updating on-hold invoices
**Feedback:** #2 (68cbb815)
**Type:** Bug / business logic
**Severity:** Medium
**Effort:** Small (2-3 hours)

**Root Cause:** Backend or frontend likely blocks edits when `processing_status = 'on_hold'`. The on-hold status should still allow field edits (just not approval progression).

**Fix:**
- Check `backend/crates/api/src/routes/invoices.rs` update handler for status guards
- Remove or relax the on-hold restriction for field updates (keep restriction only for status transitions like approve/reject)
- Frontend: ensure the edit form is not disabled when status is on-hold

---

### ISSUE-03: Customizable invoice statuses per organization
**Feedback:** #3 (b3a47f82)
**Type:** Feature
**Severity:** Medium
**Effort:** Large (2-3 days)

**Root Cause:** Invoice statuses are hardcoded as Rust enums in `backend/crates/core/src/domain/invoice.rs` (lines 91-110). No tenant-level configuration.

**Fix:**
- Add `invoice_status_config` table (tenant_id, status_name, display_label, color, sort_order, is_terminal)
- Seed default statuses on tenant creation matching current enum values
- Add API endpoints: `GET/PUT /api/v1/settings/invoice-statuses`
- Backend: resolve status validation against tenant config instead of enum
- Frontend: Settings page for managing custom statuses
- Frontend: All status displays pull from tenant config, not hardcoded map

---

### ISSUE-04: Document viewer in detail pane
**Feedback:** #4 (4894677d)
**Type:** Feature
**Severity:** High
**Effort:** Medium (1 day)

**Root Cause:** `InvoicePanel.tsx` shows metadata only. The `invoices/[id]/page.tsx` has `previewBlobUrl` state and document fetch logic, but no embedded viewer. No PDF rendering component exists.

**Fix:**
- Add a PDF/image viewer component (use `react-pdf` or `@react-pdf-viewer/core`)
- In `InvoicePanel.tsx`, add a document preview section:
  - Fetch associated documents via `documentsApi.listForInvoice(invoiceId)`
  - Render PDF inline (or image if applicable)
  - Add "Open full view" link to the full invoice detail page
- Also add viewer to `invoices/[id]/page.tsx` detail page
- Handle loading/error states for document fetch

---

### ISSUE-05: Local OCR should work
**Feedback:** #5 (e0ce9897)
**Type:** Bug / config
**Severity:** High
**Effort:** Medium (3-4 hours)

**Root Cause:** Tesseract integration exists in `backend/crates/invoice-capture/src/ocr/tesseract.rs` but may not work locally due to:
- Tesseract binary not installed or not on PATH
- `TESSERACT_PATH` env var not set
- Missing language data files
- OCR pipeline may not be wired to the upload flow end-to-end

**Fix:**
- Add Tesseract to local dev setup docs / docker-compose
- Verify the upload -> OCR -> field extraction pipeline works end-to-end locally
- Add health check endpoint: `GET /api/v1/health/ocr` that verifies Tesseract availability
- Add clear error messages when Tesseract is not available
- Document setup: `brew install tesseract` (macOS) or Docker image with Tesseract

---

### ISSUE-06: Default queues should exist
**Feedback:** #6 (f8bb5858), #13 (1036b2e1)
**Type:** Bug
**Severity:** High
**Effort:** Small (2-3 hours)

**Root Cause:** `backend/crates/db/src/seed.rs` creates tenants, users, vendors, and invoices but never creates default queues. The hard-coded UUID `11111111-4444-5555-6666-777777770001` in the dashboard layout references a non-existent queue.

**Fix:**
- Add default queue creation to `seed_tenant()` in `seed.rs`:
  - "AP Processing" (type: review)
  - "Review Queue" (type: review)
  - "Error Queue" (type: exception)
  - "Approval Queue" (type: approval)
  - "Payment Queue" (type: payment)
- Also create these queues automatically on new tenant creation (not just seed)
- Remove hardcoded UUID references; use queue lookup by type/name instead

---

### ISSUE-07: Refresh takes back to login
**Feedback:** #7 (689e3822)
**Type:** Bug
**Severity:** Critical
**Effort:** Medium (3-4 hours)

**Root Cause:** Auth flow in `apps/web/src/lib/api.ts` attempts token refresh on 401. If refresh token is expired or the refresh endpoint fails, `onLogoutCallback()` clears state and redirects to `/login`. This likely fires on every page load if the token TTL is too short.

**Fix:**
- Check refresh token TTL on backend (should be days, not minutes)
- Verify refresh endpoint actually works (test manually)
- Frontend: ensure refresh token is properly persisted across page reloads (check `localStorage` key)
- Add silent token refresh on app mount (before first API call)
- Consider: if refresh fails, show "session expired" toast instead of silent redirect
- Check: is the auth middleware stripping cookies on page reload?

---

### ISSUE-08: Created queues not persisting/showing
**Feedback:** #8 (249acc60)
**Type:** Bug
**Severity:** High
**Effort:** Medium (2-3 hours)

**Root Cause:** User created a queue but it doesn't appear on the list page. Possible causes:
- Queue creation API returns success but doesn't actually persist (check SQL)
- Queue list page filters by type/status and excludes new queues
- Tenant context not properly passed during creation
- Frontend cache not invalidated after creation

**Fix:**
- Test queue CRUD end-to-end (create -> list -> verify)
- Check `workflows.rs` create_queue handler and corresponding repo method
- Verify query client invalidation after mutation in frontend
- Ensure queue list page fetches all queues for the tenant without filtering

---

### ISSUE-09: Error creating vendor
**Feedback:** #9 (369fc3f3)
**Type:** Bug
**Severity:** High
**Effort:** Small (1-2 hours)

**Root Cause:** Vendor creation endpoint at `backend/crates/api/src/routes/vendors.rs` looks correct structurally. Likely causes:
- Missing required fields in the `CreateVendorInput` struct that frontend doesn't send
- Database constraint violation (unique vendor name per tenant?)
- tenant_id type mismatch (known issue from recent commits - `4fb7e299`)

**Fix:**
- Check backend logs for the specific error on vendor creation
- Verify `CreateVendorInput` fields match what frontend sends
- Test endpoint directly with curl
- Check for UUID type mismatches in vendor INSERT query (previous tenant_id fix may not cover vendors)

---

### ISSUE-10: Queue settings button does nothing
**Feedback:** #10 (46af42a1)
**Type:** Bug
**Severity:** Medium
**Effort:** Small (3-4 hours)

**Root Cause:** Queue detail page (`processing/queues/[id]/page.tsx`) imports `Settings` icon but the settings button has no onClick handler or routes to no page. Settings are displayed as read-only text.

**Fix:**
- Add a settings modal/dialog for queue configuration
- Fields to expose: queue name, type, SLA hours, escalation hours, escalation user, default sort
- Wire to `PUT /api/v1/workflows/queues/:id` endpoint
- Add form validation (SLA hours > 0, etc.)

---

### ISSUE-11: No obvious way to manually move invoices between queues
**Feedback:** #11 (a6a3df79)
**Type:** UX improvement
**Severity:** High
**Effort:** Medium (3-4 hours)

**Root Cause:** The API endpoint exists (`POST /workflows/invoices/:id/move-to-queue`) and `invoices/[id]/page.tsx` has `showMoveToQueueModal` state. But the queue list/detail pages don't expose a "Move to Queue" action on individual items.

**Fix:**
- Add "Move to Queue" button/action to:
  - Invoice detail panel (`InvoicePanel.tsx`)
  - Queue item rows in queue detail page
  - Invoice list page (bulk action or row action)
- Create a reusable `MoveToQueueDialog` component:
  - Dropdown to select target queue
  - Optional: assign to specific user
  - Confirm button
- Add drag-and-drop between queues as P2 enhancement

---

### ISSUE-12: More conditions on assignment rules
**Feedback:** #12 (fe9f4df6)
**Type:** Feature enhancement
**Severity:** Medium
**Effort:** Medium (1 day)

**Root Cause:** Backend supports rich conditions (10 fields, 14 operators) but the frontend rule builder at `/processing/assignment-rules/new` likely only exposes a subset.

**Fix:**
- Audit the frontend rule builder against backend `ConditionField` enum
- Expose all condition fields: Amount, VendorId, VendorName, Department, GlCode, InvoiceDate, DueDate, Tag, CustomField
- Expose all operators: Equals, NotEquals, GreaterThan, LessThan, Contains, StartsWith, EndsWith, In, NotIn, Between, IsNull, IsNotNull
- Add "AND/OR" compound condition support in UI
- Add condition previews ("If amount > $5,000 AND vendor is Acme Corp")

---

### ISSUE-13: Error queue doesn't exist
**Feedback:** #13 (1036b2e1)
**Type:** Bug
**Severity:** High
**Effort:** Covered by ISSUE-06

**Root Cause:** Same as ISSUE-06. The hardcoded UUID `11111111-4444-5555-6666-777777770001` references a queue that was never created.

**Fix:** Covered by ISSUE-06 (default queue seeding).

---

### ISSUE-14: Workflows need deeper implementation
**Feedback:** #14 (e01daeb7)
**Type:** Feature
**Severity:** Medium
**Effort:** Large (2-3 days)

**Root Cause:** Current workflow system has basic queue routing and assignment rules, but lacks:
- Visual workflow builder/editor
- Multi-step workflow chains (Queue A -> Queue B -> Approval -> Payment)
- Conditional branching (if amount > X, route to manager approval)
- SLA tracking and escalation automation
- Workflow templates

**Fix:**
- Add workflow template model (sequence of steps with conditions)
- Create visual workflow editor (drag-and-drop step builder)
- Implement step execution engine that advances invoices through workflow
- Add SLA timers with automatic escalation
- Pre-built templates: "Standard AP", "High-Value Review", "Exception Handling"

---

## Sprint Plan

### Sprint 11: Feedback Fixes - Bugs & Core UX (Week 21-22)
**Focus:** Fix all bugs and critical UX gaps reported in feedback round 1
**Effort:** ~4-5 days of work

| Priority | Issue | Summary | Effort | Status |
|----------|-------|---------|--------|--------|
| P0 | ISSUE-07 | Refresh takes back to login (auth bug) | 3-4 hrs | DONE |
| P0 | ISSUE-06 | Default queues + error queue don't exist | 2-3 hrs | DONE |
| P0 | ISSUE-08 | Created queues not showing | 2-3 hrs | DONE |
| P0 | ISSUE-09 | Error creating vendor | 1-2 hrs | DONE |
| P0 | ISSUE-05 | OCR not working locally | 3-4 hrs | DONE |
| P1 | ISSUE-04 | Document viewer in detail pane | 1 day | DONE |
| P1 | ISSUE-11 | Manual invoice queue movement UI | 3-4 hrs | DONE |
| P1 | ISSUE-02 | Allow updating on-hold invoices | 2-3 hrs | DONE |
| P1 | ISSUE-10 | Queue settings button handler | 3-4 hrs | DONE |
| P2 | ISSUE-01 | Esc key closes detail panes | 1-2 hrs | DONE |

**Sprint 11 Success Criteria:**
- [x] Page refresh preserves session (no redirect to login)
- [x] Default queues auto-created for new tenants and existing seed data
- [x] Queue CRUD fully functional (create, list, update settings)
- [x] Vendor creation works without errors
- [x] Tesseract OCR runs locally on uploaded documents
- [x] PDF/image viewer visible in invoice detail pane
- [x] Users can move invoices between queues via UI
- [x] On-hold invoices can be edited
- [x] Esc key closes all detail/side panes

---

### Sprint 12: Feedback Fixes - Features & Workflow Depth (Week 23-24)
**Focus:** Implement feature requests and deepen workflow capabilities
**Effort:** ~5-6 days of work

| Priority | Issue | Summary | Effort |
|----------|-------|---------|--------|
| P1 | ISSUE-14 | Deeper workflow implementation | 2-3 days | DONE |
| P1 | ISSUE-12 | Full assignment rule conditions in UI | 1 day | DONE |
| P2 | ISSUE-03 | Customizable invoice statuses per org | 2-3 days | DONE |

**Sprint 12 Success Criteria:**
- [x] Multi-step workflow chains configurable per tenant
- [x] Visual workflow editor functional
- [x] All backend condition fields/operators exposed in rule builder UI
- [x] Compound AND/OR conditions supported (AND logic with visual separators and preview)
- [x] Tenants can customize invoice status names and colors
- [x] Default statuses seeded, matching current hardcoded values
- [x] Workflow templates available ("Standard AP", "High-Value Review")

---

## Machine Updates Required

### Sprint 11 Files to Modify

**Backend:**
- `backend/crates/db/src/seed.rs` - Add default queue seeding
- `backend/crates/api/src/routes/invoices.rs` - Allow on-hold invoice updates
- `backend/crates/api/src/routes/vendors.rs` - Debug/fix vendor creation
- `backend/crates/api/src/routes/workflows.rs` - Verify queue CRUD persistence
- `backend/crates/api/src/routes/auth.rs` - Fix refresh token flow
- `backend/crates/invoice-capture/src/ocr/tesseract.rs` - Verify local OCR pipeline
- `backend/migrations/` - Possibly new migration for default queues on tenant create

**Frontend:**
- `apps/web/src/components/InvoicePanel.tsx` - Add Esc handler, document viewer, move-to-queue action
- `apps/web/src/lib/api.ts` - Fix token refresh persistence
- `apps/web/src/stores/auth.ts` - Fix session persistence across page reloads
- `apps/web/src/app/(dashboard)/processing/queues/[id]/page.tsx` - Wire settings button, add move action
- `apps/web/src/app/(dashboard)/invoices/[id]/page.tsx` - Add document viewer
- `apps/web/src/components/MoveToQueueDialog.tsx` - NEW: reusable queue selector dialog
- `apps/web/src/components/DocumentViewer.tsx` - NEW: PDF/image viewer component
- `apps/web/src/components/QueueSettingsDialog.tsx` - NEW: queue settings editor

**Dependencies to add:**
- `react-pdf` or `@react-pdf-viewer/core` (PDF rendering)

### Sprint 12 Files to Modify

**Backend:**
- `backend/migrations/` - New migration for `invoice_status_config` table, `workflow_templates` table
- `backend/crates/core/src/domain/invoice.rs` - Refactor status from enum to configurable
- `backend/crates/core/src/domain/workflow.rs` - Add workflow template model
- `backend/crates/api/src/routes/settings.rs` - NEW: tenant settings endpoints
- `backend/crates/api/src/routes/workflows.rs` - Add workflow template CRUD

**Frontend:**
- `apps/web/src/app/(dashboard)/processing/assignment-rules/new/page.tsx` - Full condition builder
- `apps/web/src/app/(dashboard)/settings/statuses/page.tsx` - NEW: status config page
- `apps/web/src/components/WorkflowEditor.tsx` - NEW: visual workflow builder
- `apps/web/src/components/ConditionBuilder.tsx` - NEW: rich condition builder component
