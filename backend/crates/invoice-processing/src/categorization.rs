//! Intelligent invoice categorization using ML-based suggestions
//!
//! Suggests GL codes, departments, and cost centers based on:
//! - Vendor historical patterns
//! - Line item descriptions
//! - Similar invoices

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::feedback_loop::{CorrectionRule, FeedbackLearning};

/// Category suggestion with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySuggestion {
    pub category_type: CategoryType,
    pub value: String,
    pub confidence: f32,
    pub source: SuggestionSource,
    pub reasoning: Option<String>,
}

/// Type of categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CategoryType {
    GlCode,
    Department,
    CostCenter,
}

/// Source of the suggestion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    /// Based on vendor's historical invoices
    VendorHistory,
    /// Based on line item text analysis
    LineItemAnalysis,
    /// Based on similar invoices
    SimilarInvoices,
    /// Default/fallback suggestion
    Default,
}

/// Complete categorization result for an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceCategorization {
    pub invoice_id: Uuid,
    pub gl_code: Option<CategorySuggestion>,
    pub department: Option<CategorySuggestion>,
    pub cost_center: Option<CategorySuggestion>,
    pub overall_confidence: f32,
}

/// Intelligent categorization engine
pub struct CategorizationEngine {
    pool: PgPool,
}

impl CategorizationEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Suggest categories for an invoice
    pub async fn suggest_categories(
        &self,
        tenant_id: &str,
        vendor_id: Option<Uuid>,
        vendor_name: &str,
        line_items: &[LineItemInput],
        total_amount: f64,
    ) -> Result<InvoiceCategorization> {
        let mut suggestions = Vec::new();

        // Strategy 1: Vendor historical patterns (highest confidence)
        if let Some(vid) = vendor_id {
            if let Some(suggestion) = self.suggest_from_vendor_history(tenant_id, vid).await? {
                suggestions.push(suggestion);
            }
        }

        // Strategy 2: Line item text analysis
        if let Some(suggestion) = self.suggest_from_line_items(line_items).await? {
            suggestions.push(suggestion);
        }

        // Strategy 3: Similar invoices by vendor name and amount range
        if let Some(suggestion) = self
            .suggest_from_similar_invoices(tenant_id, vendor_name, total_amount)
            .await?
        {
            suggestions.push(suggestion);
        }

        // Aggregate suggestions by category type
        let gl_code = self
            .pick_best_suggestion(&suggestions, CategoryType::GlCode)
            .await?;
        let department = self
            .pick_best_suggestion(&suggestions, CategoryType::Department)
            .await?;
        let cost_center = self
            .pick_best_suggestion(&suggestions, CategoryType::CostCenter)
            .await?;

        // Apply learned correction rules from user feedback
        let feedback = FeedbackLearning::new(self.pool.clone());
        let correction_rules = feedback
            .get_active_correction_rules(tenant_id)
            .await
            .unwrap_or_default();

        let gl_code = self.apply_correction(&gl_code, &correction_rules);
        let department = self.apply_correction(&department, &correction_rules);
        let cost_center = self.apply_correction(&cost_center, &correction_rules);

        // Calculate overall confidence
        let overall_confidence =
            self.calculate_overall_confidence(&gl_code, &department, &cost_center);

        Ok(InvoiceCategorization {
            invoice_id: Uuid::nil(), // Will be set by caller
            gl_code,
            department,
            cost_center,
            overall_confidence,
        })
    }

    /// Suggest categories based on vendor's historical invoices
    async fn suggest_from_vendor_history(
        &self,
        tenant_id: &str,
        vendor_id: Uuid,
    ) -> Result<Option<CategorySuggestion>> {
        // Get most common GL code, department, cost center for this vendor
        let row = sqlx::query(
            r#"
            SELECT
                gl_code,
                department,
                cost_center,
                COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND vendor_id = $2
            AND gl_code IS NOT NULL
            GROUP BY gl_code, department, cost_center
            ORDER BY usage_count DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let usage_count: i64 = r.try_get("usage_count").unwrap_or(0);
            let confidence = (usage_count as f32 / 10.0).min(0.95); // Cap at 95%

            CategorySuggestion {
                category_type: CategoryType::GlCode, // Primary category
                value: r.try_get("gl_code").unwrap_or_default(),
                confidence,
                source: SuggestionSource::VendorHistory,
                reasoning: Some(format!(
                    "Based on {} previous invoices from this vendor",
                    usage_count
                )),
            }
        }))
    }

    /// Suggest categories based on line item text analysis
    async fn suggest_from_line_items(
        &self,
        line_items: &[LineItemInput],
    ) -> Result<Option<CategorySuggestion>> {
        if line_items.is_empty() {
            return Ok(None);
        }

        // Simple keyword-based categorization
        // In production, this would use an ML model or OpenAI embeddings
        let combined_text = line_items
            .iter()
            .map(|item| item.description.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");

        // Common patterns
        let (category, confidence) = if combined_text.contains("software")
            || combined_text.contains("subscription")
            || combined_text.contains("saas")
            || combined_text.contains("license")
        {
            ("6000-Software & Subscriptions", 0.85)
        } else if combined_text.contains("marketing")
            || combined_text.contains("advertising")
            || combined_text.contains("ad")
        {
            ("7000-Marketing", 0.80)
        } else if combined_text.contains("travel")
            || combined_text.contains("flight")
            || combined_text.contains("hotel")
        {
            ("8000-Travel & Entertainment", 0.80)
        } else if combined_text.contains("office")
            || combined_text.contains("supplies")
            || combined_text.contains("equipment")
        {
            ("5000-Office Supplies & Equipment", 0.75)
        } else if combined_text.contains("consulting")
            || combined_text.contains("professional")
            || combined_text.contains("services")
        {
            ("9000-Professional Services", 0.75)
        } else {
            ("0000-General", 0.40)
        };

        Ok(Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: category.to_string(),
            confidence,
            source: SuggestionSource::LineItemAnalysis,
            reasoning: Some("Based on line item description analysis".to_string()),
        }))
    }

    /// Suggest categories based on similar invoices (same vendor name, similar amount)
    async fn suggest_from_similar_invoices(
        &self,
        tenant_id: &str,
        vendor_name: &str,
        total_amount: f64,
    ) -> Result<Option<CategorySuggestion>> {
        // Find invoices from vendors with similar names and similar amounts (±20%)
        let amount_min = total_amount * 0.8;
        let amount_max = total_amount * 1.2;

        let row = sqlx::query(
            r#"
            SELECT
                gl_code,
                department,
                cost_center,
                COUNT(*) as usage_count
            FROM invoices
            WHERE tenant_id = $1
            AND vendor_name ILIKE $2
            AND total_amount_cents >= $3
            AND total_amount_cents <= $4
            AND gl_code IS NOT NULL
            GROUP BY gl_code, department, cost_center
            ORDER BY usage_count DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(format!("%{}%", vendor_name))
        .bind(amount_min as i64 * 100) // Convert to cents
        .bind(amount_max as i64 * 100)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let usage_count: i64 = r.try_get("usage_count").unwrap_or(0);
            let confidence = (usage_count as f32 / 5.0).min(0.70); // Lower confidence for similarity

            CategorySuggestion {
                category_type: CategoryType::GlCode,
                value: r.try_get("gl_code").unwrap_or_default(),
                confidence,
                source: SuggestionSource::SimilarInvoices,
                reasoning: Some(format!(
                    "Based on {} similar invoices from this vendor",
                    usage_count
                )),
            }
        }))
    }

    /// Pick the best suggestion for a category type
    async fn pick_best_suggestion(
        &self,
        suggestions: &[CategorySuggestion],
        category_type: CategoryType,
    ) -> Result<Option<CategorySuggestion>> {
        // Filter by category type and pick highest confidence
        let matching: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category_type == category_type)
            .collect();

        if matching.is_empty() {
            return Ok(None);
        }

        // Sort by confidence (descending)
        let best = matching
            .into_iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        Ok(Some(best.clone()))
    }

    /// If a correction rule matches the suggestion, swap the value and boost
    /// confidence (users have validated this mapping). Otherwise return as-is.
    fn apply_correction(
        &self,
        suggestion: &Option<CategorySuggestion>,
        rules: &[CorrectionRule],
    ) -> Option<CategorySuggestion> {
        let s = suggestion.as_ref()?;
        let matching_rule = rules
            .iter()
            .find(|r| r.category_type == s.category_type && r.suggested_value == s.value);

        match matching_rule {
            Some(rule) => Some(CategorySuggestion {
                category_type: s.category_type.clone(),
                value: rule.correct_value.clone(),
                confidence: (s.confidence * 1.1).min(0.99),
                source: SuggestionSource::VendorHistory,
                reasoning: Some(format!(
                    "Corrected from '{}' based on {} user corrections",
                    s.value, rule.frequency
                )),
            }),
            None => suggestion.clone(),
        }
    }

    /// Calculate overall confidence score.
    ///
    /// Uses a fixed denominator of 3 so missing fields contribute 0.0 to the
    /// average. This ensures incomplete categorizations (e.g. 1-of-3 fields
    /// at 0.98 confidence) produce a low overall score (0.327) that cannot
    /// reach the 0.95 auto-approval threshold.
    fn calculate_overall_confidence(
        &self,
        gl_code: &Option<CategorySuggestion>,
        department: &Option<CategorySuggestion>,
        cost_center: &Option<CategorySuggestion>,
    ) -> f32 {
        let gl_score = gl_code.as_ref().map_or(0.0, |s| s.confidence);
        let dept_score = department.as_ref().map_or(0.0, |s| s.confidence);
        let cc_score = cost_center.as_ref().map_or(0.0, |s| s.confidence);

        (gl_score + dept_score + cc_score) / 3.0
    }

    // ------------------------------------------------------------------
    // Per-line-item categorization (issue #315)
    // ------------------------------------------------------------------

    /// Suggest GL/department/cost-center for each line item independently.
    ///
    /// For each line:
    ///  1. Look up vendor+description-based correction rules (feedback loop).
    ///  2. Fall back to keyword heuristics (`categorize_line_by_keywords`).
    ///  3. If the line description matches >1 distinct GL signal, or vendor
    ///     history shows a consistent split, emit split suggestions.
    pub async fn suggest_per_line_categorizations(
        &self,
        tenant_id: &str,
        invoice_id: Uuid,
        _vendor_id: Option<Uuid>,
        line_items: &[LineItemInput],
        vendor_history: Option<&VendorHistory>,
        prior_codings: &[PriorLineCoding],
    ) -> Result<PerLineInvoiceCategorization> {
        let feedback = FeedbackLearning::new(self.pool.clone());
        let correction_rules = feedback
            .get_active_correction_rules(tenant_id)
            .await
            .unwrap_or_default();

        let mut lines = Vec::with_capacity(line_items.len());

        for (idx, item) in line_items.iter().enumerate() {
            let line_item_id = format!("line-{}", idx);

            // 1. Try vendor history match
            let mut gl_code = None;
            let mut department = None;
            let mut cost_center = None;
            let mut source = SuggestionSource::Default;
            let mut confidence = 0.40;
            let mut rationale = "Default fallback".to_string();

            if let Some(history) = vendor_history {
                if let Some(prior) = find_matching_prior(&item.description, &history.prior_codings)
                {
                    gl_code = Some(prior.gl_code.clone());
                    department = prior.department.clone();
                    cost_center = prior.cost_center.clone();
                    source = SuggestionSource::VendorHistory;
                    confidence = 0.90;
                    rationale = format!(
                        "Based on prior approved coding for vendor ({})",
                        prior.gl_code
                    );
                }
            }

            // Also check generic prior_codings
            if gl_code.is_none() {
                if let Some(prior) = find_matching_prior(&item.description, prior_codings) {
                    gl_code = Some(prior.gl_code.clone());
                    department = prior.department.clone();
                    cost_center = prior.cost_center.clone();
                    source = SuggestionSource::VendorHistory;
                    confidence = 0.85;
                    rationale = format!("Based on prior approved coding ({})", prior.gl_code);
                }
            }

            // 2. Keyword fallback
            if gl_code.is_none() {
                let kw = categorize_line_by_keywords(&item.description);
                gl_code = Some(kw.gl_code);
                department = kw.department;
                cost_center = kw.cost_center;
                source = SuggestionSource::LineItemAnalysis;
                confidence = kw.confidence;
                rationale = kw.rationale;
            }

            // Apply correction rules
            let gl_code_val = gl_code.unwrap_or_else(|| "0000-General".to_string());
            let corrected_gl = apply_line_correction(&gl_code_val, &correction_rules);

            let (final_gl, final_conf, final_source) = if corrected_gl != gl_code_val {
                (
                    corrected_gl,
                    (confidence * 1.1).min(0.99),
                    SuggestionSource::VendorHistory,
                )
            } else {
                (gl_code_val, confidence, source)
            };

            // 3. Detect splits
            let splits = detect_line_splits(&item.description, item.amount, vendor_history);

            lines.push(LineCategorization {
                line_item_id,
                line_index: idx,
                gl_code: final_gl,
                department,
                cost_center,
                confidence: final_conf,
                rationale,
                source: final_source,
                splits,
            });
        }

        // Overall confidence = mean of line confidences
        let overall_confidence = if lines.is_empty() {
            0.0
        } else {
            lines.iter().map(|l| l.confidence).sum::<f64>() / lines.len() as f64
        };

        Ok(PerLineInvoiceCategorization {
            invoice_id,
            lines,
            overall_confidence,
        })
    }

    /// Look up per-line prior approved codings for a vendor.
    pub async fn vendor_history_lookup(
        &self,
        tenant_id: &str,
        vendor_id: Uuid,
    ) -> Result<VendorHistory> {
        // Fetch prior approved codings from invoices for this vendor.
        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, i64)>(
            r#"
            SELECT
                li->>'description' AS description,
                i.gl_code,
                i.department,
                i.cost_center,
                (li->>'amount_cents')::bigint AS amount_cents
            FROM invoices i, LATERAL jsonb_array_elements(i.line_items) AS li
            WHERE i.tenant_id = $1
              AND i.vendor_id = $2
              AND i.gl_code IS NOT NULL
            ORDER BY i.created_at DESC
            LIMIT 100
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        let prior_codings: Vec<PriorLineCoding> = rows
            .into_iter()
            .map(|(desc, gl, dept, cc, cents)| PriorLineCoding {
                description: desc,
                gl_code: gl,
                department: dept,
                cost_center: cc,
                amount: cents as f64 / 100.0,
            })
            .collect();

        // Detect historical splits: group by description, then look for
        // distinct GL codes with consistent ratios.
        let splits = detect_historical_splits(&prior_codings);

        Ok(VendorHistory {
            vendor_id,
            prior_codings,
            splits,
        })
    }
}

/// Input for line items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItemInput {
    pub description: String,
    pub quantity: Option<f64>,
    pub amount: f64,
}

// ---------------------------------------------------------------------------
// Per-line-item categorization types (issue #315)
// ---------------------------------------------------------------------------

/// A single split allocation within one line item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineSplitSuggestion {
    pub gl_code: String,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub amount: f64,
    pub percentage: f64,
    pub confidence: f64,
    pub rationale: String,
}

/// Per-line categorization result for a single line item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineCategorization {
    pub line_item_id: String,
    pub line_index: usize,
    pub gl_code: String,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub confidence: f64,
    pub rationale: String,
    pub source: SuggestionSource,
    pub splits: Vec<LineSplitSuggestion>,
}

/// Complete per-line categorization result for an entire invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerLineInvoiceCategorization {
    pub invoice_id: Uuid,
    pub lines: Vec<LineCategorization>,
    pub overall_confidence: f64,
}

/// Historical per-line coding from a prior approved invoice.
#[derive(Debug, Clone)]
pub struct PriorLineCoding {
    pub description: String,
    pub gl_code: String,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub amount: f64,
}

/// A historical split ratio observed for a vendor/description pair.
#[derive(Debug, Clone)]
pub struct HistoricalSplit {
    pub gl_code: String,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub ratio: f64,
}

/// Vendor history summary used by per-line categorization.
#[derive(Debug, Clone)]
pub struct VendorHistory {
    pub vendor_id: Uuid,
    pub prior_codings: Vec<PriorLineCoding>,
    pub splits: Vec<HistoricalSplit>,
}

// ---------------------------------------------------------------------------
// Free helper functions used by per-line categorization
// ---------------------------------------------------------------------------

/// Keyword-based GL categorization for a single line description.
pub struct KeywordResult {
    pub gl_code: String,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub confidence: f64,
    pub rationale: String,
}

pub fn categorize_line_by_keywords(description: &str) -> KeywordResult {
    let lower = description.to_lowercase();

    if lower.contains("software")
        || lower.contains("subscription")
        || lower.contains("saas")
        || lower.contains("license")
        || lower.contains("aws")
        || lower.contains("ec2")
        || lower.contains("cloud")
    {
        return KeywordResult {
            gl_code: "6000-Software & Subscriptions".to_string(),
            department: Some("Engineering".to_string()),
            cost_center: None,
            confidence: 0.85,
            rationale: "Keyword match: software/subscription/cloud".to_string(),
        };
    }

    if lower.contains("marketing")
        || lower.contains("advertising")
        || lower.contains(" ad")
        || lower.contains("ads")
    {
        return KeywordResult {
            gl_code: "7000-Marketing".to_string(),
            department: Some("Marketing".to_string()),
            cost_center: None,
            confidence: 0.80,
            rationale: "Keyword match: marketing/advertising".to_string(),
        };
    }

    if lower.contains("travel")
        || lower.contains("flight")
        || lower.contains("hotel")
        || lower.contains("meals")
    {
        return KeywordResult {
            gl_code: "8000-Travel & Entertainment".to_string(),
            department: None,
            cost_center: None,
            confidence: 0.80,
            rationale: "Keyword match: travel/entertainment".to_string(),
        };
    }

    if lower.contains("office")
        || lower.contains("supplies")
        || lower.contains("equipment")
        || lower.contains("chairs")
        || lower.contains("furniture")
    {
        return KeywordResult {
            gl_code: "5000-Office Supplies & Equipment".to_string(),
            department: None,
            cost_center: None,
            confidence: 0.75,
            rationale: "Keyword match: office/supplies/equipment".to_string(),
        };
    }

    if lower.contains("consulting") || lower.contains("professional") || lower.contains("services")
    {
        return KeywordResult {
            gl_code: "9000-Professional Services".to_string(),
            department: None,
            cost_center: None,
            confidence: 0.75,
            rationale: "Keyword match: consulting/services".to_string(),
        };
    }

    KeywordResult {
        gl_code: "0000-General".to_string(),
        department: None,
        cost_center: None,
        confidence: 0.40,
        rationale: "No keyword match; default".to_string(),
    }
}

/// Find the best matching prior coding for a line description.
pub fn find_matching_prior<'a>(
    description: &str,
    codings: &'a [PriorLineCoding],
) -> Option<&'a PriorLineCoding> {
    let lower = description.to_lowercase();
    codings.iter().find(|c| {
        let desc_lower = c.description.to_lowercase();
        // Match if either contains the other or shares key words
        lower.contains(&desc_lower)
            || desc_lower.contains(&lower)
            || shares_significant_words(&lower, &desc_lower)
    })
}

/// Check if two descriptions share at least 2 significant words (>3 chars).
fn shares_significant_words(a: &str, b: &str) -> bool {
    let words_a: Vec<&str> = a.split_whitespace().filter(|w| w.len() > 3).collect();
    let words_b: Vec<&str> = b.split_whitespace().filter(|w| w.len() > 3).collect();
    let shared = words_a.iter().filter(|w| words_b.contains(w)).count();
    shared >= 2
}

/// Apply correction rules to a single line GL code.
pub fn apply_line_correction(gl_code: &str, rules: &[CorrectionRule]) -> String {
    rules
        .iter()
        .find(|r| r.category_type == CategoryType::GlCode && r.suggested_value == gl_code)
        .map(|r| r.correct_value.clone())
        .unwrap_or_else(|| gl_code.to_string())
}

/// Detect whether a line should be split across multiple GL accounts.
///
/// Checks:
///  1. Vendor history splits (consistent ratios from `HistoricalSplit`).
///  2. Compound description containing keywords for >1 distinct GL.
pub fn detect_line_splits(
    description: &str,
    line_amount: f64,
    vendor_history: Option<&VendorHistory>,
) -> Vec<LineSplitSuggestion> {
    // 1. Vendor history splits
    if let Some(history) = vendor_history {
        if !history.splits.is_empty() {
            let total_ratio: f64 = history.splits.iter().map(|s| s.ratio).sum();
            if total_ratio > 0.0 {
                return history
                    .splits
                    .iter()
                    .map(|s| LineSplitSuggestion {
                        gl_code: s.gl_code.clone(),
                        department: s.department.clone(),
                        cost_center: s.cost_center.clone(),
                        amount: (line_amount * s.ratio / total_ratio),
                        percentage: s.ratio / total_ratio,
                        confidence: 0.85,
                        rationale: format!(
                            "Historical vendor split (ratio {:.2})",
                            s.ratio / total_ratio
                        ),
                    })
                    .collect();
            }
        }
    }

    // 2. Compound description heuristic
    let lower = description.to_lowercase();
    let gl_signals = collect_gl_signals(&lower);
    if gl_signals.len() >= 2 {
        let n = gl_signals.len() as f64;
        return gl_signals
            .into_iter()
            .map(|(gl, dept, rationale)| LineSplitSuggestion {
                gl_code: gl,
                department: dept,
                cost_center: None,
                amount: line_amount / n,
                percentage: 1.0 / n,
                confidence: 0.70,
                rationale,
            })
            .collect();
    }

    Vec::new()
}

/// Collect distinct GL signals from a description string.
pub fn collect_gl_signals(lower: &str) -> Vec<(String, Option<String>, String)> {
    let mut signals = Vec::new();

    if lower.contains("software")
        || lower.contains("subscription")
        || lower.contains("saas")
        || lower.contains("license")
        || lower.contains("aws")
        || lower.contains("ec2")
        || lower.contains("cloud")
    {
        signals.push((
            "6000-Software & Subscriptions".to_string(),
            Some("Engineering".to_string()),
            "Software keyword detected".to_string(),
        ));
    }
    if lower.contains("marketing")
        || lower.contains("advertising")
        || lower.contains(" ad")
        || lower.contains("ads")
    {
        signals.push((
            "7000-Marketing".to_string(),
            Some("Marketing".to_string()),
            "Marketing keyword detected".to_string(),
        ));
    }
    if lower.contains("travel") || lower.contains("flight") || lower.contains("hotel") {
        signals.push((
            "8000-Travel & Entertainment".to_string(),
            None,
            "Travel keyword detected".to_string(),
        ));
    }
    if lower.contains("meals") || lower.contains("food") || lower.contains("dining") {
        signals.push((
            "8000-Travel & Entertainment".to_string(),
            None,
            "Meals keyword detected".to_string(),
        ));
    }
    if lower.contains("office")
        || lower.contains("supplies")
        || lower.contains("equipment")
        || lower.contains("chairs")
    {
        signals.push((
            "5000-Office Supplies & Equipment".to_string(),
            None,
            "Office keyword detected".to_string(),
        ));
    }
    if lower.contains("consulting") || lower.contains("professional") || lower.contains("services")
    {
        signals.push((
            "9000-Professional Services".to_string(),
            None,
            "Services keyword detected".to_string(),
        ));
    }

    // Deduplicate by GL code (keep first occurrence)
    let mut seen = std::collections::HashSet::new();
    signals.retain(|(gl, _, _)| seen.insert(gl.clone()));

    signals
}

/// Detect historical split ratios from prior codings.
///
/// Groups prior codings by (description, gl_code), computes relative
/// frequencies, and returns splits when a single description maps to
/// multiple GL codes with a consistent ratio.
pub fn detect_historical_splits(codings: &[PriorLineCoding]) -> Vec<HistoricalSplit> {
    use std::collections::HashMap;

    // Group amounts by GL code across all codings
    let mut gl_totals: HashMap<String, (f64, Option<String>, Option<String>)> = HashMap::new();
    for c in codings {
        let entry = gl_totals.entry(c.gl_code.clone()).or_insert((
            0.0,
            c.department.clone(),
            c.cost_center.clone(),
        ));
        entry.0 += c.amount;
    }

    if gl_totals.len() < 2 {
        return Vec::new();
    }

    let total: f64 = gl_totals.values().map(|(amt, _, _)| *amt).sum();
    if total <= 0.0 {
        return Vec::new();
    }

    gl_totals
        .into_iter()
        .map(|(gl, (amt, dept, cc))| HistoricalSplit {
            gl_code: gl,
            department: dept,
            cost_center: cc,
            ratio: amt / total,
        })
        .collect()
}

/// Persist per-line categorization results into `invoice_line_categorizations`.
///
/// One row per `LineCategorization`. The `splits` column stores the
/// `Vec<LineSplitSuggestion>` as JSONB. All writes are tenant-scoped.
pub async fn persist_line_categorizations(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    categorization: &PerLineInvoiceCategorization,
) -> anyhow::Result<()> {
    for line in &categorization.lines {
        let splits_json = serde_json::to_value(&line.splits).unwrap_or(serde_json::json!([]));
        let source_str = match line.source {
            SuggestionSource::VendorHistory => "vendor_history",
            SuggestionSource::LineItemAnalysis => "line_item_analysis",
            SuggestionSource::SimilarInvoices => "similar_invoices",
            SuggestionSource::Default => "default",
        };

        sqlx::query(
            r#"
            INSERT INTO invoice_line_categorizations (
                tenant_id, invoice_id, line_item_id, line_index,
                suggested_gl_code, suggested_department, suggested_cost_center,
                confidence, rationale, source, splits
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(tenant_id)
        .bind(categorization.invoice_id)
        .bind(&line.line_item_id)
        .bind(line.line_index as i32)
        .bind(&line.gl_code)
        .bind(&line.department)
        .bind(&line.cost_center)
        .bind(line.confidence)
        .bind(&line.rationale)
        .bind(source_str)
        .bind(&splits_json)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to persist line categorization: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_line_item_analysis_software() {
        let items = [LineItemInput {
            description: "Annual software license".to_string(),
            quantity: Some(1.0),
            amount: 1200.0,
        }];

        let engine =
            CategorizationEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let suggestion = engine
            .suggest_from_line_items(&items)
            .await
            .unwrap()
            .expect("software line item should produce a category suggestion");

        assert_eq!(suggestion.category_type, CategoryType::GlCode);
        assert_eq!(suggestion.value, "6000-Software & Subscriptions");
        assert_eq!(suggestion.source, SuggestionSource::LineItemAnalysis);
    }

    #[tokio::test]
    async fn test_apply_correction_rewrites_value_and_boosts_confidence() {
        let engine =
            CategorizationEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let suggestion = Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "6000-Software".to_string(),
            confidence: 0.85,
            source: SuggestionSource::LineItemAnalysis,
            reasoning: Some("keyword match".to_string()),
        });
        let rules = vec![CorrectionRule {
            category_type: CategoryType::GlCode,
            suggested_value: "6000-Software".to_string(),
            correct_value: "6100-SaaS Licenses".to_string(),
            frequency: 7,
        }];

        let result = engine.apply_correction(&suggestion, &rules).unwrap();

        assert_eq!(result.value, "6100-SaaS Licenses");
        assert!(result.confidence > 0.85);
        assert!(result.confidence <= 0.99);
        assert!(result
            .reasoning
            .as_ref()
            .unwrap()
            .contains("user corrections"));
        assert_eq!(result.source, SuggestionSource::VendorHistory);
    }

    #[tokio::test]
    async fn test_apply_correction_no_matching_rule_returns_unchanged() {
        let engine =
            CategorizationEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let suggestion = Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "6000-Software".to_string(),
            confidence: 0.80,
            source: SuggestionSource::LineItemAnalysis,
            reasoning: Some("keyword match".to_string()),
        });
        let rules = vec![CorrectionRule {
            category_type: CategoryType::GlCode,
            suggested_value: "7000-Marketing".to_string(),
            correct_value: "7100-Digital Ads".to_string(),
            frequency: 3,
        }];

        let result = engine.apply_correction(&suggestion, &rules).unwrap();

        assert_eq!(result.value, "6000-Software");
        assert!((result.confidence - 0.80).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_apply_correction_respects_category_type() {
        let engine =
            CategorizationEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let suggestion = Some(CategorySuggestion {
            category_type: CategoryType::Department,
            value: "Engineering".to_string(),
            confidence: 0.75,
            source: SuggestionSource::SimilarInvoices,
            reasoning: Some("similar invoices".to_string()),
        });
        // Rule matches value string but is for GlCode, not Department
        let rules = vec![CorrectionRule {
            category_type: CategoryType::GlCode,
            suggested_value: "Engineering".to_string(),
            correct_value: "6000-Engineering".to_string(),
            frequency: 5,
        }];

        let result = engine.apply_correction(&suggestion, &rules).unwrap();

        // Department suggestion must NOT be rewritten by a GlCode rule
        assert_eq!(result.value, "Engineering");
        assert!((result.confidence - 0.75).abs() < f32::EPSILON);
    }
}
