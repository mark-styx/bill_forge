# Multi-Tenant Financial SaaS Security Patterns

> Research compilation for BillForge — tenant isolation, audit logging, encryption-at-rest, and access control patterns used by production financial SaaS platforms.

---

## 1. Tenant Isolation Patterns

### 1.1 Schema-Per-Tenant vs Row-Level Security (RLS)

#### Schema-Per-Tenant

Each tenant gets its own PostgreSQL schema (`tenant_123`, `tenant_456`) within a shared database instance.

**Pros:**
- True logical isolation — a `DROP SCHEMA` for one tenant can't affect another
- Per-tenant extensions are possible (custom functions, indexes)
- Easier to migrate a tenant to their own database later
- Natural naming: no `tenant_id` column pollution in every query

**Cons:**
- Schema management overhead — `CREATE SCHEMA`, migrations must run per-schema
- Connection pooling complexity — must set `search_path` per-request
- Cross-tenant reporting requires `dblink` or federated queries
- Upper bound on practical tenant count (~1,000–5,000 schemas before operational pain)

**Pattern:**
```sql
-- Migration runner applies to all tenant schemas
SELECT schema_name FROM tenant_schemas WHERE active = true;
-- For each: SET search_path = tenant_123, public; then run migration
```

```rust
// Connection pool middleware (e.g., r2d2 + custom layer)
fn set_search_path(conn: &mut PgConnection, tenant_id: &str) -> Result<()> {
    sql_query(format!("SET search_path = tenant_{}", tenant_id))
        .execute(conn)?;
    Ok(())
}
```

#### Row-Level Security (RLS) — Recommended for BillForge

PostgreSQL's native RLS attaches policies to tables that filter rows based on a session variable. This is what most modern financial SaaS platforms use (Stripe, Modern Treasury, Ramp).

**How it works:**
```sql
-- 1. Every tenant-scoped table has a tenant_id column
CREATE TABLE invoices (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    vendor_name TEXT NOT NULL,
    amount      NUMERIC(12,2) NOT NULL,
    status      TEXT NOT NULL DEFAULT 'draft',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 2. Enable RLS on the table
ALTER TABLE invoices ENABLE ROW LEVEL SECURITY;

-- 3. Force RLS even for table owner (critical!)
ALTER TABLE invoices FORCE ROW LEVEL SECURITY;

-- 4. Create policies using the session variable
-- Application sets app.current_tenant_id at connection checkout
CREATE POLICY tenant_isolation ON invoices
    USING (tenant_id = current_setting('app.current_tenant_id')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id')::uuid);
```

**For superusers / background workers that need to bypass RLS:**
```sql
CREATE POLICY admin_override ON invoices
    TO db_admin
    USING (true)
    WITH CHECK (true);
```

**Multi-column joins naturally stay isolated:**
```sql
-- This only returns line items for the current tenant's invoices
SELECT * FROM invoice_line_items ili
JOIN invoices i ON i.id = ili.invoice_id
WHERE ili.amount > 1000;
-- RLS on invoices table automatically filters — no tenant_id needed on line_items
```

**However**, every table that is directly queried must have its own policy:
```sql
ALTER TABLE invoice_line_items ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation_invoice_lines ON invoice_line_items
    USING (
        invoice_id IN (
            SELECT id FROM invoices
            WHERE tenant_id = current_setting('app.current_tenant_id')::uuid
        )
    );
```

#### Application-Level Tenant Context Enforcement

The most critical layer. If the application doesn't set `app.current_tenant_id` correctly, RLS blocks everything. If it sets it wrong, data leaks.

**Pattern: Middleware + Connection Wrapper**

```rust
// Axum middleware example
pub struct TenantId(uuid::Uuid);

async fn tenant_isolation_middleware(
    State(pool): State<PgPool>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Extract tenant from JWT claims or subdomain
    let claims = extract_claims(&req)?;
    let tenant_id = claims.tenant_id;

    // 2. Verify user actually belongs to this tenant
    let membership = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM user_tenants 
            WHERE user_id = $1 AND tenant_id = $2 AND active = true
        )"
    )
    .bind(claims.user_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !membership {
        return Err(StatusCode::FORBIDDEN);
    }

    // 3. Attach tenant context to request
    req.extensions_mut().insert(TenantId(tenant_id));

    Ok(next.run(req).await)
}

// Custom PgConnection wrapper that auto-sets search_path equivalent
// via a statement that runs after checkout:
// SET LOCAL app.current_tenant_id = '{tenant_id}';
```

**Pool checkout wrapper (SQLx pattern):**
```rust
// sqlx::Executor custom wrapper
async fn with_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<TenantConnection> {
    let mut conn = pool.acquire().await?;
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{}'",
        tenant_id
    ))
    .execute(&mut *conn)
    .await?;
    Ok(TenantConnection { inner: conn })
}
```

**Defense-in-depth checklist:**
- [ ] RLS enabled + forced on every tenant-scoped table
- [ ] Middleware extracts tenant from authenticated token, not from user input
- [ ] Tenant membership verified in DB (not just trusted from JWT)
- [ ] `SET LOCAL` used (resets at transaction end, no leak between requests)
- [ ] No endpoint accepts `tenant_id` as a query parameter — it comes from auth only
- [ ] Background jobs pass tenant context explicitly (never "default" tenant)
- [ ] Integration tests include cross-tenant data access attempts

#### What Financial Platforms Actually Do

| Platform | Approach | Notes |
|----------|----------|-------|
| **Stripe** | Row-level with sharding | Rows keyed by ` livemode` + merchant; data sharded across DB clusters |
| **Modern Treasury** | Row-level isolation | Per-company rows with RLS-like enforcement at application + DB level |
| **Ramp** | Row-level with tenant_id | All models scoped to `organization_id`; middleware enforces |
| **Brex** | Schema-per-tenant historically, migrated to RLS | Started with schemas for strong isolation, moved to RLS for operational simplicity |
| **Airbase** | Row-level isolation | Standard multi-tenant SaaS pattern with `workspace_id` |
| **Tipalti** | Row-level with additional encryption | Each customer's financial data encrypted with customer-specific key |

**Key insight from Stripe's engineering blog:** Stripe uses a combination of row-level isolation at the DB layer and a custom routing layer that maps (merchant, resource_type) to specific database shards. For a platform at BillForge's scale, RLS within a single database (or a few logical databases) is the right starting point. Sharding becomes necessary at 100M+ rows per table.

---

## 2. Audit Logging Patterns

### 2.1 Immutable Append-Only Audit Log Design

The audit log must be **write-once, read-many**. No updates, no deletes. This is both a security requirement and a regulatory one (SOC 2 CC6.1, SOX, PCI DSS Req 10).

**Schema:**
```sql
CREATE TABLE audit_log_events (
    -- Use a separate sequence to prevent gaps from revealing deleted rows
    event_id      BIGSERIAL PRIMARY KEY,
    
    -- Immutable event data
    event_type    TEXT NOT NULL,        -- e.g., 'invoice.created', 'payment.approved'
    actor_id      UUID NOT NULL,        -- who performed the action
    actor_type    TEXT NOT NULL,        -- 'user', 'service_account', 'system'
    tenant_id     UUID NOT NULL,        -- which tenant context
    resource_type TEXT NOT NULL,        -- 'invoice', 'payment', 'bank_account'
    resource_id   UUID NOT NULL,        -- the affected resource
    
    -- What changed
    action        TEXT NOT NULL,        -- 'create', 'update', 'delete', 'view', 'login'
    changes       JSONB,                -- for updates: {"field": {"old": "draft", "new": "approved"}}
    metadata      JSONB,                -- request IP, user agent, API version
    
    -- Immutability
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    event_hash    TEXT NOT NULL         -- SHA-256 of all fields above for tamper detection
);

-- CRITICAL: Revoke all update/delete permissions
REVOKE UPDATE, DELETE ON audit_log_events FROM PUBLIC;
REVOKE UPDATE, DELETE ON audit_log_events FROM app_service;

-- Only the audit writer role can insert
GRANT INSERT ON audit_log_events TO audit_writer;

-- Read access for compliance/auditors only
GRANT SELECT ON audit_log_events TO compliance_reader;
```

**Tamper detection — event hash:**
```rust
fn compute_event_hash(event: &AuditEvent) -> String {
    let input = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        event.event_type,
        event.actor_id,
        event.actor_type,
        event.tenant_id,
        event.resource_type,
        event.resource_id,
        event.action,
        serde_json::to_string(&event.changes).unwrap_or_default(),
        serde_json::to_string(&event.metadata).unwrap_or_default(),
    );
    sha256_hex(input)
}

// Periodic integrity check job (run daily):
// SELECT event_hash, SHA256(concat all fields) FROM audit_log_events 
// WHERE created_at > now() - interval '1 day'
// Verify no mismatches
```

**Append-only enforcement at the database level:**
```sql
-- Additional protection: trigger that blocks updates/deletes
CREATE OR REPLACE FUNCTION prevent_audit_log_modification()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'Audit log modifications are prohibited';
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_log_no_update
    BEFORE UPDATE ON audit_log_events
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_modification();

CREATE TRIGGER audit_log_no_delete
    BEFORE DELETE ON audit_log_events
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_modification();
```

### 2.2 What Events Must Be Captured

**Tier 1 — Always capture (SOC 2 / PCI DSS baseline):**

| Event Category | Specific Events | Why |
|---------------|-----------------|-----|
| Authentication | Login (success/failure), logout, MFA enrollment/change, password reset, token issuance/revocation | Security monitoring, access reviews |
| Authorization | Role assignment/revocation, permission changes, tenant membership changes | WHO has access to WHAT |
| Data Access | Viewing bank account numbers, downloading invoices, exporting payment files | Financial data access is itself sensitive |
| Data Modification | Create/update/delete on: invoices, payments, bank accounts, vendors, approval chains | Reconstruct state at any point in time |
| Payment Actions | Payment initiated, approved, sent, failed, returned | Financial reconciliation |
| System | Configuration changes, integration webhook updates, API key creation/rotation | Infrastructure integrity |

**Tier 2 — Financial regulation specific (SOX, 7-year retention):**

| Event Category | Specific Events | Regulation |
|---------------|-----------------|------------|
| Approval Chain | Who approved, when, from what IP, what the payment state was at approval time | SOX §404 |
| Segregation Violations | Attempts to self-approve, approver modified their own limits | SOX §404 |
| Bank Communication | ACH file generation, bank API calls (success/failure), reconciliation runs | BAI2/MT940 processing |
| Compliance | Sanctions screening results, OFAC hits, KYC verification results | BSA/AML |

### 2.3 Retention Requirements

| Regulation | Minimum Retention | What's Covered |
|-----------|-------------------|----------------|
| SOC 2 Type II | 1 year (typically) | All access logs, change logs, security events |
| PCI DSS | 1 year | Cardholder data access logs, audit trails |
| SOX / SEC | 7 years | Financial records, approval trails, internal controls evidence |
| BSA/AML | 5 years | Transaction records, customer identification |
| GDPR (EU tenants) | Data minimization — no longer than necessary, but audit logs are typically excepted | Access logs, processing activities |

**Implementation pattern:**
```sql
-- Partition audit_log_events by month for efficient retention management
CREATE TABLE audit_log_events (
    -- ... same columns as above ...
) PARTITION BY RANGE (created_at);

-- Auto-create monthly partitions
CREATE TABLE audit_log_events_2026_04 PARTITION OF audit_log_events
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');

CREATE TABLE audit_log_events_2026_05 PARTITION OF audit_log_events
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');

-- Archive + delete old partitions (e.g., older than 7 years for financial tenants)
-- Move to cold storage (S3 + Glacier) before dropping:
-- pg_dump -t audit_log_events_2019_04 | gzip > s3://audit-archive/2019_04.sql.gz
-- DROP TABLE audit_log_events_2019_04;
```

### 2.4 Event Sourcing Pattern for Audit Trails

For critical financial entities (payments, approvals), use event sourcing alongside the operational database:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum PaymentEvent {
    #[serde(rename = "payment.created")]
    Created {
        payment_id: Uuid,
        invoice_id: Uuid,
        amount: Decimal,
        payee_account_id: Uuid,
        created_by: Uuid,
    },
    #[serde(rename = "payment.submitted_for_approval")]
    SubmittedForApproval {
        payment_id: Uuid,
        submitted_by: Uuid,
        requested_approver_id: Uuid,
    },
    #[serde(rename = "payment.approved")]
    Approved {
        payment_id: Uuid,
        approved_by: Uuid,
        approval_note: Option<String>,
    },
    #[serde(rename = "payment.sent")]
    Sent {
        payment_id: Uuid,
        ach_trace_number: Option<String>,
        sent_at: DateTime<Utc>,
    },
    #[serde(rename = "payment.returned")]
    Returned {
        payment_id: Uuid,
        return_code: String,
        reason: String,
    },
}

// Event store is the audit log — it IS the source of truth
// The operational `payments` table is a READ MODEL (projection)
pub struct PaymentEventStore {
    pool: PgPool,
}

impl PaymentEventStore {
    pub async fn append(&self, event: PaymentEvent, tenant_id: Uuid) -> Result<i64> {
        let event_json = serde_json::to_value(&event)?;
        let event_hash = compute_hash(&event, &tenant_id);
        
        let row: (i64,) = sqlx::query_as(
            r#"INSERT INTO payment_events (tenant_id, event_data, event_hash)
               VALUES ($1, $2, $3) RETURNING sequence_number"#
        )
        .bind(tenant_id)
        .bind(&event_json)
        .bind(&event_hash)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row.0)
    }
    
    pub async fn get_history(&self, payment_id: Uuid, tenant_id: Uuid) -> Result<Vec<PaymentEvent>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            r#"SELECT event_data FROM payment_events
               WHERE tenant_id = $1 AND event_data->>'payment_id' = $2
               ORDER BY sequence_number"#
        )
        .bind(tenant_id)
        .bind(payment_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        
        rows.into_iter()
            .map(|(v,)| serde_json::from_value(v).map_err(Into::into))
            .collect()
    }
}
```

**Why event sourcing for payments specifically:**
- The full history of WHO approved WHAT and WHEN is reconstructable
- You can answer "what did this payment look like at 3:47 PM last Tuesday?" — critical for disputes
- Auditors can verify the approval chain without trusting the current state
- Reconciliation: sum of payment events should match bank statements

---

## 3. Encryption at Rest

### 3.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│                                                              │
│  Bank Account # ──► Tokenizer ──► "tok_abc123" (stored)     │
│  SSN ───► Application-level encryption ──► ciphertext (DB)  │
│  Everything else ──► Disk encryption (EBS/EBS)              │
└─────────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
┌────────────────┐            ┌─────────────────┐
│  AWS KMS /     │            │  PostgreSQL      │
│  HashiCorp     │            │  Full Disk       │
│  Vault Transit │            │  Encryption      │
│  (Envelope     │            │  (AES-256)       │
│   Encryption)  │            │                  │
└────────────────┘            └─────────────────┘
```

**Three layers of encryption (defense in depth):**

| Layer | What It Protects | Implementation |
|-------|-----------------|----------------|
| Disk (full-volume) | All data at rest on disk | AWS EBS encryption, LUKS, or managed DB encryption |
| Database (TDE or column-level) | Sensitive columns | `pgcrypto` + application-level, or Vault Transit |
| Application (field-level) | Specific PII/financial fields before hitting DB | Envelope encryption in application code |

### 3.2 Column-Level Encryption for Financial Data

**Option A: Application-level encryption with Vault Transit (Recommended)**

The application encrypts sensitive fields before writing to PostgreSQL. The database never sees plaintext.

```rust
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use rand::RngCore;

pub struct EncryptionService {
    // DEK cache: tenant_id -> (key, version)
    dek_cache: DashMap<Uuid, (Vec<u8>, u32)>,
    vault_client: VaultClient,
}

#[derive(Serialize, Deserialize)]
pub struct EncryptedField {
    /// Base64-encoded ciphertext
    pub ciphertext: String,
    /// Base64-encoded nonce (12 bytes for AES-256-GCM)
    pub nonce: String,
    /// Vault key version used to encrypt
    pub key_version: u32,
    /// Which tenant's DEK was used (for key isolation)
    pub tenant_key_id: String,
}

impl EncryptionService {
    pub async fn encrypt(&self, plaintext: &str, tenant_id: Uuid) -> Result<EncryptedField> {
        // 1. Get or create DEK for this tenant
        let (dek, version) = self.get_or_create_dek(tenant_id).await?;
        
        // 2. Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // 3. Encrypt
        let cipher = Aes256Gcm::new_from_slice(&dek)
            .map_err(|_| Error::EncryptionError("invalid key length"))?;
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| Error::EncryptionError("encryption failed"))?;
        
        Ok(EncryptedField {
            ciphertext: base64_encode(&ciphertext),
            nonce: base64_encode(&nonce_bytes),
            key_version: version,
            tenant_key_id: format!("tenant_{}", tenant_id),
        })
    }
    
    pub async fn decrypt(&self, encrypted: &EncryptedField, tenant_id: Uuid) -> Result<String> {
        let (dek, _version) = self.get_or_create_dek(tenant_id).await?;
        
        let cipher = Aes256Gcm::new_from_slice(&dek)
            .map_err(|_| Error::EncryptionError("invalid key length"))?;
        
        let nonce = Nonce::from_slice(&base64_decode(&encrypted.nonce));
        let plaintext = cipher.decrypt(nonce, &base64_decode(&encrypted.ciphertext))
            .map_err(|_| Error::EncryptionError("decryption failed"))?;
        
        String::from_utf8(plaintext).map_err(Into::into)
    }
    
    async fn get_or_create_dek(&self, tenant_id: Uuid) -> Result<(Vec<u8>, u32)> {
        // Check cache first
        if let Some(entry) = self.dek_cache.get(&tenant_id) {
            return Ok(entry.value().clone());
        }
        
        // Ask Vault to create a data key (envelope encryption)
        // POST /transit/datakey/plaintext/tenant_{tenant_id}
        // Vault returns: { "plaintext": "<base64 DEK>", "ciphertext": "<wrapped DEK>" }
        // We store the wrapped DEK in DB; cache the plaintext DEK in memory
        let response = self.vault_client
            .create_data_key(&format!("tenant_{}", tenant_id))
            .await?;
        
        let dek = base64_decode(&response.plaintext);
        let version = response.key_version;
        
        self.dek_cache.insert(tenant_id, (dek.clone(), version));
        Ok((dek, version))
    }
}
```

**What to encrypt at the column level:**
- Bank account numbers (full routing + account)
- SSN / EIN / Tax IDs
- Vendor bank credentials
- Authentication tokens / API keys stored in DB
- Signed documents or check images

**What NOT to encrypt at the column level (disk encryption suffices):**
- Invoice amounts, dates, status (need queryability)
- Vendor names (need search)
- Payment IDs (already non-sensitive identifiers)
- Audit log entries (need to be queryable; protect with access controls instead)

### 3.3 Key Management: Envelope Encryption

**Envelope encryption architecture:**

```
┌──────────────────────────────────────────────────────────┐
│                   Envelope Encryption Flow                │
│                                                           │
│  1. App requests Data Encryption Key (DEK) from Vault    │
│     POST /v1/transit/datakey/plaintext/tenant_{id}       │
│                                                           │
│  2. Vault generates DEK, encrypts it with KEK (Key      │
│     Encryption Key), returns:                            │
│     - plaintext DEK (in memory only, never persisted)    │
│     - ciphertext DEK (wrapped, stored in application DB) │
│                                                           │
│  3. App uses plaintext DEK to encrypt sensitive field   │
│                                                           │
│  4. Wrapped DEK stored in DB:                            │
│     tenant_encryption_keys table:                        │
│       tenant_id | wrapped_dek | key_version | created    │
│                                                           │
│  5. On decrypt: App requests Vault to unwrap DEK,       │
│     or uses cached plaintext DEK                        │
└──────────────────────────────────────────────────────────┘

                    ┌──────────────┐
                    │  AWS KMS     │  ◄── KEK (Key Encryption Key)
                    │  or Vault    │       - Managed by security team
                    │  Transit     │       - HSM-backed
                    └──────┬───────┘       - Auto-rotated
                           │
                    encrypts/decrypts
                           │
                    ┌──────▼───────┐
                    │  DEK per     │  ◄── Data Encryption Key
                    │  Tenant      │       - Unique per tenant
                    └──────┬───────┘       - Stored wrapped in app DB
                           │
                    encrypts/decrypts
                           │
                    ┌──────▼───────┐
                    │  Bank Acct   │
                    │  Numbers,    │  ◄── Sensitive fields
                    │  SSNs, etc.  │       - Stored as ciphertext in DB
                    └──────────────┘
```

**AWS KMS approach (simpler, fully managed):**

```rust
use aws_sdk_kms::Client as KmsClient;

pub struct KmsEncryptionService {
    kms: KmsClient,
    key_arn: String, // CMK ARN for this environment
}

impl KmsEncryptionService {
    /// Generate a data key (envelope encryption)
    pub async fn generate_data_key(&self, tenant_context: &str) -> Result<DataKey> {
        let response = self.kms
            .generate_data_key()
            .key_id(&self.key_arn)
            .key_spec(DataKeySpec::Aes256)
            .encryption_context("tenant_id", tenant_context)
            .send()
            .await?;
        
        Ok(DataKey {
            plaintext: response.plaintext.unwrap().into_vec(),
            ciphertext: response.ciphertext_blob.unwrap().into_vec(),
        })
    }
    
    /// Decrypt a wrapped DEK
    pub async fn decrypt_data_key(
        &self, 
        wrapped_dek: &[u8], 
        tenant_context: &str
    ) -> Result<Vec<u8>> {
        let response = self.kms
            .decrypt()
            .ciphertext_blob(wrapped_dek)
            .encryption_context("tenant_id", tenant_context)
            .send()
            .await?;
        
        Ok(response.plaintext.unwrap().into_vec())
    }
    
    /// Encrypt a field directly (for smaller payloads, skip envelope)
    pub async fn encrypt_field(
        &self,
        plaintext: &[u8],
        tenant_context: &str,
    ) -> Result<Vec<u8>> {
        let response = self.kms
            .encrypt()
            .key_id(&self.key_arn)
            .plaintext(plaintext.into())
            .encryption_context("tenant_id", tenant_context)
            .send()
            .await?;
        
        Ok(response.ciphertext_blob.unwrap().into_vec())
    }
}
```

**KMS encryption context is critical for multi-tenant:**
```rust
// The encryption context binds ciphertext to a specific tenant
// Even if an attacker gets the ciphertext, they can't decrypt it
// without providing the correct tenant_id context
let encryption_context = HashMap::from([
    ("tenant_id".to_string(), tenant_id.to_string()),
    ("resource_type".to_string(), "bank_account".to_string()),
]);
```

### 3.4 Key Rotation Strategies

**Multi-tenant key rotation levels:**

| Level | What Rotates | Frequency | Impact |
|-------|-------------|-----------|--------|
| KEK (Master Key) | AWS KMS CMK or Vault transit key | Annually (automatic) | No application changes — DEKs re-wrapped transparently |
| DEK (Data Key) | Per-tenant data encryption key | Every 90 days or on compromise | Must re-encrypt all tenant data with new DEK |
| Field-Level | Individual encrypted values | On access after DEK rotation | Lazy re-encryption pattern |

**Lazy re-encryption pattern (practical for financial SaaS):**

```rust
/// On read: check if the field was encrypted with current DEK version
/// If not, re-encrypt with current DEK and update the row
pub async fn decrypt_and_maybe_rotate(
    &self,
    encrypted: &EncryptedField,
    tenant_id: Uuid,
    current_dek_version: u32,
) -> Result<(String, Option<EncryptedField>)> {
    let plaintext = self.decrypt(encrypted, tenant_id).await?;
    
    if encrypted.key_version < current_dek_version {
        // Re-encrypt with current DEK
        let re_encrypted = self.encrypt(&plaintext, tenant_id).await?;
        Ok((plaintext, Some(re_encrypted)))
    } else {
        Ok((plaintext, None))
    }
}

// In the application layer, after decrypting:
// if let Some(re_encrypted) = maybe_updated {
//     sqlx::query("UPDATE bank_accounts SET encrypted_account_number = $1 WHERE id = $2")
//         .bind(serde_json::to_value(&re_encrypted)?)
//         .bind(account_id)
//         .execute(&pool)
//         .await?;
// }
```

**Bulk rotation job (run quarterly):**
```sql
-- Find all encrypted fields still on old DEK version
SELECT id, tenant_id, encrypted_account_number 
FROM bank_accounts 
WHERE encrypted_account_number->>'key_version' != $1
ORDER BY last_accessed_at DESC
LIMIT 10000;
-- Re-encrypt each, update in batch
```

### 3.5 Tokenization of Bank Account Data

Tokenization replaces sensitive data with a non-reversible, non-sensitive reference (token). Unlike encryption, tokens have no mathematical relationship to the original data.

**When to tokenize vs encrypt:**
- **Tokenize** when: the original value is only needed for payments (sending to bank via Plaid/Modern Treasury API), and you need to pass references around the system safely
- **Encrypt** when: the application needs to display the original value (e.g., "ending in 4321" display), or search by account number

**Tokenization pattern using a vault registry:**

```sql
CREATE TABLE bank_account_tokens (
    token          TEXT PRIMARY KEY,  -- e.g., "tok_prod_a1b2c3d4"
    tenant_id      UUID NOT NULL,
    
    -- The actual sensitive data (encrypted at rest)
    routing_number_encrypted  JSONB NOT NULL,  -- EncryptedField
    account_number_encrypted  JSONB NOT NULL,  -- EncryptedField
    account_type  TEXT NOT NULL,  -- 'checking', 'savings'
    
    -- Metadata (not encrypted)
    bank_name     TEXT,
    display_name  TEXT,  -- "Chase ••••4321"
    
    -- Audit
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by    UUID NOT NULL,
    
    -- RLS
    CONSTRAINT valid_token_format CHECK (token ~ '^tok_[a-z]+_[a-z0-9]+$')
);

ALTER TABLE bank_account_tokens ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_tokens ON bank_account_tokens
    USING (tenant_id = current_setting('app.current_tenant_id')::uuid);
```

```rust
pub struct Tokenizer {
    prefix: String,  // "tok_prod" or "tok_sandbox"
}

impl Tokenizer {
    pub fn tokenize(&self) -> String {
        format!("{}_{}", self.prefix, nanoid!(16, &ALPHABET_LOWER))
    }
    
    /// Resolve token back to bank details (for payment submission)
    pub async fn resolve(
        &self, 
        token: &str, 
        tenant_id: Uuid,
        encryption: &EncryptionService,
    ) -> Result<BankAccountDetails> {
        let row = sqlx::query_as::<_, (serde_json::Value, serde_json::Value)>(
            "SELECT routing_number_encrypted, account_number_encrypted 
             FROM bank_account_tokens 
             WHERE token = $1 AND tenant_id = $2"
        )
        .bind(token)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::TokenNotFound)?;
        
        let routing = encryption.decrypt(
            &serde_json::from_value(row.0)?, 
            tenant_id
        ).await?;
        let account = encryption.decrypt(
            &serde_json::from_value(row.1)?,
            tenant_id
        ).await?;
        
        Ok(BankAccountDetails { routing, account })
    }
}
```

**What Stripe does:** Stripe uses a similar pattern — they tokenize card numbers and bank accounts into `tok_*` or `ba_*` tokens. The actual PAN/account data never touches your servers. Modern Treasury does the same with `bank_account_id` references.

**For BillForge:** Use Plaid/Modern Treasury tokens when possible (they hold the sensitive data). When you must store bank account data directly, use the tokenizer + encryption pattern above.

---

## 4. Access Control Patterns

### 4.1 RBAC Model for AP Automation

Based on patterns from Ramp, Brex, Airbase, and Tipalti:

```
┌────────────────────────────────────────────────────────────┐
│                    Role Hierarchy                          │
│                                                            │
│  Owner ──── Can do everything + manage billing            │
│    │                                                       │
│  Admin ──── Manage users, roles, integrations, all data   │
│    │                                                       │
│  Controller ── View all financial data, approve payments  │
│    │              Manage vendors, GL codes                 │
│    │                                                       │
│  Approver ─── View assigned invoices, approve/reject      │
│    │              No access to admin settings              │
│    │                                                       │
│  Bookkeeper ── Create/edit invoices, manage GL codes      │
│    │               Cannot approve payments                 │
│    │                                                       │
│  Employee ──── Submit expenses, view own submissions      │
│    │               Cannot view company financials          │
│    │                                                       │
│  Auditor ───── Read-only access to all financial data     │
│                + full audit log access                     │
│                Cannot modify anything                      │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

**Permission matrix (stored in DB, not hardcoded):**

```sql
CREATE TABLE roles (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    name        TEXT NOT NULL,           -- 'admin', 'approver', etc.
    description TEXT,
    is_system   BOOLEAN DEFAULT false,   -- system roles can't be deleted
    created_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE(tenant_id, name)
);

CREATE TABLE permissions (
    id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code TEXT UNIQUE NOT NULL   -- 'invoices.create', 'payments.approve', etc.
);

CREATE TABLE role_permissions (
    role_id       UUID REFERENCES roles(id),
    permission_id UUID REFERENCES permissions(id),
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE user_roles (
    user_id  UUID NOT NULL REFERENCES users(id),
    role_id  UUID NOT NULL REFERENCES roles(id),
    tenant_id UUID NOT NULL,
    assigned_by UUID NOT NULL,
    assigned_at TIMESTAMPTZ DEFAULT now(),
    PRIMARY KEY (user_id, role_id, tenant_id)
);
```

**Permission namespacing by resource:**

```
invoices.{create, read, update, delete, approve, export}
payments.{create, read, approve, cancel, export}
vendors.{create, read, update, delete}
bank_accounts.{create, read, update, delete, verify}
users.{create, read, update, delete, manage_roles}
audit_log.{read}
reports.{read, export}
integrations.{create, read, update, delete, configure_webhooks}
settings.{read, update_billing, update_payment_settings}
```

**Permission enforcement middleware:**

```rust
#[derive(Debug, Clone)]
pub struct Permissions(Vec<String>);

impl Permissions {
    pub fn has(&self, permission: &str) -> bool {
        self.0.iter().any(|p| {
            p == permission || p.ends_with(".*") && permission.starts_with(&p[..p.len()-1])
        })
    }
    
    pub fn has_all(&self, permissions: &[&str]) -> bool {
        permissions.iter().all(|p| self.has(p))
    }
}

// Axum extractor
async fn require_permission(
    State(pool): State<PgPool>,
    extensions: Extensions,
    required_permission: &str,
) -> Result<(), AppError> {
    let tenant_id = extensions.get::<TenantId>()
        .ok_or(AppError::Internal("missing tenant context"))?;
    let user_id = extensions.get::<UserId>()
        .ok_or(AppError::Internal("missing user context"))?;
    
    let permissions: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT p.code
           FROM role_permissions rp
           JOIN roles r ON r.id = rp.role_id
           JOIN permissions p ON p.id = rp.permission_id
           JOIN user_roles ur ON ur.role_id = r.id
           WHERE ur.user_id = $1 AND ur.tenant_id = $2"#
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_all(&pool)
    .await?;
    
    let perms = Permissions(permissions);
    if !perms.has(required_permission) {
        return Err(AppError::Forbidden);
    }
    
    Ok(())
}

// Usage in handler:
// async fn approve_payment(
//     _perm: RequirePermission<"payments.approve">,  // compile-time check
//     ...
// )
```

### 4.2 Segregation of Duties in Payment Workflows

This is a **hard SOX requirement**. The person who creates a payment cannot be the person who approves it.

```sql
CREATE TABLE payment_approvals (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id      UUID NOT NULL REFERENCES payments(id),
    approver_id     UUID NOT NULL REFERENCES users(id),
    approval_level  INT NOT NULL DEFAULT 1,  -- for multi-level approval
    decision        TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    reason          TEXT,
    ip_address      INET,
    user_agent      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    UNIQUE(payment_id, approver_id, approval_level)
);

-- Application-level enforcement (not just DB constraint):
-- Check at payment creation time + approval time
```

```rust
#[derive(Debug)]
pub enum PaymentAmountTier {
    /// Single approver
    Small,    // <$5,000
    /// One controller + one admin
    Medium,   // $5,000 - $50,000
    /// Two controllers + CFO
    Large,    // $50,000 - $500,000
    /// Full approval chain: controller + admin + CFO + CEO
    Enterprise, // >$500,000
}

impl PaymentAmountTier {
    pub fn from_amount(amount: Decimal, tenant_settings: &TenantSettings) -> Self {
        if amount < tenant_settings.tier1_threshold {
            Self::Small
        } else if amount < tenant_settings.tier2_threshold {
            Self::Medium
        } else if amount < tenant_settings.tier3_threshold {
            Self::Large
        } else {
            Self::Enterprise
        }
    }
    
    pub fn required_approvals(&self) -> Vec<ApprovalRequirement> {
        match self {
            Self::Small => vec![
                ApprovalRequirement { role: "approver", level: 1 },
            ],
            Self::Medium => vec![
                ApprovalRequirement { role: "approver", level: 1 },
                ApprovalRequirement { role: "controller", level: 2 },
            ],
            Self::Large => vec![
                ApprovalRequirement { role: "controller", level: 1 },
                ApprovalRequirement { role: "admin", level: 2 },
                ApprovalRequirement { role: "cfo", level: 3 },
            ],
            Self::Enterprise => vec![
                ApprovalRequirement { role: "controller", level: 1 },
                ApprovalRequirement { role: "admin", level: 2 },
                ApprovalRequirement { role: "cfo", level: 3 },
                ApprovalRequirement { role: "owner", level: 4 },
            ],
        }
    }
}

#[derive(Debug)]
pub struct ApprovalError;

impl PaymentService {
    pub async fn approve_payment(
        &self,
        payment_id: Uuid,
        approver_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<(), ApprovalError> {
        let payment = self.get_payment(payment_id, tenant_id).await?;
        
        // RULE 1: Creator cannot approve
        if payment.created_by == approver_id {
            return Err(ApprovalError::SelfApproval);
        }
        
        // RULE 2: Approver must have the required role for this level
        let approver_roles = self.get_user_roles(approver_id, tenant_id).await?;
        let tier = PaymentAmountTier::from_amount(payment.amount, &self.tenant_settings);
        let requirements = tier.required_approvals();
        
        // Find the next unfulfilled approval level
        let existing_approvals = self.get_approvals(payment_id).await?;
        let next_level = existing_approvals.len() as u32 + 1;
        
        if let Some(req) = requirements.get(next_level as usize - 1) {
            if !approver_roles.contains(&req.role.to_string()) {
                return Err(ApprovalError::InsufficientRole);
            }
        }
        
        // RULE 3: Same person can't approve at multiple levels
        if existing_approvals.iter().any(|a| a.approver_id == approver_id) {
            return Err(ApprovalError::AlreadyApproved);
        }
        
        // Record the approval (also goes to audit log)
        self.record_approval(payment_id, approver_id, next_level).await?;
        self.emit_audit_event(AuditEvent::payment_approved(payment_id, approver_id)).await?;
        
        // Check if all approvals are complete → auto-release for payment
        if (existing_approvals.len() + 1) >= requirements.len() {
            self.schedule_payment(payment_id).await?;
        }
        
        Ok(())
    }
}
```

### 4.3 Service Account and API Key Management

```sql
CREATE TABLE api_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    name        TEXT NOT NULL,           -- "QuickBooks Integration"
    key_hash    TEXT NOT NULL,           -- SHA-256 of the actual key
    key_prefix  TEXT NOT NULL,           -- First 8 chars for identification: "bk_live_a1b2c3"
    permissions JSONB NOT NULL DEFAULT '[]', -- ["invoices.read", "payments.create"]
    
    -- Lifecycle
    created_by  UUID NOT NULL,
    last_used_at TIMESTAMPTZ,
    expires_at  TIMESTAMPTZ,             -- NULL = no expiry (audit flags these)
    revoked_at  TIMESTAMPTZ,
    revoked_by  UUID,
    
    created_at  TIMESTAMPTZ DEFAULT now()
);

-- Key format: bk_live_<32_random_chars>  (production)
--             bk_test_<32_random_chars>  (sandbox)
-- Total: 40 chars; we store only the hash, never the raw key
```

```rust
pub struct ApiKeyService;

impl ApiKeyService {
    pub fn generate_key() -> (String, String, String) {
        let raw = format!("bk_live_{}", nanoid!(32, &ALPHABET_ALPHANUMERIC));
        let hash = sha256_hex(&raw);
        let prefix = raw[..16].to_string(); // "bk_live_a1b2c3"
        (raw, hash, prefix)  // raw returned once, never stored
    }
    
    pub async fn authenticate(
        &self, 
        raw_key: &str, 
        pool: &PgPool
    ) -> Result<ApiKeyContext, AuthError> {
        let hash = sha256_hex(raw_key);
        
        let key = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE key_hash = $1 AND revoked_at IS NULL"
        )
        .bind(&hash)
        .fetch_optional(pool)
        .await?
        .ok_or(AuthError::InvalidKey)?;
        
        // Check expiry
        if let Some(expires) = key.expires_at {
            if expires < Utc::now() {
                return Err(AuthError::KeyExpired);
            }
        }
        
        // Update last_used_at asynchronously
        let key_id = key.id;
        tokio::spawn(async move {
            sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
                .bind(key_id)
                .execute(pool)
                .await
                .ok();
        });
        
        Ok(ApiKeyContext {
            tenant_id: key.tenant_id,
            key_id: key.id,
            key_name: key.name,
            permissions: key.permissions,
            actor_type: "service_account",
        })
    }
}
```

### 4.4 Session Management in Multi-Tenant Context

```rust
/// Session token (JWT or opaque) must include:
/// - sub: user_id
/// - tid: tenant_id (critical for multi-tenant)
/// - roles: [role1, role2]
/// - iat, exp: issued at, expiry
/// - jti: unique token ID for revocation
/// - iss: "billforge"

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,           // user_id
    pub tid: Uuid,           // tenant_id — NEVER derived from subdomain
    pub roles: Vec<String>,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,         // for token revocation list
    pub iss: String,
}

/// Session validation flow:
/// 1. Extract JWT from Authorization header
/// 2. Verify signature (HMAC-SHA256 or RSA)
/// 3. Check exp > now()
/// 4. Check jti NOT in revoked_tokens set (Redis SET with TTL matching exp)
/// 5. Extract tid (tenant_id) — this becomes the RLS context
/// 6. Verify user still belongs to tenant (DB check, cached with short TTL)
/// 7. Set app.current_tenant_id in DB connection

/// Token revocation on:
/// - User logout
/// - User removed from tenant
/// - Tenant suspended
/// - Password change
/// - Security event (suspicious login → revoke all user sessions)

pub struct SessionRevocationCache {
    redis: RedisPool,
}

impl SessionRevocationCache {
    pub async fn revoke_token(&self, jti: &str, exp_at: i64) -> Result<()> {
        let ttl = exp_at - Utc::now().timestamp();
        if ttl > 0 {
            let mut conn = self.redis.get().await?;
            redis::cmd("SADD")
                .arg(format!("revoked_tokens:{}", jti))
                .arg("1")
                .query_async::<_, ()>(&mut conn)
                .await?;
            redis::cmd("EXPIRE")
                .arg(format!("revoked_tokens:{}", jti))
                .arg(ttl)
                .query_async::<_, ()>(&mut conn)
                .await?;
        }
        Ok(())
    }
    
    pub async fn revoke_all_user_sessions(&self, user_id: Uuid) -> Result<()> {
        // Set a global revocation flag for the user
        let mut conn = self.redis.get().await?;
        redis::cmd("SET")
            .arg(format!("user_revoked:{}", user_id))
            .arg(Utc::now().timestamp())
            .arg("EX")
            .arg(86400) // 24 hours — any JWT issued before this is invalid
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }
}
```

---

## 5. Summary: Recommended Architecture for BillForge

### Database Layer
| Concern | Pattern | Implementation |
|---------|---------|----------------|
| Tenant Isolation | Row-Level Security | `current_setting('app.current_tenant_id')` on every tenant-scoped table |
| Audit Logging | Append-only partitioned table | Monthly partitions, 7-year retention, SHA-256 event hashing |
| Event Sourcing | For payments + approvals | `payment_events` table as immutable event log |
| Sensitive Data | Tokenized + encrypted | Bank accounts: `tok_*` tokens, encrypted account numbers |
| Key Management | AWS KMS envelope encryption | Per-tenant DEK, KMS CMK for KEK, encryption context binding |

### Application Layer
| Concern | Pattern | Implementation |
|---------|---------|----------------|
| Tenant Context | Middleware + DB connection wrapper | JWT → tenant extraction → `SET LOCAL app.current_tenant_id` |
| RBAC | Permission-based access control | `roles` → `permissions` matrix, enforced in middleware |
| Segregation of Duties | Tiered approval chains | Amount-based approval tiers, self-approval prevention |
| API Keys | Hash-only storage, scoped permissions | `bk_live_*` keys, SHA-256 hash stored, prefix for identification |
| Sessions | JWT + Redis revocation | Tenant ID in claims, revocation on security events |

### Infrastructure Layer
| Concern | Pattern |
|---------|---------|
| Encryption at Rest | Full-disk (EBS/managed) + column-level (KMS/Vault) |
| Key Rotation | Annual KEK rotation (auto), quarterly DEK rotation (lazy re-encrypt) |
| Secrets Management | HashiCorp Vault or AWS Secrets Manager for DB creds, API keys |
| Network | VPC isolation, private subnets for DB, TLS everywhere |

---

## Sources & References

- **PostgreSQL Row-Level Security**: [PostgreSQL Docs — DDL Row Security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- **HashiCorp Vault Transit Engine**: [Vault Transit Secrets Engine](https://developer.hashicorp.com/vault/docs/secrets/transit)
- **AWS KMS Envelope Encryption**: [AWS KMS Developer Guide — Envelope Encryption](https://docs.aws.amazon.com/kms/latest/developerguide/enveloping.html)
- **Stripe Engineering Blog**: Stripe uses row-level isolation with sharding; API design patterns for tokenization
- **Modern Treasury**: Bank account tokenization (`bank_account_id` references); payment rail abstraction
- **Ramp Engineering**: Organization-scoped RBAC; `organization_id` on all models
- **SOC 2 CC6.1**: Logical and physical access controls; audit trail requirements
- **PCI DSS Requirement 10**: Audit trail requirements for cardholder data environments
- **SOX Section 404**: Internal controls over financial reporting; segregation of duties
