# Pilot Onboarding Playbook

This playbook is the operator checklist for bringing a pilot tenant live.

## Prerequisites

- Confirm CI is green on `origin/main`.
- Confirm `pnpm security:audit` has zero P1 findings.
- Confirm local or hosted OCR mode is selected. For local OCR on macOS, install Tesseract with `brew install tesseract`.
- Confirm the pilot has named an AP owner, technical contact, and escalation contact.

## Tenant Setup

1. Create or refresh the tenant using the standard backend startup path.
2. Confirm default queues exist: AP Processing, Review Queue, Error Queue, Approval Queue, Payment Queue.
3. Create pilot users with least-privilege roles.
4. Configure approval limits and assignment rules for the pilot's first workflow.
5. Import vendors from spreadsheet or connect QuickBooks when available.

## First Invoice Validation

1. Upload one representative invoice.
2. Confirm OCR extraction completes or routes to Error Queue with a clear reason.
3. Confirm the invoice lands in the expected queue.
4. Move the invoice through review, approval, and export.
5. Confirm the dashboard and audit activity reflect the workflow.

## Success Metrics

- First invoice processed during onboarding.
- AP owner can find queue state without engineering help.
- No P1 security findings remain open.
- Pilot success target is active usage and measurable invoice processing volume.

## Support Path

- Use `docs/runbooks/on-call-operations.md` for incident response.
- Capture product feedback as issues or internal feedback records.
- Roll back pilot-specific configuration before changing shared code when the issue is tenant setup only.
