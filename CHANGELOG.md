# CHANGELOG

## 2026-03-20 - Repository cleanup and CI fixes

### Fixed
- **CI pipeline**: Fixed `dtolnay/rust-action` → `dtolnay/rust-toolchain` in all GitHub Actions workflows
- **CI pipeline**: Removed invalid YAML workflow files (`1e291820.yaml`, `850e5b20.yaml`) that were monitoring configs, not GitHub Actions
- **Frontend build**: Excluded `vitest.config.ts` from Next.js TypeScript compilation to resolve Vite version mismatch
- **Lockfile**: Regenerated `pnpm-lock.yaml` to be compatible with pnpm 8

### Removed
- Agent-generated hash-named artifacts (feature crates, test files, Docker files, Terraform modules)
- Legacy SQLite-era crates (`DbPool`, `Invoice`, `OCRResult`, `invoice_service`, `tests`)
- 29 duplicate strategy document iterations (kept latest versions only)
- Agent loop scripts and output files

### Updated
- `SPRINT_PLAN.md` rewritten to reflect actual project state (14 sprints completed)
- `tsconfig.json` excludes test files from production build

---

## 2026-03-12 - Fix tenant_id UUID type mismatches across backend

### Problem
All `tenant_id` database columns are `UUID`, but Rust code was binding them as strings (`.as_str()`) and deserializing row results into `String` fields. This causes runtime type mismatch errors when PostgreSQL strict type checking is enabled.

### Fixed
- **17 source files** updated to bind `tenant_id` as `Uuid` via `*tenant_id.as_uuid()` instead of `.as_str()`
- **Row struct types** changed from `tenant_id: String` to `tenant_id: Uuid`
- **Column name fix**: `total_amount` → `total_amount_cents` in predictive analytics queries
- **Mobile routes**: `tenant_id.to_string()` → `tenant_id.0` for correct UUID binding
- **Migration 056**: Added `report_digests` table to the VARCHAR→UUID type migration

### Build Status
- Release build successful (billforge-server 23MB, billforge-worker 5.2MB)

---

## 2026-03-12 - Code review fixes

### Fixed
- **Race condition in token refresh**: Added locking mechanism to prevent multiple concurrent refresh requests
- **Duplicated callback setup**: Extracted `setupApiCallbacks()` helper
- **Silent error swallowing**: Added error logging in token refresh failure paths
- **Fetch request duplication**: Created `executeRequest()` helper method
- **Naming confusion**: Fixed `_hasHydrated` → `hasHydrated`

---

## 2026-03-05 - Sprint 11 feedback fixes

### Fixed
- Auth token refresh reliability
- Default queue creation on tenant setup
- Vendor creation flow
- On-hold invoice editing
- ESC key behavior in modals
- Queue CRUD operations
- OCR configuration UI
- Document viewer improvements
- Move-to-queue functionality
- Queue settings page

---

## 2026-02-20 - Sprint 14: Predictive Analytics & Mobile

### Added
- Predictive analytics engine with anomaly detection
- Enhanced OCR provider support (multi-provider comparison)
- Mobile app backend with delta sync protocol
- Push notification infrastructure (FCM + APNS)
- Feedback chat component and API route

---

## 2026-02-10 - Sprint 13: AI & Notifications

### Added
- ML categorization pipeline with embedding cache and feedback loop
- ML confidence threshold for auto-approval workflows
- Intelligent approval routing with workload balancing
- Slack and Teams notification integration
- Xero accounting integration (OAuth 2.0)
- Background job scheduler for ML pipeline tasks

---

## 2026-01-30 - Sprint 10: Advanced Reporting & AI Foundation

### Added
- Advanced reporting API
- Email digest system
- AI categorization baseline
- ML categorizer integration with fallback

---

## Prior Sprints (1-9)

Core platform built: multi-tenant PostgreSQL architecture, JWT auth, invoice capture with multi-provider OCR, work queues, assignment rules, approval chains, delegations, workflow templates, QuickBooks integration, email approvals, dashboard, reports, vendor management, settings, and theme customization.
