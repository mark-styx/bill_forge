# EDI Integration Plan - BillForge

## Strategy: API-Based EDI via Middleware

**Do not build an X12 parser.** Use an API-based EDI platform (Stedi, Orderful, or SPS Commerce) as the translation layer. BillForge receives/sends normalized JSON via webhooks. The middleware handles X12 parsing, AS2/SFTP transport, and trading partner management.

This is what every serious AP platform does. The Rust `edi` crate (55K downloads, lightly maintained) is fine for diagnostics but not for production where you need to handle hundreds of trading-partner-specific variants.

---

## Architecture

```
Trading Partners (Walmart, Target, etc.)
        |
        | X12 810/850/856/997 via AS2/SFTP
        v
  ┌─────────────────────┐
  │  EDI Middleware      │  (Stedi / Orderful / SPS Commerce)
  │  - X12 parsing      │
  │  - Partner mgmt     │
  │  - Transport (AS2)  │
  └────────┬────────────┘
           | JSON webhooks + REST API
           v
  ┌─────────────────────┐
  │  BillForge          │
  │  crates/edi/        │  New crate
  │  - Webhook receiver │
  │  - Document mapper  │
  │  - 3-way matching   │
  │  - Ack tracking     │
  └─────────────────────┘
```

---

## What We Build vs Buy

### Buy (EDI Middleware)
- X12 parsing/generation (810, 850, 856, 997)
- EDIFACT support (international)
- AS2/SFTP transport
- Trading partner onboarding + profile management
- ISA/GS envelope handling
- Certificate management
- Compliance validation

### Build (BillForge `crates/edi/`)
- Webhook receiver for inbound documents (normalized JSON)
- Mapper from EDI JSON to BillForge Invoice/Vendor/PO models
- 3-way match engine (PO 850 vs ASN 856 vs Invoice 810)
- Functional acknowledgment (997) state tracking
- Outbound document submission via middleware API
- Trading partner configuration UI
- EDI activity dashboard

---

## Phases

### Phase 1: Foundation + Inbound Invoices (810)
**Goal:** Receive EDI invoices from trading partners and route them into BillForge's existing approval pipeline.

**New crate: `crates/edi/`**
```
crates/edi/
├── Cargo.toml
└── src/
    ├── lib.rs            # Module exports
    ├── config.rs         # EDI provider config (Stedi API key, webhook secret)
    ├── client.rs         # REST client for middleware API (send docs, check status)
    ├── types.rs          # Canonical EDI document types (JSON, not X12)
    ├── mapper.rs         # EDI JSON -> BillForge domain models
    └── webhook.rs        # Webhook signature verification
```

**Types (`types.rs`):**
```rust
pub struct EdiInvoice {
    pub sender_id: String,           // ISA sender qualifier + ID
    pub receiver_id: String,         // ISA receiver
    pub interchange_control: String, // ICN for tracking
    pub invoice_number: String,      // BIG02
    pub invoice_date: NaiveDate,     // BIG01
    pub po_number: Option<String>,   // BIG04
    pub vendor_name: String,         // N1*SE
    pub vendor_id: Option<String>,   // N1*SE identifier
    pub bill_to: Option<EdiParty>,   // N1*BT
    pub remit_to: Option<EdiParty>,  // N1*RI
    pub line_items: Vec<EdiLineItem>,
    pub total_amount_cents: i64,     // TDS01
    pub currency: String,
    pub terms: Option<EdiPaymentTerms>,
    pub due_date: Option<NaiveDate>,
}

pub struct EdiLineItem {
    pub line_number: u32,            // IT101
    pub quantity: f64,               // IT102
    pub unit: String,                // IT103 (EA, CS, LB, etc.)
    pub unit_price_cents: i64,       // IT104
    pub product_id: Option<String>,  // IT106+ (UPC, SKU, etc.)
    pub description: String,         // PID
}

pub enum EdiDocumentType {
    Invoice810,
    PurchaseOrder850,
    ShipNotice856,
    FunctionalAck997,
}

pub enum AckStatus {
    Pending,
    Accepted,
    AcceptedWithErrors,
    Rejected,
}
```

**Mapper (`mapper.rs`):**
- `EdiInvoice` -> `CreateInvoiceInput` (existing BillForge type)
- Auto-match vendor by EDI sender ID or name -> existing `VendorRepository::find_by_name`
- Set `capture_status = Reviewed` (no OCR needed, data is structured)
- Set `processing_status = Submitted` (goes straight to approval pipeline)

**API routes (`api/src/routes/edi.rs`):**
```
POST /api/v1/edi/webhook/inbound      # Receive docs from middleware
GET  /api/v1/edi/documents             # List EDI documents
GET  /api/v1/edi/documents/:id         # Document detail + raw payload
POST /api/v1/edi/connect               # Configure middleware credentials
GET  /api/v1/edi/status                # Connection health
GET  /api/v1/edi/partners              # List trading partners
POST /api/v1/edi/partners              # Add trading partner
```

**Worker job: `EdiProcessInbound`**
- Webhook writes raw payload to storage + enqueues job
- Job parses, maps, creates invoice, links to vendor
- On failure: marks document as `ProcessingFailed` with error detail

**Database (tenant DB):**
```sql
CREATE TABLE IF NOT EXISTS edi_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    document_type VARCHAR(20) NOT NULL,  -- invoice_810, po_850, asn_856, ack_997
    direction VARCHAR(10) NOT NULL,      -- inbound, outbound
    interchange_control VARCHAR(50),
    sender_id VARCHAR(50),
    receiver_id VARCHAR(50),
    status VARCHAR(20) NOT NULL DEFAULT 'received',  -- received, processing, mapped, failed
    invoice_id UUID REFERENCES invoices(id),
    raw_payload JSONB NOT NULL,
    mapped_data JSONB,
    error_message TEXT,
    ack_status VARCHAR(20),              -- pending, accepted, rejected
    ack_received_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS edi_trading_partners (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    edi_qualifier VARCHAR(10),           -- ISA qualifier (01, 08, 12, ZZ, etc.)
    edi_id VARCHAR(50) NOT NULL,         -- ISA sender/receiver ID
    vendor_id UUID REFERENCES vendors(id),
    is_active BOOLEAN NOT NULL DEFAULT true,
    settings JSONB NOT NULL DEFAULT '{}', -- partner-specific config
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, edi_id)
);
```

**Frontend:**
- New "EDI" section under Integrations page
- Trading partner list with connect/configure
- EDI document feed (inbound/outbound with status)
- Activity log showing document flow

**Deliverable:** Invoices from EDI trading partners land in BillForge automatically, matched to vendors, ready for approval. No manual data entry.

---

### Phase 2: Purchase Orders (850) + 3-Way Matching

**Goal:** Track purchase orders and automatically match incoming invoices against POs and receiving data.

**New domain model: `PurchaseOrder`**
```rust
pub struct PurchaseOrder {
    pub id: PurchaseOrderId,
    pub tenant_id: TenantId,
    pub po_number: String,
    pub vendor_id: VendorId,
    pub order_date: NaiveDate,
    pub expected_delivery: Option<NaiveDate>,
    pub status: POStatus,  // Open, PartiallyFulfilled, Fulfilled, Closed, Cancelled
    pub line_items: Vec<POLineItem>,
    pub total_amount: Money,
    pub ship_to: Option<Address>,
    pub notes: Option<String>,
}
```

**3-Way Match Engine (`crates/edi/src/matching.rs`):**
```
PO (850)  ←→  ASN (856)  ←→  Invoice (810)
   |              |              |
   po_number ─────┼──────────── BIG04
   line items ────┼──────────── IT1 segments
   quantities ────┤
                  shipped_qty ── invoiced_qty
                  
Match Result:
  - FullMatch      → auto-approve (within tolerance)
  - PartialMatch   → flag for review (quantity/price variance)
  - NoMatch        → route to exception queue
  - OverBilled     → block, require manager approval
```

**Tolerance configuration per trading partner:**
```rust
pub struct MatchTolerances {
    pub price_variance_pct: f64,     // e.g., 2.0 = allow 2% price difference
    pub quantity_variance_pct: f64,  // e.g., 5.0 = allow 5% qty difference
    pub auto_approve_below: Money,   // Auto-approve matches under this amount
}
```

**Database:**
```sql
CREATE TABLE IF NOT EXISTS purchase_orders ( ... );
CREATE TABLE IF NOT EXISTS po_line_items ( ... );
CREATE TABLE IF NOT EXISTS receiving_records ( ... );  -- from 856 ASN
CREATE TABLE IF NOT EXISTS match_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    po_id UUID REFERENCES purchase_orders(id),
    receiving_id UUID,
    match_type VARCHAR(20) NOT NULL,  -- full, partial, none, over_billed
    price_variance_pct REAL,
    quantity_variance_pct REAL,
    details JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

**Workflow integration:**
- FullMatch invoices bypass approval queue (configurable)
- PartialMatch invoices get routed to exception queue with variance details
- NoMatch invoices flagged for manual PO assignment

**Deliverable:** Automatic 3-way matching with configurable tolerances. Matched invoices auto-approve, exceptions route to the right queue.

---

### Phase 3: Outbound Documents + Acknowledgments

**Goal:** Send documents outbound (payment remittance, PO confirmations) and track 997 acknowledgments.

**Outbound flow:**
1. Invoice approved + paid in BillForge
2. Generate 820 (Payment Remittance) or 997 (Ack)
3. Submit to middleware API as JSON
4. Middleware translates to X12 and delivers via AS2/SFTP
5. Track delivery status

**997 Acknowledgment tracking:**
- State machine: `Sent` -> `AckPending` -> `Accepted` / `AcceptedWithErrors` / `Rejected`
- Auto-retry on rejection (configurable)
- Alert on ack timeout (configurable SLA, default 24h)

**New job types:**
```rust
EdiSendRemittance,    // Push 820 after payment
EdiSendAck,           // Send 997 for received documents
EdiCheckAckStatus,    // Poll for outstanding acks (scheduled)
```

**Deliverable:** Full bidirectional EDI flow. BillForge can receive invoices, match them, approve them, pay them, and send remittance advice back to the trading partner.

---

### Phase 4: Trading Partner Portal + Self-Service

**Goal:** Let vendors onboard themselves for EDI and manage their own profiles.

- Vendor-facing portal for EDI enrollment
- Test transaction flow (send test 810, verify mapping)
- Partner-specific field mapping overrides
- EDI compliance dashboard (success rate, error rate, avg processing time)
- Bulk partner import from existing VAN

**This phase is optional and depends on market demand.**

---

## Effort Estimates

| Phase | Scope | Complexity |
|-------|-------|-----------|
| Phase 1 | Inbound 810, webhook, mapper, trading partners | Medium - follows existing integration pattern |
| Phase 2 | PO model, ASN, 3-way matching engine | High - new domain model + matching logic |
| Phase 3 | Outbound 820, 997 state machine | Medium - reverse of Phase 1 |
| Phase 4 | Vendor portal, self-service onboarding | Medium - mostly frontend |

## EDI Middleware Comparison

| Provider | Model | Strengths | Pricing |
|----------|-------|-----------|---------|
| **Stedi** | API-first | Developer-friendly, JSON in/out, modern | Per-document |
| **Orderful** | API-first | Strong retail network | Per-document |
| **SPS Commerce** | Full VAN | Largest network, enterprise-grade | Monthly + per-doc |
| **Cleo** | Hybrid | On-prem option, complex integrations | Enterprise license |

**Recommendation:** Start with **Stedi** for Phase 1. API-first model fits BillForge's architecture. Switch to SPS Commerce if enterprise customers need their trading partner network.

## Dependencies

- Middleware account (Stedi or equivalent)
- At least one real trading partner willing to test (or use middleware's test mode)
- No new Rust dependencies required beyond `reqwest` (already in workspace)
