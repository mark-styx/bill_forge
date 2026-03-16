<p align="center">
  <h1 align="center">BillForge</h1>
  <p align="center">
    A modular, multi-tenant SaaS platform for Accounts Payable teams
    <br />
    <strong>Invoice Capture - Processing - Vendor Management - Reporting</strong>
  </p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Axum-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Axum" />
  <img src="https://img.shields.io/badge/Next.js_14-000000?style=for-the-badge&logo=next.js&logoColor=white" alt="Next.js" />
  <img src="https://img.shields.io/badge/TypeScript-3178C6?style=for-the-badge&logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/PostgreSQL-4169E1?style=for-the-badge&logo=postgresql&logoColor=white" alt="PostgreSQL" />
  <img src="https://img.shields.io/badge/Tailwind_CSS-06B6D4?style=for-the-badge&logo=tailwindcss&logoColor=white" alt="Tailwind" />
</p>

---

## Overview

BillForge automates the full accounts payable lifecycle, from document capture through payment approval. Built as a modular monorepo with strict multi-tenant isolation, each module can be independently enabled per organization.

## Screenshots

<details open>
<summary><strong>Dashboard</strong> - Real-time KPIs, quick actions, and activity feed</summary>
<br />
<p align="center">
  <img src="docs/screenshots/dashboard.png" alt="Dashboard" width="100%" />
</p>
</details>

<details>
<summary><strong>Login</strong> - Multi-tenant login with product configuration</summary>
<br />
<p align="center">
  <img src="docs/screenshots/login.png" alt="Login" width="100%" />
</p>
</details>

<details>
<summary><strong>Invoices</strong> - Invoice management with search, filters, and status tracking</summary>
<br />
<p align="center">
  <img src="docs/screenshots/invoices.png" alt="Invoices" width="100%" />
</p>
</details>

<details>
<summary><strong>Invoice Processing</strong> - Approvals, work queues, and workflow management</summary>
<br />
<p align="center">
  <img src="docs/screenshots/processing.png" alt="Invoice Processing" width="100%" />
</p>
</details>

<details>
<summary><strong>Workflow Templates</strong> - Multi-step invoice processing pipelines</summary>
<br />
<p align="center">
  <img src="docs/screenshots/workflows.png" alt="Workflow Templates" width="100%" />
</p>
</details>

<details>
<summary><strong>Reports</strong> - Analytics, charts, and performance metrics</summary>
<br />
<p align="center">
  <img src="docs/screenshots/reports.png" alt="Reports" width="100%" />
</p>
</details>

<details>
<summary><strong>Settings</strong> - Organization configuration and customization</summary>
<br />
<p align="center">
  <img src="docs/screenshots/settings.png" alt="Settings" width="100%" />
</p>
</details>

## Architecture

### System Architecture

```mermaid
graph TB
    subgraph Frontend["Frontend (Next.js 14)"]
        UI[Dashboard & UI]
        RQ[React Query]
        ZS[Zustand State]
    end

    subgraph API["Backend (Rust / Axum)"]
        GW[API Gateway<br/>/api/v1]
        AUTH[Auth Middleware<br/>JWT + Tenant Isolation]

        subgraph Modules
            IC[Invoice Capture]
            IP[Invoice Processing]
            VM[Vendor Management]
            RP[Reporting]
            AN[Predictive Analytics]
        end

        subgraph Services
            OCR[OCR Engine<br/>Tesseract / Textract / Vision]
            WK[Background Worker]
            EM[Email Service]
            NT[Notifications<br/>Slack / Teams]
            MP[Mobile Push<br/>FCM / APNS]
        end
    end

    subgraph Integrations
        QB[QuickBooks Online]
        XR[Xero]
    end

    subgraph Data
        PG[(PostgreSQL)]
        S3[Object Storage<br/>Local / S3]
        RD[(Redis)]
    end

    UI --> RQ --> GW
    GW --> AUTH --> Modules
    Modules --> Services
    IC --> OCR
    IP --> WK
    Modules --> PG
    Modules --> S3
    WK --> RD
    API --> QB
    API --> XR

    style Frontend fill:#1a1a2e,stroke:#16213e,color:#e0e0e0
    style API fill:#0f3460,stroke:#16213e,color:#e0e0e0
    style Data fill:#533483,stroke:#16213e,color:#e0e0e0
    style Integrations fill:#1a1a2e,stroke:#e94560,color:#e0e0e0
```

### Invoice Processing Pipeline

```mermaid
flowchart LR
    A[Document Upload] --> B[OCR Extraction]
    B --> C{Confidence<br/>Check}
    C -->|High| D[Auto-populate Fields]
    C -->|Low| E[Manual Review Queue]
    E --> D
    D --> F[Assignment Rules<br/>Engine]
    F --> G[Work Queue]
    G --> H{Approval<br/>Chain}
    H -->|Approved| I[Ready for Payment]
    H -->|Rejected| J[Return to Submitter]
    H -->|Escalated| K[Delegation /<br/>Override]
    K --> H

    style A fill:#0ea5e9,stroke:#0284c7,color:#fff
    style B fill:#8b5cf6,stroke:#7c3aed,color:#fff
    style D fill:#10b981,stroke:#059669,color:#fff
    style G fill:#f59e0b,stroke:#d97706,color:#fff
    style I fill:#22c55e,stroke:#16a34a,color:#fff
    style J fill:#ef4444,stroke:#dc2626,color:#fff
```

## Modules

<table>
<tr>
<td width="50%">

### Invoice Capture
- Multi-provider OCR (Tesseract, AWS Textract, Google Vision)
- Confidence scoring with automatic field extraction
- Manual correction UI for low-confidence results
- Bulk upload and batch processing

</td>
<td width="50%">

### Invoice Processing
- Configurable work queues with priority ordering
- Assignment rules engine with multi-condition logic
- Multi-level approval chains
- Approval delegation and spending limits
- Workflow templates

</td>
</tr>
<tr>
<td width="50%">

### Vendor Management
- Full vendor lifecycle (onboarding to offboarding)
- Tax document collection and storage (W-9, 1099)
- Vendor contacts and communication log
- Vendor-specific approval routing

</td>
<td width="50%">

### Reporting & Analytics
- Real-time dashboard with KPIs
- Invoice aging analysis
- Vendor spend summaries
- Workflow performance metrics
- Predictive analytics and anomaly detection

</td>
</tr>
<tr>
<td width="50%">

### Integrations
- QuickBooks Online (OAuth 2.0)
- Xero (OAuth 2.0)
- Email-based approve/reject actions
- Slack and Teams notifications

</td>
<td width="50%">

### Mobile
- Delta sync protocol for offline-first mobile
- Push notifications (FCM + APNS)
- Mobile approval workflows
- Device management

</td>
</tr>
</table>

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Frontend** | Next.js 14 (App Router), TypeScript, Tailwind CSS, shadcn/ui |
| **State** | React Query (server), Zustand (client) |
| **Backend** | Rust, Axum 0.7, Tokio async runtime |
| **Database** | PostgreSQL 15+ via sqlx (59 migrations) |
| **Auth** | Custom JWT with per-request tenant validation |
| **OCR** | Tesseract (default), AWS Textract, Google Vision |
| **Storage** | Local filesystem or S3-compatible |
| **Cache** | Redis |
| **Testing** | Vitest, React Testing Library, MSW |
| **Infra** | Docker, Terraform |

## Project Structure

```
bill_forge/
├── apps/web/                       # Next.js 14 frontend
│   └── src/
│       ├── app/(dashboard)/        # App Router pages
│       │   ├── dashboard/          #   Dashboard & KPIs
│       │   ├── invoices/           #   Invoice CRUD, upload, detail
│       │   ├── vendors/            #   Vendor management
│       │   ├── processing/         #   Queues, rules, approvals
│       │   │   ├── queues/         #     Work queue management
│       │   │   ├── assignment-rules/ #   Routing rules
│       │   │   ├── workflows/      #     Workflow templates
│       │   │   ├── approvals/      #     Approval chains
│       │   │   ├── delegations/    #     Approval delegation
│       │   │   └── approval-limits/ #    Spending limits
│       │   ├── reports/            #   Analytics & export
│       │   └── settings/           #   Organization config
│       ├── components/ui/          # shadcn/ui components
│       └── lib/api.ts              # Typed API client
│
├── backend/
│   ├── crates/
│   │   ├── api/                    # Axum HTTP layer
│   │   ├── core/                   # Domain types & traits
│   │   ├── db/                     # PostgreSQL repositories
│   │   ├── auth/                   # JWT authentication
│   │   ├── invoice-capture/        # OCR pipeline
│   │   ├── invoice-processing/     # Workflow engine
│   │   ├── vendor-mgmt/           # Vendor lifecycle
│   │   ├── reporting/             # Analytics queries
│   │   ├── analytics/             # Predictive models
│   │   ├── quickbooks/            # QB Online OAuth
│   │   ├── xero/                  # Xero OAuth
│   │   ├── worker/                # Background jobs
│   │   ├── email/                 # SMTP service
│   │   ├── notifications/         # Slack / Teams
│   │   ├── mobile-push/           # FCM + APNS
│   │   └── feedback/              # User feedback
│   ├── migrations/                # 59 PostgreSQL migrations
│   └── Cargo.toml                 # Workspace manifest
│
├── sandbox/                       # Demo environment & seed data
├── terraform/                     # Infrastructure as code
└── docker/                        # Dockerfiles & compose configs
```

## Getting Started

### Prerequisites

- **Node.js** 20+ and **pnpm** 8+
- **Rust** 1.75+ (with `cargo-watch` for dev)
- **PostgreSQL** 15+
- **Docker** (optional, for managed infra)

### Quick Start

```bash
# 1. Install dependencies
pnpm install

# 2. Start infrastructure
docker-compose -f docker/docker-compose.yml up -d

# 3. Run migrations
pnpm db:migrate

# 4. Start backend (with hot reload)
pnpm backend:dev

# 5. Start frontend (separate terminal)
pnpm dev
```

### Sandbox Mode

Launch a pre-configured demo environment:

```bash
pnpm sandbox:start    # Start with seed data
pnpm sandbox:reset    # Reset to clean state
```

### Production Build

```bash
# Backend binary
cd backend && cargo build --release
# Output: backend/target/release/billforge-server

# Frontend
pnpm build
```

### Commands

| Command | Description |
|---------|-------------|
| `pnpm dev` | Frontend dev server |
| `pnpm backend:dev` | Backend with hot reload |
| `pnpm backend:build` | Production backend build |
| `pnpm db:migrate` | Run database migrations |
| `pnpm test` | Run all tests |
| `pnpm lint` | Lint all packages |
| `pnpm typecheck` | TypeScript type checking |

## Configuration

Configure via environment variables (see `.env.example`):

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `JWT_SECRET` | JWT signing secret | Required in production |
| `BACKEND_HOST` | Bind address | `127.0.0.1` |
| `BACKEND_PORT` | Server port | `8080` |
| `STORAGE_PROVIDER` | `local` or `s3` | `local` |
| `OCR_PROVIDER` | `tesseract`, `aws_textract`, `google_vision` | `tesseract` |
| `REDIS_URL` | Redis connection | Optional |

## API Reference

All endpoints under `/api/v1`, authenticated via JWT Bearer token with tenant context.

```mermaid
graph LR
    subgraph Core
        A[/auth]
        B[/invoices]
        C[/vendors]
        D[/documents]
    end

    subgraph Processing
        E[/workflows]
        F[/dashboard]
        G[/reports]
        H[/audit]
    end

    subgraph Integrations
        I[/quickbooks]
        J[/xero]
        K[/mobile]
        L[/analytics/predictive]
    end

    subgraph Config
        M[/settings]
        N[/feedback]
        O[/actions]
    end

    style Core fill:#0ea5e9,stroke:#0284c7,color:#fff
    style Processing fill:#8b5cf6,stroke:#7c3aed,color:#fff
    style Integrations fill:#f59e0b,stroke:#d97706,color:#fff
    style Config fill:#6b7280,stroke:#4b5563,color:#fff
```

Health endpoints: `/health`, `/health/live`, `/health/ready`, `/health/detailed`

## Multi-Tenancy

```mermaid
flowchart TD
    REQ[Incoming Request] --> JWT[JWT Validation]
    JWT --> TID[Extract tenant_id]
    TID --> MW[Tenant Middleware]
    MW --> DB[All Queries Scoped<br/>WHERE tenant_id = $1]
    MW --> ST[Storage Scoped<br/>tenant_id/files/...]
    MW --> AU[Audit Scoped<br/>Per-tenant logs]

    style REQ fill:#e0e0e0,stroke:#999,color:#333
    style MW fill:#ef4444,stroke:#dc2626,color:#fff
    style DB fill:#4169E1,stroke:#2c4fa1,color:#fff
    style ST fill:#10b981,stroke:#059669,color:#fff
    style AU fill:#8b5cf6,stroke:#7c3aed,color:#fff
```

Every database query, storage operation, and audit log is scoped to the authenticated tenant. Cross-tenant data access is architecturally impossible.

## License

Proprietary - All rights reserved
