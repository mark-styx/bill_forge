# Product Knowledge Source Catalog

**Phase 2B: Platform Expert And Issue Intake**

This file is the canonical product knowledge source catalog used by product experts and issue intake workflows. Every durable knowledge source that informs product decisions, implementation history, or API surface should be listed here so that intake reviewers can locate authoritative information without searching the repository ad hoc.

## Catalog

| Knowledge area | Source path | Source type | Primary contents | Update trigger | Freshness expectation | Intake usage |
|----------------|-------------|-------------|------------------|----------------|----------------------|--------------|
| Product docs | `README.md` | Markdown | Project overview, setup instructions, architecture summary | Major feature or onboarding change | Current within one sprint | Confirm product intent and scope |
| Product docs | `PRODUCT_STRATEGY.md` | Markdown | Business model, target users, value proposition | Product strategy revision | Current per quarter | Validate feature alignment with strategy |
| Product docs | `features.md` | Markdown | Feature list and descriptions | Feature addition or deprecation | Current within one sprint | Check feature coverage |
| Product docs | `docs/northstar.md` | Markdown | North-star metric and product vision | Vision refresh | Current per quarter | Ground decisions in product vision |
| Product docs | `docs/CPO_PRODUCT_STRATEGY_V13.md` | Markdown | Detailed product strategy from CPO | Quarterly strategy cycle | Current per quarter | Reference authoritative product direction |
| Product docs | `docs/CTO_STRATEGIC_TECHNICAL_PLAN_V7.md` | Markdown | Technical strategy and architecture roadmap | Quarterly planning cycle | Current per quarter | Reference technical direction |
| Implementation plans | `SPRINT_PLAN.md` | Markdown | Current sprint objectives and tasks | Sprint kickoff | Updated every sprint | Confirm planned work |
| Implementation plans | `POLISH_PLAN.md` | Markdown | Polish and quality improvement backlog | Polish cycle start | Updated per cycle | Identify deferred quality items |
| Implementation plans | `docs/sprint9_implementation_plan.md` | Markdown | Sprint 9 plan | Sprint 9 kickoff | Frozen | Historical reference |
| Implementation plans | `docs/sprint10_implementation_plan.md` | Markdown | Sprint 10 plan | Sprint 10 kickoff | Frozen | Historical reference |
| Implementation plans | `docs/feedback_round1_sprint_plan.md` | Markdown | Feedback-driven sprint plan | Feedback review | Frozen | Historical reference |
| Implementation plans | `docs/edi_integration_plan.md` | Markdown | EDI integration design and plan | EDI milestone | Frozen | Historical reference |
| Sprint summaries | `docs/sprint2_implementation_summary.md` | Markdown | Sprint 2 results | Sprint 2 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint3_implementation_summary.md` | Markdown | Sprint 3 results | Sprint 3 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint4_implementation_summary.md` | Markdown | Sprint 4 results | Sprint 4 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint5_implementation_summary.md` | Markdown | Sprint 5 results | Sprint 5 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint6_implementation_summary.md` | Markdown | Sprint 6 results | Sprint 6 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint7_implementation_summary.md` | Markdown | Sprint 7 results | Sprint 7 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint8_implementation_summary.md` | Markdown | Sprint 8 results | Sprint 8 retro | Frozen | Implementation history |
| Sprint summaries | `docs/sprint9_implementation_summary.md` | Markdown | Sprint 9 results | Sprint 9 retro | Frozen | Implementation history |
| Sprint summaries | `backend/docs/sprint13_intelligent_routing.md` | Markdown | Intelligent routing implementation | Sprint 13 retro | Frozen | Implementation history |
| Sprint summaries | `backend/docs/sprint13_notifications.md` | Markdown | Notifications implementation | Sprint 13 retro | Frozen | Implementation history |
| Sprint summaries | `backend/docs/sprint13_predictive_analytics.md` | Markdown | Predictive analytics implementation | Sprint 13 retro | Frozen | Implementation history |
| Sprint summaries | `backend/docs/sprint14_enhanced_ocr.md` | Markdown | Enhanced OCR implementation | Sprint 14 retro | Frozen | Implementation history |
| OpenAPI metadata | `backend/crates/api/src/openapi.rs` | Rust source | OpenAPI spec generation, schema definitions | API surface change | Current with every API merge | Inspect API schema and endpoint contracts |
| OpenAPI metadata | `backend/crates/api/tests/openapi_spec_test.rs` | Rust test | OpenAPI spec validation tests | API surface change | Current with every API merge | Confirm spec test coverage |
| Route metadata | `backend/crates/api/src/routes/mod.rs` | Rust source | Route module registry and mounting | Route addition or removal | Current with every API merge | Map API surface |
| Route metadata | `backend/crates/api/src/routes/` | Rust source dir | Individual route handler modules | Route addition or removal | Current with every API merge | Trace endpoint implementations |
| Route metadata | `apps/web/src/lib/api.ts` | TypeScript | Frontend API client functions | Backend API change | Current with every API merge | Confirm frontend-backend contract |
| Route metadata | `apps/web/src/app/(dashboard)/` | TSX directory | Dashboard route pages | UI feature change | Current with every UI sprint | Map user-facing pages |
| Module definitions | `backend/Cargo.toml` | TOML | Rust workspace root and crate list | Crate addition or removal | Current | Identify backend modules |
| Module definitions | `backend/crates/*/Cargo.toml` | TOML (glob) | Per-crate dependencies and features | Dependency change | Current | Trace crate dependency graph |
| Module definitions | `packages/shared-types/src/index.ts` | TypeScript | Shared type definitions exported to frontend | Type contract change | Current with every API merge | Confirm type parity |
| Module definitions | `apps/web/package.json` | JSON | Web app dependencies and scripts | Dependency change | Current | Identify frontend modules |
| Module definitions | `pnpm-workspace.yaml` | YAML | Monorepo workspace package list | Package addition or removal | Current | Map workspace structure |
| Known issues | `docs/known_issues.md` | Markdown | Structured known-issues register | Issue discovery or resolution | Reviewed every sprint | Check for existing known issues |
| Runbooks | `docs/runbooks/readiness-gates.md` | Markdown | Coverage, benchmark, and security gate commands | Pilot readiness process change | Current with each pilot-readiness phase | Confirm readiness verification commands |
| Runbooks | `docs/runbooks/security-audit.md` | Markdown | Security audit triage and evidence rules | Security gate change | Current with security process changes | Evaluate dependency advisory handling |
| Runbooks | `docs/runbooks/pilot-onboarding.md` | Markdown | Pilot tenant setup and validation checklist | Pilot onboarding process change | Current with each pilot cycle | Plan and validate pilot onboarding |
| Release notes | `CHANGELOG.md` | Markdown | Release history and change log | Every release | Updated per release | Confirm release-note impact |
| Release notes | `.github/workflows/release.yml` | YAML | Release CI/CD workflow definition | Release process change | Current | Understand release pipeline |

## Catalog Maintenance Rules

1. **New durable product knowledge sources** must be added to this catalog when introduced. If a new markdown document, source file, or configuration artifact provides durable product knowledge, add a row before merging.
2. **Implementation and sprint documents** should be linked in this catalog after the sprint completes and the summary is published.
3. **Known issues** should be removed from `docs/known_issues.md` only when resolved or superseded by a newer entry with a cross-reference.

## Phase 2B Intake Checklist

When reviewing an issue or feature request through the Phase 2B intake process:

1. **Check product intent**: Consult the Product docs entries to confirm the request aligns with product strategy and vision.
2. **Confirm implementation history**: Search Sprint summaries and Implementation plans to see whether related work was already done or planned.
3. **Inspect API/route/module metadata**: Use OpenAPI metadata, Route metadata, and Module definitions entries to understand the current API surface and dependency graph.
4. **Check known issues**: Review `docs/known_issues.md` to see if the issue is already tracked or if a related defect exists.
5. **Confirm release-note impact**: Determine whether the change requires a `CHANGELOG.md` entry and whether the release workflow is affected.
