# BillForge Sandbox

A pre-configured environment for testing and development.

## Quick Start

```bash
# From the project root
pnpm sandbox:start
```

This will:
1. Start the required infrastructure (databases)
2. Seed demo data
3. Start the development server

## Demo Credentials

| Field | Value |
|-------|-------|
| Tenant ID | `sandbox-tenant-001` |
| Email | `admin@sandbox.local` |
| Password | `sandbox123` |

## Demo Data

The sandbox includes:

### Users
- **Admin User**: Full access to all modules
- **AP User**: Accounts payable staff with invoice processing permissions
- **Approver**: Can approve invoices up to $5,000
- **Vendor Manager**: Can manage vendor records

### Vendors
- Acme Corporation (Business)
- TechSupplies Inc (Business)
- Office Depot (Business)
- Cloud Services LLC (Business)
- John Smith (Contractor)
- Jane Doe Consulting (Contractor)
- Utilities Co (Business)
- Insurance Corp (Business)

### Invoices
Pre-loaded invoices in various states:
- Pending review
- Pending approval
- Approved
- Ready for payment
- Paid

### Workflow Rules
- Auto-approve invoices under $500
- Require manager approval for invoices over $5,000
- Route utilities to automatic payment queue

## Resetting the Sandbox

```bash
pnpm sandbox:reset
```

This will delete all data and recreate the sandbox from scratch.

## Modules Enabled

All modules are enabled in sandbox mode:
- ✅ Invoice Capture
- ✅ Invoice Processing
- ✅ Vendor Management
- ✅ Reporting

## Testing Specific Scenarios

### Invoice Capture Flow
1. Click "Upload Invoice" on the dashboard
2. Upload a PDF or image
3. Review the OCR-extracted data
4. Make corrections if needed
5. Submit for processing

### Approval Flow
1. Login as admin@sandbox.local
2. Go to Processing > Pending Approvals
3. Review an invoice
4. Approve or reject with comments

### Vendor Management
1. Navigate to Vendors
2. Create a new vendor
3. Upload a W-9 document
4. Add contacts

### Reporting
1. View the Reports dashboard
2. Check aging reports
3. Export data to CSV
