# BillForge

A modular SaaS platform for Accounts Payable teams to manage invoices, vendors, approvals, and reporting.

## Architecture Overview

BillForge is built as a **modular monorepo** with complete tenant isolation. Each product module can be independently subscribed to, yet all share a cohesive user experience.

### Products

| Product | Description | Key Features |
|---------|-------------|--------------|
| **Invoice Capture** | OCR-powered invoice data extraction | Multi-OCR support, field correction UI, data export, API |
| **Invoice Processing** | Workflow management for invoice approval | Role-based approvals, custom routing rules, work queues |
| **Vendor Management** | Vendor lifecycle management | Tax document storage, contractor communication |
| **Reporting** | Analytics and dashboards | Cross-module insights, custom reports |

## Tech Stack

- **Frontend**: Next.js 14+ (App Router), TypeScript, Tailwind CSS, shadcn/ui
- **Backend**: Rust (Axum), DuckDB, SQLite (metadata)
- **OCR**: Tesseract (local), AWS Textract, Google Vision (cloud options)
- **Auth**: Custom JWT with tenant isolation
- **Storage**: S3-compatible object storage

## Project Structure

```
bill_forge/
├── apps/
│   └── web/                    # Next.js frontend application
├── backend/
│   ├── crates/
│   │   ├── api/                # HTTP API layer (Axum)
│   │   ├── core/               # Shared domain logic
│   │   ├── invoice-capture/    # OCR and data extraction
│   │   ├── invoice-processing/ # Workflow engine
│   │   ├── vendor-mgmt/        # Vendor management
│   │   ├── reporting/          # Analytics engine
│   │   ├── auth/               # Authentication & authorization
│   │   └── db/                 # Database layer (DuckDB + SQLite)
│   └── Cargo.toml              # Workspace configuration
├── packages/
│   └── shared-types/           # TypeScript types shared across modules
├── migrations/                 # Database migrations
├── sandbox/                    # Sandbox environment configuration
└── docker/                     # Docker configurations
```

## Multi-Tenancy Model

BillForge uses **database-per-tenant** isolation:

- Each tenant has isolated DuckDB files for analytical data
- SQLite for tenant metadata and authentication
- Complete data segregation with no possibility of cross-tenant access
- Tenant context validated on every request

## Getting Started

### Prerequisites

- Node.js 20+
- Rust 1.75+
- Docker & Docker Compose
- pnpm 8+

### Development Setup

```bash
# Install dependencies
pnpm install

# Start infrastructure (databases, storage)
docker-compose up -d

# Run database migrations
pnpm db:migrate

# Start backend (Rust)
cd backend && cargo run

# Start frontend (separate terminal)
pnpm dev
```

### Sandbox Mode

```bash
# Start sandbox with seed data
pnpm sandbox:start

# Access at http://localhost:3000
# Demo credentials in sandbox/README.md
```

## Environment Variables

Copy `.env.example` to `.env` and configure:

```env
# Database
DATABASE_URL=sqlite://./data/billforge.db
TENANT_DB_PATH=./data/tenants

# Authentication
JWT_SECRET=your-secret-key
JWT_EXPIRY=24h

# OCR Configuration
OCR_PROVIDER=tesseract # tesseract | aws_textract | google_vision
AWS_ACCESS_KEY_ID=
AWS_SECRET_ACCESS_KEY=
GOOGLE_APPLICATION_CREDENTIALS=

# Storage
STORAGE_PROVIDER=local # local | s3
S3_BUCKET=
S3_REGION=
```

## API Documentation

API documentation available at `/api/docs` when running locally.

## License

Proprietary - All rights reserved
