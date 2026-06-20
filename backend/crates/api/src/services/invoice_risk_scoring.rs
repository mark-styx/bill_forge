//! Unified invoice risk scoring service (refs #420).
//!
//! Background scorer that runs against every incoming invoice and combines
//! three signal families into a single verdict:
//!   - duplicate risk: 5-signal fuzzy match via DuplicateDetector against the
//!     same tenant's recent invoices, plus a cross-tenant lookup against the
//!     metadata-DB `federated_vendor_signals` k-anonymized aggregate for the
//!     `fake_invoice_pattern` signal type (covers the 'same PDF hash across
//!     tenants on the same vendor' fraud pattern called out in #420 without
//!     ever reading another tenant's raw invoice rows).
//!   - fraud risk (vendor-level run_fraud_guard: domain age, lookalike,
//!     recent bank change, country mismatch)
//!   - amount-spike (z-score of the new invoice's amount against the vendor's
//!     90-day mean / stddev)
//!
//! Scored invoices land in `invoice_risk_verdicts` and, when the tier is
//! `Block`, are routed to the tenant's exception work queue with the evidence
//! JSON attached. Every verdict additionally writes an audit log entry to
//! preserve the platform's transparency pillar.
//!
//! The service is invoked from the invoice ingest handler via `score_and_route`
//! in a spawned task so ingest latency is not affected.

use billforge_analytics::anomaly_detection::{DuplicateDetector, InvoiceRecord};
use billforge_core::{
    domain::{AuditAction, AuditEntry, InvoiceId, ResourceType, VendorId, WorkQueueId},
    traits::{AuditService, WorkQueueRepository},
    types::TenantId,
    Result,
};
use billforge_db::repositories::{AuditRepositoryImpl, WorkflowRepositoryImpl};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::fraud_guard::{self, FraudSignals, RiskLevel};

/// K-anonymity floor for cross-tenant federated signal lookups. Mirrors the
/// network-wide default enforced by the federated_risk module / migration 141.
const CROSS_TENANT_K_ANONYMITY_FLOOR: i64 = 5;

/// Sub-score contributed by a high-confidence cross-tenant duplicate signal.
/// Caps at 1.0 so the duplicate component can never escape its weighted slot.
const CROSS_TENANT_DUPLICATE_SUBSCORE: f32 = 1.0;

// ---------------------------------------------------------------------------
// Configuration constants (composite weights / tier thresholds)
// ---------------------------------------------------------------------------

/// Weighted contribution of each component to the unified score in [0, 1].
const WEIGHT_DUPLICATE: f32 = 0.5;
const WEIGHT_FRAUD: f32 = 0.4;
const WEIGHT_AMOUNT_SPIKE: f32 = 0.1;

/// Tier thresholds. A verdict whose `score` is >= BLOCK is routed to the
/// exception queue; a verdict in [REVIEW, BLOCK) is recorded but not routed.
const TIER_REVIEW_MIN: f32 = 0.4;
const TIER_BLOCK_MIN: f32 = 0.7;

/// Z-score above which the invoice's amount is treated as an outlier vs.
/// the vendor's 90-day history.
const AMOUNT_SPIKE_ZSCORE_THRESHOLD: f64 = 3.0;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    Clear,
    Review,
    Block,
}

impl RiskTier {
    fn from_score(score: f32) -> Self {
        if score >= TIER_BLOCK_MIN {
            RiskTier::Block
        } else if score >= TIER_REVIEW_MIN {
            RiskTier::Review
        } else {
            RiskTier::Clear
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            RiskTier::Clear => "clear",
            RiskTier::Review => "review",
            RiskTier::Block => "block",
        }
    }
}

/// A single duplicate match contributing to the verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateSignal {
    pub existing_invoice_id: String,
    pub score: f64,
    pub vendor: f64,
    pub invoice_number: f64,
    pub amount: f64,
    pub date: f64,
    pub line_item_fingerprint: f64,
}

/// A single fraud-side signal contributing to the verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudSignal {
    pub kind: String,
    pub risk: RiskLevel,
    pub detail: serde_json::Value,
}

/// Amount-spike check vs vendor history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmountSpikeSignal {
    pub vendor_id: Option<Uuid>,
    pub invoice_amount: f64,
    pub vendor_mean_amount: f64,
    pub vendor_stddev_amount: f64,
    pub vendor_sample_size: i64,
    pub zscore: f64,
    pub flagged: bool,
}

/// One k-anonymized aggregate row from the federated_vendor_signals network.
/// Surfaces the 'same PDF hash across tenants on the same vendor' pattern as a
/// duplicate-family signal: when N>=k other tenants have flagged this vendor
/// with `fake_invoice_pattern`, the new invoice inherits a high cross-tenant
/// duplicate sub-score. No raw cross-tenant invoice rows are read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTenantSignal {
    pub signal_type: String,
    pub contributor_count: i64,
    pub weighted_score: f64,
    pub explanation: String,
}

/// The full per-invoice verdict. Persisted (without `verdict_id`) into
/// `invoice_risk_verdicts.evidence` and emitted on the audit-log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskVerdict {
    pub verdict_id: Uuid,
    pub invoice_id: Uuid,
    pub tenant_id: Uuid,
    pub score: f32,
    pub tier: RiskTier,
    pub duplicate_signals: Vec<DuplicateSignal>,
    pub cross_tenant_signals: Vec<CrossTenantSignal>,
    pub fraud_signals: Vec<FraudSignal>,
    pub amount_spike: AmountSpikeSignal,
    pub created_at: DateTime<Utc>,
}

impl RiskVerdict {
    /// Build the evidence JSON that lands in `invoice_risk_verdicts.evidence`
    /// (excludes the verdict_id since that's a column).
    fn evidence_json(&self) -> serde_json::Value {
        serde_json::json!({
            "score": self.score,
            "tier": self.tier.as_db_str(),
            "duplicate_signals": self.duplicate_signals,
            "cross_tenant_signals": self.cross_tenant_signals,
            "fraud_signals": self.fraud_signals,
            "amount_spike": self.amount_spike,
        })
    }
}

// ---------------------------------------------------------------------------
// Scorer
// ---------------------------------------------------------------------------

/// Background scorer that computes the unified verdict for an invoice and
/// (when high-tier) enqueues it on the exception work queue.
///
/// When `metadata_pool` and `network_salt` are both supplied, the scorer also
/// performs a cross-tenant federated lookup against `federated_vendor_signals`
/// to surface 'same vendor, same fake-invoice pattern' signals reported by
/// other tenants (refs #420).
#[derive(Clone)]
pub struct InvoiceRiskScorer {
    metadata_pool: Option<Arc<PgPool>>,
    network_salt: Option<String>,
}

impl InvoiceRiskScorer {
    pub fn new() -> Self {
        Self {
            metadata_pool: None,
            network_salt: None,
        }
    }

    /// Attach the metadata-DB pool and network salt used for the cross-tenant
    /// federated-signal lookup. Without these, the cross-tenant duplicate
    /// signal is silently skipped (the scorer still runs the in-tenant
    /// duplicate + fraud + amount-spike checks). Pass `Some(salt)` only when
    /// the operator has configured `NETWORK_HASH_SALT`; mis-matched salts
    /// land on the wrong hash bucket and produce silent misses.
    pub fn with_federated_network(
        mut self,
        metadata_pool: Arc<PgPool>,
        network_salt: Option<String>,
    ) -> Self {
        self.metadata_pool = Some(metadata_pool);
        self.network_salt = network_salt.filter(|s| !s.trim().is_empty());
        self
    }

    /// Combined orchestrator: score the invoice, persist the verdict, route
    /// to the exception queue if blocking, and write an audit entry.
    pub async fn score_and_route(
        &self,
        tenant_id: &TenantId,
        invoice_id: &InvoiceId,
        pool: Arc<PgPool>,
    ) -> Result<RiskVerdict> {
        let verdict = self.score_invoice(tenant_id, invoice_id, &pool).await?;
        self.persist_verdict(&verdict, &pool).await?;

        if matches!(verdict.tier, RiskTier::Block) {
            if let Err(e) = self.enqueue_exception(tenant_id, invoice_id, &pool).await {
                tracing::warn!(
                    error = %e,
                    invoice_id = %invoice_id,
                    "Failed to enqueue invoice on exception queue"
                );
            }
        }

        self.log_audit(tenant_id, &verdict, &pool).await;
        Ok(verdict)
    }

    /// Run all three signal families and assemble the verdict. Does not
    /// persist or route - callers use `score_and_route` for that.
    pub async fn score_invoice(
        &self,
        tenant_id: &TenantId,
        invoice_id: &InvoiceId,
        pool: &PgPool,
    ) -> Result<RiskVerdict> {
        let invoice = load_invoice_for_scoring(pool, tenant_id, invoice_id).await?;

        let duplicate_signals = compute_duplicate_signals(pool, tenant_id, &invoice).await;
        let dup_subscore = max_duplicate_score(&duplicate_signals);

        let cross_tenant_signals = self
            .compute_cross_tenant_signals(pool, tenant_id, &invoice)
            .await;
        let cross_tenant_subscore = cross_tenant_duplicate_subscore(&cross_tenant_signals);

        let fraud_signals = compute_fraud_signals(pool, tenant_id, &invoice).await;
        let fraud_subscore = fraud_subscore_from_signals(&fraud_signals);

        let amount_spike = compute_amount_spike(pool, tenant_id, &invoice).await;
        let amount_subscore = if amount_spike.flagged { 1.0 } else { 0.0 };

        // The duplicate weight is split across in-tenant + cross-tenant matches:
        // either source alone can saturate the duplicate sub-score, so the
        // strongest of the two drives the composite.
        let combined_dup_subscore = dup_subscore.max(cross_tenant_subscore);

        let composite = WEIGHT_DUPLICATE * combined_dup_subscore
            + WEIGHT_FRAUD * fraud_subscore
            + WEIGHT_AMOUNT_SPIKE * amount_subscore;
        let score = composite.clamp(0.0, 1.0);
        // Tier is the strongest of (composite-score tier, per-component tier).
        // This ensures a single high-confidence signal (e.g. an exact
        // duplicate, or a confirmed bank-change + lookalike vendor) is enough
        // to escalate to Block even if other components contribute nothing.
        let tier = max_tier(
            RiskTier::from_score(score),
            max_tier(
                tier_from_duplicate(combined_dup_subscore),
                max_tier(
                    tier_from_cross_tenant(&cross_tenant_signals),
                    max_tier(
                        tier_from_fraud(&fraud_signals),
                        if amount_spike.flagged {
                            RiskTier::Review
                        } else {
                            RiskTier::Clear
                        },
                    ),
                ),
            ),
        );

        Ok(RiskVerdict {
            verdict_id: Uuid::new_v4(),
            invoice_id: *invoice_id.as_uuid(),
            tenant_id: *tenant_id.as_uuid(),
            score,
            tier,
            duplicate_signals,
            cross_tenant_signals,
            fraud_signals,
            amount_spike,
            created_at: Utc::now(),
        })
    }

    /// K-anonymized cross-tenant lookup against `federated_vendor_signals`.
    /// Returns an empty vec when:
    ///   - the scorer has no metadata pool or network salt wired (configured
    ///     by `with_federated_network` at startup),
    ///   - the vendor's identity tuple cannot be loaded, or
    ///   - no aggregate row meets the k-anonymity floor.
    ///
    /// Only the `fake_invoice_pattern` and `bank_account_change` signal types
    /// are surfaced here, matching the duplicate / fraud dimensions called out
    /// in #420.
    async fn compute_cross_tenant_signals(
        &self,
        tenant_pool: &PgPool,
        tenant_id: &TenantId,
        invoice: &ScoredInvoice,
    ) -> Vec<CrossTenantSignal> {
        let (Some(meta_pool), Some(salt)) =
            (self.metadata_pool.as_ref(), self.network_salt.as_ref())
        else {
            return vec![];
        };

        // Load the vendor identity tuple from the tenant DB. Mirrors the
        // canonical (name, tax_id, bank_account_last_four) used by
        // routes/federated_vendor_risk.rs so both sides hash on the same
        // bucket.
        let (vendor_name, tax_id, bank_last_four) = match invoice.vendor_id {
            Some(vid) => match load_vendor_identity(tenant_pool, tenant_id, vid).await {
                Some(v) => v,
                None => return vec![],
            },
            None => {
                // No FK vendor; fall back to the invoice's free-text vendor
                // name so that uncategorized-by-vendor invoices still hit the
                // hash. tax_id + bank fingerprint are unavailable, so this
                // only matches signals contributed under the name-only tuple.
                (invoice.vendor_name.clone(), None, None)
            }
        };

        let normalized = normalize_vendor_name(&vendor_name);
        let hash = vendor_hash(
            &normalized,
            tax_id.as_deref(),
            bank_last_four.as_deref(),
            salt,
        );

        let rows: Vec<(String, i64, i64, f64)> = match sqlx::query_as(
            r#"SELECT signal_type, signal_count, contributor_count, weighted_score
                 FROM federated_vendor_risk_aggregates
                WHERE vendor_hash = $1
                  AND contributor_count >= $2
                  AND signal_type IN ('fake_invoice_pattern', 'bank_account_change')
                ORDER BY weighted_score DESC"#,
        )
        .bind(&hash)
        .bind(CROSS_TENANT_K_ANONYMITY_FLOOR)
        .fetch_all(&**meta_pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    invoice_id = %invoice.id,
                    "Federated cross-tenant signal lookup failed"
                );
                return vec![];
            }
        };

        rows.into_iter()
            .map(|(signal_type, _signal_count, contributor_count, weighted_score)| {
                let explanation = format!(
                    "{} other tenants in the network have reported a {} for this vendor within the last 90 days.",
                    contributor_count,
                    cross_tenant_signal_label(&signal_type),
                );
                CrossTenantSignal {
                    signal_type,
                    contributor_count,
                    weighted_score,
                    explanation,
                }
            })
            .collect()
    }

    async fn persist_verdict(&self, verdict: &RiskVerdict, pool: &PgPool) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO invoice_risk_verdicts
                (id, tenant_id, invoice_id, score, tier, evidence, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(verdict.verdict_id)
        .bind(verdict.tenant_id)
        .bind(verdict.invoice_id)
        .bind(verdict.score)
        .bind(verdict.tier.as_db_str())
        .bind(verdict.evidence_json())
        .bind(verdict.created_at)
        .execute(pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to persist risk verdict: {}", e))
        })?;
        Ok(())
    }

    /// Route a blocking-tier invoice into the tenant's exception work queue.
    /// Best-effort: if no exception queue exists, the verdict is still recorded
    /// but no queue item is created.
    pub async fn enqueue_exception(
        &self,
        tenant_id: &TenantId,
        invoice_id: &InvoiceId,
        pool: &PgPool,
    ) -> Result<Option<WorkQueueId>> {
        let pool_arc = Arc::new(pool.clone());
        let repo = WorkflowRepositoryImpl::new(pool_arc);
        let exception_queue = WorkQueueRepository::get_by_type(
            &repo,
            tenant_id,
            billforge_core::domain::QueueType::Exception,
        )
        .await?;
        let Some(queue) = exception_queue else {
            tracing::warn!(
                tenant_id = %tenant_id.as_uuid(),
                invoice_id = %invoice_id,
                "No exception queue configured; risk verdict recorded but invoice not routed"
            );
            return Ok(None);
        };
        WorkQueueRepository::move_item(&repo, tenant_id, invoice_id, &queue.id, None).await?;
        Ok(Some(queue.id))
    }

    async fn log_audit(&self, tenant_id: &TenantId, verdict: &RiskVerdict, pool: &PgPool) {
        let entry = AuditEntry::new(
            tenant_id.clone(),
            None,
            AuditAction::Update,
            ResourceType::Invoice,
            verdict.invoice_id.to_string(),
            format!(
                "Invoice risk scored: tier={} score={:.3}",
                verdict.tier.as_db_str(),
                verdict.score
            ),
        )
        .with_metadata(serde_json::json!({
            "kind": "invoice.risk_scored",
            "verdict": verdict.evidence_json(),
        }));
        let audit = AuditRepositoryImpl::new(Arc::new(pool.clone()));
        if let Err(e) = audit.log(entry).await {
            tracing::warn!(
                error = %e,
                invoice_id = %verdict.invoice_id,
                "Failed to log invoice.risk_scored audit entry"
            );
        }
    }
}

impl Default for InvoiceRiskScorer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal slice of an invoice needed for the scorer. We don't need the full
/// domain `Invoice` here, just the fields the three signal families read.
#[derive(Debug, Clone)]
struct ScoredInvoice {
    id: Uuid,
    vendor_id: Option<Uuid>,
    vendor_name: String,
    invoice_number: String,
    invoice_date: Option<chrono::NaiveDate>,
    total_amount_cents: i64,
    line_items: serde_json::Value,
    created_at: DateTime<Utc>,
}

async fn load_invoice_for_scoring(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice_id: &InvoiceId,
) -> Result<ScoredInvoice> {
    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<Uuid>,
            String,
            String,
            Option<chrono::NaiveDate>,
            i64,
            Option<serde_json::Value>,
            DateTime<Utc>,
        ),
    >(
        r#"SELECT id, vendor_id, vendor_name, invoice_number, invoice_date,
                  total_amount_cents, line_items, created_at
             FROM invoices
            WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(*invoice_id.as_uuid())
    .bind(*tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to load invoice for scoring: {}", e))
    })?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Invoice".to_string(),
        id: invoice_id.to_string(),
    })?;

    Ok(ScoredInvoice {
        id: row.0,
        vendor_id: row.1,
        vendor_name: row.2,
        invoice_number: row.3,
        invoice_date: row.4,
        total_amount_cents: row.5,
        line_items: row.6.unwrap_or_else(|| serde_json::json!([])),
        created_at: row.7,
    })
}

/// Build a `DuplicateDetector::InvoiceRecord` from the loaded invoice.
fn to_record(inv: &ScoredInvoice) -> InvoiceRecord {
    let amount = inv.total_amount_cents as f64 / 100.0;
    let invoice_date = inv
        .invoice_date
        .and_then(|d| d.and_hms_opt(12, 0, 0).map(|ndt| ndt.and_utc()))
        .unwrap_or(inv.created_at);

    let line_items: Vec<(String, Option<f64>, Option<f64>)> = inv
        .line_items
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let desc = item.get("description")?.as_str()?.to_string();
                    let qty = item.get("quantity").and_then(|v| v.as_f64());
                    let up = item
                        .get("unit_price_cents")
                        .and_then(|v| v.as_i64())
                        .map(|c| c as f64 / 100.0);
                    Some((desc, qty, up))
                })
                .collect()
        })
        .unwrap_or_default();

    let fp = InvoiceRecord::compute_line_item_fingerprint(&line_items);
    InvoiceRecord {
        invoice_id: inv.id.to_string(),
        vendor_name: inv.vendor_name.clone(),
        amount,
        invoice_date,
        invoice_number: Some(inv.invoice_number.clone()),
        line_item_fingerprint: if fp.is_empty() { None } else { Some(fp) },
    }
}

/// Load the canonical (name, tax_id, bank_account_last_four) tuple used to
/// derive the cross-tenant vendor_hash. Returns None on any DB error so the
/// federated lookup degrades to a silent skip rather than poisoning the
/// scorer.
async fn load_vendor_identity(
    tenant_pool: &PgPool,
    tenant_id: &TenantId,
    vendor_id: Uuid,
) -> Option<(String, Option<String>, Option<String>)> {
    sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT name, tax_id, bank_account_last_four \
           FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(tenant_pool)
    .await
    .ok()
    .flatten()
}

/// Lowercase + collapse-whitespace + strip non-alphanumeric. Mirrors
/// `billforge_vendor_mgmt::federated_risk::normalize_vendor_name` so both
/// sides land on the same hash bucket (the contributor and the reader).
fn normalize_vendor_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_space = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Salted SHA-256 of the canonical vendor tuple `salt|name|tax_id|bank_fp`.
/// Mirrors `billforge_vendor_mgmt::federated_risk::vendor_hash` so contributor
/// and reader land on the same bucket. Inlined here (rather than pulling the
/// vendor-mgmt crate as a dependency) to keep the scorer self-contained.
fn vendor_hash(
    normalized_name: &str,
    tax_id: Option<&str>,
    bank_fingerprint: Option<&str>,
    network_salt: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(network_salt.as_bytes());
    hasher.update(b"|");
    hasher.update(normalized_name.as_bytes());
    hasher.update(b"|");
    hasher.update(tax_id.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(bank_fingerprint.unwrap_or("").as_bytes());
    hex::encode(hasher.finalize())
}

/// Human-readable label for a federated signal type in the cross-tenant
/// explanation sentence. Falls back to the raw token if the row uses a value
/// added after this code was compiled.
fn cross_tenant_signal_label(signal_type: &str) -> &str {
    match signal_type {
        "bank_account_change" => "suspicious bank-account change",
        "ofac_near_match" => "OFAC near-match",
        "fake_invoice_pattern" => "fake-invoice pattern",
        "dispute_rate_high" => "elevated dispute rate",
        other => other,
    }
}

/// Cross-tenant federated signals always carry high confidence: getting past
/// the k-anonymity floor (>=5 distinct contributing tenants) is already a
/// strong prior. We saturate to 1.0 the moment any qualifying signal exists.
fn cross_tenant_duplicate_subscore(signals: &[CrossTenantSignal]) -> f32 {
    if signals.is_empty() {
        0.0
    } else {
        CROSS_TENANT_DUPLICATE_SUBSCORE
    }
}

/// A federated `fake_invoice_pattern` hit goes straight to Block; any other
/// cross-tenant signal that survives the k-anonymity floor escalates to
/// Review. No signals leave the tier untouched.
fn tier_from_cross_tenant(signals: &[CrossTenantSignal]) -> RiskTier {
    if signals.iter().any(|s| s.signal_type == "fake_invoice_pattern") {
        RiskTier::Block
    } else if !signals.is_empty() {
        RiskTier::Review
    } else {
        RiskTier::Clear
    }
}

async fn compute_duplicate_signals(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice: &ScoredInvoice,
) -> Vec<DuplicateSignal> {
    let rows = match sqlx::query_as::<
        _,
        (
            Uuid,
            Option<String>,
            Option<i64>,
            Option<chrono::NaiveDate>,
            Option<serde_json::Value>,
            Option<String>,
        ),
    >(
        r#"SELECT id, invoice_number, total_amount_cents, invoice_date, line_items, vendor_name
             FROM invoices
            WHERE tenant_id = $1
              AND id != $2
              AND created_at > NOW() - INTERVAL '90 days'
            ORDER BY created_at DESC
            LIMIT 200"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(invoice.id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load candidate duplicates");
            return vec![];
        }
    };

    let detector = DuplicateDetector::new(*tenant_id.as_uuid());
    let new_record = to_record(invoice);

    let mut out = Vec::new();
    for (id, inv_num, amt_cents, inv_date, line_items_json, vendor_name) in rows {
        let existing_date = inv_date
            .and_then(|d| d.and_hms_opt(12, 0, 0).map(|ndt| ndt.and_utc()))
            .unwrap_or_else(Utc::now);
        let li_items: Vec<(String, Option<f64>, Option<f64>)> = line_items_json
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let desc = item.get("description")?.as_str()?.to_string();
                        let qty = item.get("quantity").and_then(|v| v.as_f64());
                        let up = item
                            .get("unit_price_cents")
                            .and_then(|v| v.as_i64())
                            .map(|c| c as f64 / 100.0);
                        Some((desc, qty, up))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fp = InvoiceRecord::compute_line_item_fingerprint(&li_items);
        let existing = InvoiceRecord {
            invoice_id: id.to_string(),
            vendor_name: vendor_name.unwrap_or_default(),
            amount: amt_cents.unwrap_or(0) as f64 / 100.0,
            invoice_date: existing_date,
            invoice_number: inv_num,
            line_item_fingerprint: if fp.is_empty() { None } else { Some(fp) },
        };
        let (score, breakdown) = detector.score_pair(&new_record, &existing);
        if score > 0.6 {
            out.push(DuplicateSignal {
                existing_invoice_id: id.to_string(),
                score,
                vendor: *breakdown.get("vendor").unwrap_or(&0.0),
                invoice_number: *breakdown.get("invoice_number").unwrap_or(&0.0),
                amount: *breakdown.get("amount").unwrap_or(&0.0),
                date: *breakdown.get("date").unwrap_or(&0.0),
                line_item_fingerprint: *breakdown
                    .get("line_item_fingerprint")
                    .unwrap_or(&0.0),
            });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn max_duplicate_score(signals: &[DuplicateSignal]) -> f32 {
    signals
        .iter()
        .map(|s| s.score as f32)
        .fold(0.0_f32, f32::max)
        .min(1.0)
}

async fn compute_fraud_signals(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice: &ScoredInvoice,
) -> Vec<FraudSignal> {
    let Some(vendor_uuid) = invoice.vendor_id else {
        return vec![];
    };

    let vendor_row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
        ),
    >(
        r#"SELECT email, address, bank_country
             FROM vendors
            WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(vendor_uuid)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (email, address_json, bank_country) = vendor_row.unwrap_or((None, None, None));

    let vendor_country = address_json
        .as_ref()
        .and_then(|v| v.get("country"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let domain = fraud_guard::extract_domain(email.as_deref(), None);
    let vendor_id_obj = VendorId(vendor_uuid);
    let signals: FraudSignals = fraud_guard::run_fraud_guard(
        tenant_id,
        Some(&vendor_id_obj),
        &invoice.vendor_name,
        &domain,
        vendor_country.as_deref(),
        bank_country.as_deref(),
        pool,
    )
    .await;

    vec![
        FraudSignal {
            kind: "domain_age".to_string(),
            risk: signals.domain_age.risk,
            detail: serde_json::to_value(&signals.domain_age).unwrap_or_default(),
        },
        FraudSignal {
            kind: "lookalike".to_string(),
            risk: signals.lookalike.risk,
            detail: serde_json::to_value(&signals.lookalike).unwrap_or_default(),
        },
        FraudSignal {
            kind: "bank_change".to_string(),
            risk: signals.bank_change.risk,
            detail: serde_json::to_value(&signals.bank_change).unwrap_or_default(),
        },
        FraudSignal {
            kind: "country_mismatch".to_string(),
            risk: signals.country_mismatch.risk,
            detail: serde_json::to_value(&signals.country_mismatch).unwrap_or_default(),
        },
    ]
}

/// Map the overall RiskLevel to a sub-score in [0, 1].
fn fraud_subscore_from_signals(signals: &[FraudSignal]) -> f32 {
    let max_risk = signals
        .iter()
        .map(|s| s.risk)
        .fold(RiskLevel::Low, RiskLevel::max);
    match max_risk {
        RiskLevel::Unknown | RiskLevel::Low => 0.0,
        RiskLevel::Medium => 0.5,
        RiskLevel::High => 1.0,
    }
}

/// Per-component tier promotion: a near-exact duplicate alone is enough to
/// block, a strong-but-imperfect match goes to review.
fn tier_from_duplicate(dup_subscore: f32) -> RiskTier {
    if dup_subscore >= 0.9 {
        RiskTier::Block
    } else if dup_subscore >= 0.6 {
        RiskTier::Review
    } else {
        RiskTier::Clear
    }
}

/// Per-component tier promotion: any High fraud signal blocks; a Medium goes
/// to review; everything else stays clear on the fraud axis.
fn tier_from_fraud(signals: &[FraudSignal]) -> RiskTier {
    let max_risk = signals
        .iter()
        .map(|s| s.risk)
        .fold(RiskLevel::Low, RiskLevel::max);
    match max_risk {
        RiskLevel::High => RiskTier::Block,
        RiskLevel::Medium => RiskTier::Review,
        RiskLevel::Unknown | RiskLevel::Low => RiskTier::Clear,
    }
}

/// Return the stricter of two tiers (Block > Review > Clear).
fn max_tier(a: RiskTier, b: RiskTier) -> RiskTier {
    fn rank(t: RiskTier) -> u8 {
        match t {
            RiskTier::Clear => 0,
            RiskTier::Review => 1,
            RiskTier::Block => 2,
        }
    }
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

async fn compute_amount_spike(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice: &ScoredInvoice,
) -> AmountSpikeSignal {
    let amount = invoice.total_amount_cents as f64 / 100.0;
    let Some(vendor_uuid) = invoice.vendor_id else {
        return AmountSpikeSignal {
            vendor_id: None,
            invoice_amount: amount,
            vendor_mean_amount: 0.0,
            vendor_stddev_amount: 0.0,
            vendor_sample_size: 0,
            zscore: 0.0,
            flagged: false,
        };
    };

    let stats: Option<(Option<f64>, Option<f64>, Option<i64>)> = sqlx::query_as(
        r#"SELECT AVG(total_amount_cents / 100.0)::float8,
                  STDDEV_SAMP(total_amount_cents / 100.0)::float8,
                  COUNT(*)
             FROM invoices
            WHERE tenant_id = $1
              AND vendor_id = $2
              AND id != $3
              AND created_at > NOW() - INTERVAL '90 days'"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(vendor_uuid)
    .bind(invoice.id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (mean, stddev, count) = stats.unwrap_or((None, None, Some(0)));
    let mean = mean.unwrap_or(0.0);
    let stddev = stddev.unwrap_or(0.0);
    let count = count.unwrap_or(0);

    let zscore = if count >= 5 && stddev > 0.0 {
        (amount - mean).abs() / stddev
    } else {
        0.0
    };
    let flagged = zscore > AMOUNT_SPIKE_ZSCORE_THRESHOLD;

    AmountSpikeSignal {
        vendor_id: Some(vendor_uuid),
        invoice_amount: amount,
        vendor_mean_amount: mean,
        vendor_stddev_amount: stddev,
        vendor_sample_size: count,
        zscore,
        flagged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_thresholds_partition_the_score_space() {
        assert_eq!(RiskTier::from_score(0.0), RiskTier::Clear);
        assert_eq!(RiskTier::from_score(0.39), RiskTier::Clear);
        assert_eq!(RiskTier::from_score(0.4), RiskTier::Review);
        assert_eq!(RiskTier::from_score(0.69), RiskTier::Review);
        assert_eq!(RiskTier::from_score(0.7), RiskTier::Block);
        assert_eq!(RiskTier::from_score(1.0), RiskTier::Block);
    }

    #[test]
    fn fraud_subscore_promotes_high_risk_to_one() {
        let high = vec![FraudSignal {
            kind: "lookalike".to_string(),
            risk: RiskLevel::High,
            detail: serde_json::json!({}),
        }];
        assert!((fraud_subscore_from_signals(&high) - 1.0).abs() < 1e-6);

        let medium = vec![FraudSignal {
            kind: "bank_change".to_string(),
            risk: RiskLevel::Medium,
            detail: serde_json::json!({}),
        }];
        assert!((fraud_subscore_from_signals(&medium) - 0.5).abs() < 1e-6);

        let none: Vec<FraudSignal> = vec![];
        assert!((fraud_subscore_from_signals(&none) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn max_duplicate_score_picks_highest() {
        let sigs = vec![
            DuplicateSignal {
                existing_invoice_id: "a".to_string(),
                score: 0.65,
                vendor: 1.0,
                invoice_number: 0.0,
                amount: 0.0,
                date: 0.0,
                line_item_fingerprint: 0.0,
            },
            DuplicateSignal {
                existing_invoice_id: "b".to_string(),
                score: 0.95,
                vendor: 1.0,
                invoice_number: 1.0,
                amount: 1.0,
                date: 1.0,
                line_item_fingerprint: 1.0,
            },
        ];
        assert!((max_duplicate_score(&sigs) - 0.95).abs() < 1e-5);
    }

    #[test]
    fn max_duplicate_score_empty_is_zero() {
        assert_eq!(max_duplicate_score(&[]), 0.0);
    }

    #[test]
    fn cross_tenant_subscore_zero_when_empty() {
        assert_eq!(cross_tenant_duplicate_subscore(&[]), 0.0);
    }

    #[test]
    fn cross_tenant_subscore_saturates_when_present() {
        let sigs = vec![CrossTenantSignal {
            signal_type: "fake_invoice_pattern".into(),
            contributor_count: 7,
            weighted_score: 9.0,
            explanation: "x".into(),
        }];
        assert!((cross_tenant_duplicate_subscore(&sigs) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn tier_from_cross_tenant_promotes_fake_invoice_to_block() {
        let sigs = vec![CrossTenantSignal {
            signal_type: "fake_invoice_pattern".into(),
            contributor_count: 7,
            weighted_score: 9.0,
            explanation: "x".into(),
        }];
        assert_eq!(tier_from_cross_tenant(&sigs), RiskTier::Block);
    }

    #[test]
    fn tier_from_cross_tenant_promotes_bank_change_to_review() {
        let sigs = vec![CrossTenantSignal {
            signal_type: "bank_account_change".into(),
            contributor_count: 5,
            weighted_score: 5.0,
            explanation: "x".into(),
        }];
        assert_eq!(tier_from_cross_tenant(&sigs), RiskTier::Review);
    }

    #[test]
    fn tier_from_cross_tenant_clear_when_empty() {
        assert_eq!(tier_from_cross_tenant(&[]), RiskTier::Clear);
    }

    #[test]
    fn vendor_hash_matches_federated_risk_contract() {
        // Stability: same inputs land on the same bucket.
        let a = vendor_hash("acme inc", Some("12-3456789"), Some("last4-9999"), "salt");
        let b = vendor_hash("acme inc", Some("12-3456789"), Some("last4-9999"), "salt");
        assert_eq!(a, b);
        // Salt rotation reshuffles the hash space.
        let c = vendor_hash("acme inc", Some("12-3456789"), Some("last4-9999"), "other");
        assert_ne!(a, c);
        // Hex-encoded SHA-256 is 64 chars.
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn normalize_vendor_name_strips_punctuation_and_lowercases() {
        assert_eq!(normalize_vendor_name("Acme, Inc."), "acme inc");
        assert_eq!(normalize_vendor_name("  ACME   INC  "), "acme inc");
    }
}
