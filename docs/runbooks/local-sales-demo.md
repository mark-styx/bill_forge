# Local Sales Demo Runbook

## Start

From the repo root:

```bash
docker compose up -d postgres redis minio minio-init
pnpm install
pnpm backend:dev
pnpm dev
```

Open the web app at the frontend URL printed by `pnpm dev`.

## Demo Login

Use the login page's **Use demo account** button, or enter:

```text
Tenant: Meridian Industries
Email: admin@sandbox.local
Password: sandbox123
```

The sandbox tenant ID remains `11111111-1111-1111-1111-111111111111` for API-level checks.

## Happy Path

1. Start at `/dashboard` and show live KPIs, recent activity, and the AP workload.
2. Open `/invoices` and show the seeded invoice mix: review, pending approval, ready for payment, on hold, OCR errors, rejected, and paid.
3. Open `/processing/approvals` and approve or reject a pending invoice.
4. Open `/reports` and show approval SLA bottlenecks plus cash-flow obligations.
5. Open `/migrate` and walk through QuickBooks or spreadsheet vendor import.
6. Open `/integrations` and explain QuickBooks-first ERP sync with other connectors available as later-stage integrations.

## Reset

The API startup path reseeds the fixed sandbox tenant idempotently. For a full reset:

```bash
docker compose down -v
docker compose up -d postgres redis minio minio-init
pnpm backend:dev
```

## Known Limits

- This is a local sandbox demo, not a production compliance environment.
- Email delivery may use the mock provider unless real email settings are configured.
- External ERP OAuth flows require provider credentials; the demo can still show integration status and import/export surfaces.
