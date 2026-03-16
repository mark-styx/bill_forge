# BillForge

A modular, multi-tenant SaaS platform for Accounts Payable teams to manage invoices, vendors, approvals, and reporting.

## Architecture

BillForge is a **monorepo** with a Rust backend and Next.js frontend, built around strict tenant isolation. Each product module can be independently subscribed to while sharing a cohesive user experience.

### Modules

| Module | Description | Key Features |
|--------|-------------|--------------|
| **Invoice Capture** | OCR-powered data extraction | Tesseract / AWS Textract / Google Vision, field correction, confidence scoring |
| **Invoice Processing** | Workflow and approval management | Work queues, assignment rules, approval chains, delegation, limits |
| **Vendor Management** | Vendor lifecycle and communication | Tax document storage, contacts, messaging |
| **Reporting** | Analytics and dashboards | Aging analysis, vendor summaries, workflow metrics, predictive analytics |
| **Integrations** | Accounting system sync | QuickBooks Online OAuth, Xero OAuth |
| **Mobile** | Mobile app backend | Delta sync, push notifications (FCM + APNS), mobile approvals |

## Tech Stack

- **Frontend**: Next.js 14 (App Router), TypeScript, Tailwind CSS, shadcn/ui, React Query, Zustand
- **Backend**: Rust (Axum 0.7), async with Tokio
- **Database**: PostgreSQL (via sqlx), 59 migrations
- **Auth**: Custom JWT with per-request tenant isolation
- **OCR**: Tesseract (default), AWS Textract, Google Vision
- **Storage**: Local filesystem or S3-compatible object storage
- **Testing**: Vitest, React Testing Library, MSW

## Project Structure

```
bill_forge/
├── apps/
│   └── web/                      # Next.js frontend
├── backend/
│   ├── crates/
│   │   ├── api/                  # Axum HTTP API (routes, config, middleware)
│   │   ├── core/                 # Domain types, traits, error handling
│   │   ├── db/                   # PostgreSQL repositories (sqlx)
│   │   ├── auth/                 # JWT authentication
│   │   ├── invoice-capture/      # OCR pipeline
│   │   ├── invoice-processing/   # Workflow engine
│   │   ├── vendor-mgmt/          # Vendor management
│   │   ├── reporting/            # Analytics
│   │   ├── quickbooks/           # QuickBooks Online integration
│   │   ├── xero/                 # Xero integration
│   │   ├── worker/               # Background jobs
│   │   ├── analytics/            # Predictive analytics
│   │   ├── email/                # SMTP email
│   │   ├── notifications/        # Slack/Teams notifications
│   │   ├── mobile-push/          # FCM + APNS push notifications
│   │   └── feedback/             # User feedback collection
│   ├── migrations/               # PostgreSQL migrations
│   └── Cargo.toml                # Workspace configuration
├── packages/
│   └── shared-types/             # Shared TypeScript types
├── sandbox/                      # Demo environment and seed data
├── terraform/                    # Infrastructure as code
└── docker/                       # Docker configurations
```

## Getting Started

### Prerequisites

- Node.js 20+
- Rust 1.75+
- PostgreSQL 15+
- pnpm 8+
- Docker & Docker Compose (optional, for managed infrastructure)

### Development Setup

```bash
# Install frontend dependencies
pnpm install

# Start infrastructure (PostgreSQL, Redis)
docker-compose -f docker/docker-compose.yml up -d

# Run database migrations
pnpm db:migrate

# Start backend
pnpm backend:dev

# Start frontend (separate terminal)
pnpm dev
```

### Sandbox Mode

Pre-configured demo environment with seed data:

```bash
pnpm sandbox:start
```

### Production Build

```bash
# Backend
cd backend && cargo build --release
# Binary output: backend/target/release/billforge-server

# Frontend
pnpm build
```

### Useful Commands

| Command | Description |
|---------|-------------|
| `pnpm dev` | Start frontend dev server |
| `pnpm backend:dev` | Start backend with hot reload |
| `pnpm backend:build` | Production backend build |
| `pnpm db:migrate` | Run database migrations |
| `pnpm test` | Run all tests |
| `pnpm lint` | Lint all packages |
| `pnpm typecheck` | TypeScript type checking |
| `pnpm sandbox:start` | Start sandbox with seed data |
| `pnpm sandbox:reset` | Reset sandbox to clean state |

## Configuration

The backend is configured via environment variables. Key settings:

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `JWT_SECRET` | Secret for JWT signing | Required in production |
| `BACKEND_HOST` | Server bind address | `127.0.0.1` |
| `BACKEND_PORT` | Server port | `8080` |
| `STORAGE_PROVIDER` | `local` or `s3` | `local` |
| `OCR_PROVIDER` | `tesseract`, `aws_textract`, or `google_vision` | `tesseract` |
| `REDIS_URL` | Redis connection string | Optional |

See `.env.example` for a complete list.

## API

All API routes are namespaced under `/api/v1`:

| Route | Module |
|-------|--------|
| `/api/v1/auth` | Authentication |
| `/api/v1/invoices` | Invoice management |
| `/api/v1/vendors` | Vendor management |
| `/api/v1/workflows` | Queues, rules, approvals, delegations |
| `/api/v1/reports` | Reporting and analytics |
| `/api/v1/dashboard` | Dashboard metrics |
| `/api/v1/documents` | Document storage |
| `/api/v1/audit` | Audit logs |
| `/api/v1/settings` | Organization settings |
| `/api/v1/quickbooks` | QuickBooks OAuth |
| `/api/v1/xero` | Xero OAuth |
| `/api/v1/analytics/predictive` | Predictive analytics |
| `/api/v1/mobile` | Mobile sync and push |
| `/api/v1/feedback` | User feedback |

Health checks available at `/health`, `/health/live`, `/health/ready`, `/health/detailed`.

## License

Proprietary - All rights reserved
