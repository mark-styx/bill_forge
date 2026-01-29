# dream board

# Architecture
- backend: rust
- frontend: next.js/tailwind css
- database: duckdb for analytical data, postgres for oltp

build everything to work locally, but with the ability to deploy to AWS.
so store docs locally, but assume production will be using s3 for example.

Over document everything! The application should be self explanatory and usage should be so easy for the customer. The LLM chatbot should be named Winston and should have access to all docs and data for the customer.

# Invoice Processing Platform: Mid-Market Prioritization

## TIER 1: MUST-HAVE FEATURES (90%+ Deal Breakers)

### Core OCR & Extraction
- **High-accuracy field extraction** (vendor name, invoice number, amount, date, line items)
- **Multi-format support** (PDF, images, email attachments, scanned documents)
- **Confidence scoring** with clear visual indicators of uncertain extractions
- **Line-item extraction** (item description, quantity, unit price, tax)
- **Automatic vendor matching** to master vendor list

**Why it matters:** Mid-market has 500-2000 invoices/month. Manual data entry is their #1 pain point. 85%+ automation rate is the primary ROI driver.

### Basic ERP Integration
- **SAP** or **Oracle NetSuite** integration (top 2 for mid-market)
- **Dynamics 365 AP** module sync
- **QuickBooks Online** for smaller mid-market companies
- **Sage Intacct** for growing companies
- Real-time GL account and cost center validation
- Automatic GL posting capability
- Vendor master data sync

**Why it matters:** Mid-market already has their ERP. Integration is table-stakes—they won't adopt a disconnected system. 60%+ of deal value is post-integration.

### Approval Workflow Automation
- **Multi-level approval routing** (manager → finance lead → controller)
- **Dollar-amount based rules** (invoices <$5K auto-approve, >$50K require CFO approval)
- **Department/cost-center routing** (each dept head approves their own)
- **Exception-based workflows** (only send mismatches to manual review)
- **Email approvals** with approve/reject buttons (no need to log in)
- **Delegation support** (approvers can delegate when out)
- **SLA tracking** (escalate if not approved within X days)

**Why it matters:** Approval bottlenecks are killing their AP teams. Automation here saves 30-40% of AP labor.

***

## TIER 2: HIGH-VALUE FEATURES (60-80% Influence Purchase Decision)

### Advanced Reporting & Analytics
**Dashboard & KPIs**
- **Invoice processing metrics** - volume processed, automation %, exception rate, cycle time
- **Approval performance** - time in approval queue, approval rate by approver, bottleneck identification
- **Cost analysis** - spend by vendor, cost center, department, GL account
- **Invoice aging report** - overdue invoices, days outstanding, payment status
- **Duplicate detection report** - potential duplicates flagged for investigation
- **Exception dashboard** - summary of all holds and issues requiring action
- **Payment status tracking** - when invoices were paid, early payment discounts captured

**Why it matters:** CFOs and controllers desperately want visibility into AP performance. This is a secondary ROI driver (10-15% of value) and a key selling point to finance teams.

**Compliance & Audit Reporting**
- **Audit trail** - who approved what, when, any changes made
- **Three-way match report** - invoices matched to PO and receipt
- **Tax report** - tax amounts by jurisdiction, VAT/GST handling
- **Variance reports** - invoice amount vs. PO, invoice amount vs. receipt
- **Period-end closing reports** - invoices accrued, posted, pending payment

**Why it matters:** Mid-market has audit requirements (SOX, financial controls). Reporting automates compliance documentation.

**Vendor Intelligence**
- **Vendor spend analysis** - total spend, invoice frequency, payment terms
- **Vendor performance** - invoice accuracy, on-time delivery of invoice, payment status
- **Top vendors report** - Pareto analysis (typically 20% of vendors = 80% of spend)
- **New vendor onboarding tracking**

**Export & BI Integration**
- **Excel export** of any report
- **Scheduled email reports** (weekly/monthly summaries)
- **Power BI / Tableau connectors** for custom dashboards
- **CSV export** for data warehouse loading (Snowflake, Redshift)
- **API access** to reporting data

**Why it matters:** Mid-market finance teams already use BI tools. Easy export/integration is table-stakes.

### PO & Procurement Integration
- **Automatic PO matching** (3-way match: PO, receipt, invoice)
- **PO line-item matching** (invoice amount validates against PO lines)
- **Receipt matching** (when PO receipt exists)
- **PO quantity/price variance detection** (invoice differs from PO)
- **Unmatched invoice detection** (invoice received without corresponding PO)
- **Basic Coupa/Ariba integration** (for larger mid-market)

**Why it matters:** Mid-market procurement teams want control over spending. PO matching prevents unauthorized purchases. Saves 10-15% of AP labor on exception handling.

### Bank & Payment Integration
- **Invoice payment history** (when paid, check/ACH details)
- **Bank reconciliation** integration (payments appear in cash flow)
- **Payment file generation** (create ACH batches for processing)
- **Early payment discount tracking** (capture 2/10 net 30 opportunities)

**Why it matters:** Helps AP manage cash flow and capture savings.

***

## TIER 3: NICE-TO-HAVE FEATURES (30-50% Influence)

### AI/LLM Capabilities
**Vendor Name Disambiguation**
- If extracted vendor name is "ABC Inc" but master list has "ABC Incorporated", auto-match
- Handle common vendor name variations
- Flag suspicious vendor matches for review

**Anomaly Detection**
- Flagging duplicate invoices before approval
- Suspicious patterns (vendor submitting same invoice twice, unusual amounts)
- Fraud indicators (invoice from new vendor with high amount)

**Conversational AI**
- "Show me all invoices from Vendor X" 
- "What's our average payment time to vendors?"
- "Find invoices over $50K without approval"
- **But note:** Mid-market is less interested in ChatGPT-style chat. They want **specific, queryable insights**, not conversational exploration.

**Why it matters:** Nice automation boost, but mid-market prioritizes rule-based automation over AI. They want predictable, auditable workflows.

### Mobile Approval
- **Mobile app for approvals** - push notification when invoice ready, approve from phone
- **Receipt photo capture** - take picture at point of receipt

**Why it matters:** Middle-tier priority. Useful for traveling approvers, but not a must-have.

### Vendor Self-Service Portal
- **Vendors can submit invoices** directly to platform
- **Vendors can check payment status** and invoice history
- **Reduces email ping-pong** on status questions

**Why it matters:** Improves vendor relationships, but lower priority than internal efficiency.

### Advanced Compliance Features
- **Multi-currency support** (if global operations)
- **Tax jurisdiction handling** (VAT, GST, HST for Canadian companies)
- **GDPR/data residency** (EU companies)

**Why it matters:** Only needed if company operates internationally.

***

## TIER 4: FUTURE/DIFFERENTIATION (Lower Priority Today)

### Advanced LLM Integration
- Contract term validation against invoice
- Intelligent exception resolution suggestions
- Invoice content summarization

### Blockchain/Immutability
- Relevant only for highly regulated industries

### Specialized OCR
- Handwriting recognition
- Complex table extraction

***

## RECOMMENDED MID-MARKET GO-TO-MARKET POSITIONING

**Primary Value Proposition:**
> Reduce accounts payable processing cost by 40-50% through intelligent automation, while giving your finance team real-time visibility into spending and approvals.

**Key Selling Points (in priority order):**

1. **Automation Rate** - "Process 80% of invoices with zero manual intervention"
2. **Approval Speed** - "Invoices approved in hours, not days"
3. **Reporting & Visibility** - "Real-time dashboard of AP performance, spending by department, vendor, GL account"
4. **Easy Integration** - "Works with your existing ERP (SAP, NetSuite, Dynamics 365)"
5. **Compliance** - "Audit-ready with complete trail and three-way match validation"
6. **Cost Savings** - "Capture 2% of invoices through duplicate/fraud detection and early payment discounts"

**Sales Talking Points:**
- "Your AP team can focus on vendor relationship management instead of data entry"
- "Finance gets visibility into spending patterns and approval bottlenecks"
- "Typically pays for itself in 4-6 months through labor savings"
- "Works with your current systems—no rip-and-replace"

***

## Feature Implementation Roadmap for Mid-Market Success

**MVP/Phase 1 (Launch):**
- OCR extraction + confidence scoring
- ERP integration (SAP, NetSuite, Dynamics 365)
- Basic approval workflows
- Core dashboard (volume, exceptions, cycle time)

**Phase 2 (Months 3-6):**
- Advanced reporting suite (spend analysis, aging, vendor intelligence)
- PO matching integration
- Email-based approvals
- BI tool exports (Power BI, Tableau)

**Phase 3 (Months 6-12):**
- Anomaly detection & duplicate flagging
- Bank reconciliation integration
- Mobile app
- Payment file generation

**Phase 4 (Year 2+):**
- Conversational AI (specific queries)
- Vendor self-service portal
- Advanced multi-currency/tax handling
- LLM-powered exception resolution

***

## Competitive Differentiation for Mid-Market

**Where you can win against Coupa/Ariba:**
1. **Reporting focus** - mid-market loves analytics (Coupa/Ariba are procurement-first)
2. **Ease of implementation** - 4-6 weeks vs. 4-6 months
3. **Transparent pricing** - per-invoice model vs. per-user licensing
4. **Domain expertise** - vertical-specific solutions (construction, manufacturing, healthcare)
5. **Superior OCR accuracy** - custom-trained models for industry documents

**Where you should match:**
1. ERP integration breadth
2. Approval workflow flexibility
3. Compliance/audit capabilities


# Other
Things to consider

Features

generate mock invoices for the data so the app can actually be tested
the ocr product should have two queues built in

I want a cleaner more concise UI. Very modern looking and brighter. Maybe add customizable color themes that can be set for an organization.
invoice view should have a pdf copy of the invoice and all fields should be editable

- there should be an error queue for invoices that came in unmapped
  - the ocr product should be adding invoices to the ap queue or errors

- queue flow should be customizable
  - default should be accounts payable -> pending approval -> ready for payment -> submitted
- each queue should have customizable assignments
  - in accounts payable if invoice from x vendor arrives assign to ap person a
  - in pending approval if invoice from x vendor arrives assign to registered approver for vendor x
  - in pending approval if invoice for y department arrives assign to registered approver for y department
  - approval/assignment hierarchy can be customizable
    - can require more than one approval
  - ready for payment queue should have an ap manager or assigned lead ap person with bulk submits
    - the rules and automations can be set here as well. Can even set auto-submit for some invoices.
->

Reporting
- A calendar heatmap for invoices, that's filterable by type, vendor etc.


- User adds comments for other users
  - organizational customizable status to append as necessary plus reporting and navigation to these classifications

- edit invoice in list and bulk editing options

- audit log for all actions
  - invoice updates, status changes etc
  - user updates etc.
- Robust workflow automation
  - auto-assign
  - auto-approve
  - etc
- Utility reminders
  - exp. invoice checks etc
- Manual invoice upload
- Auto Vendor Statement Requests and Reviews
- LLM integration that can answer platform and tenant specific data questions

Enterprise
- Bring your own data (BYOD)
    - Customer brings vendors
    - Customer integrates invoice inflow
- Custom integrations with existing systems
    - Salesforce
    - Sage
    - Workday
- Single Sign-On (SSO)
- Custom Audit Requirements
    - retention policies (etc.)
- Connectors to BI Platforms
    - Power BI
    - Tableau
- Multi-Cloud resilience
- Fully Secluded Servers

