//! Contract-aware matching for recurring non-PO spend.
//!
//! Matches incoming invoices (without a PO) against stored vendor contracts
//! that define a fixed monthly amount, optional annual escalator, and term.
//! In-band invoices flow to touchless approval; out-of-band invoices become
//! exceptions.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Input required to match an invoice against a contract.
#[derive(Debug, Clone)]
pub struct ContractMatchInput {
    pub tenant_id: Uuid,
    pub vendor_id: Uuid,
    pub invoice_date: NaiveDate,
    /// Invoice total in dollars (e.g. 1000.00).
    pub amount: f64,
    pub currency: String,
}

/// Outcome of attempting to match an invoice to a contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum ContractMatchOutcome {
    /// Invoice amount within tolerance of expected contract amount.
    InBand {
        contract_id: Uuid,
        expected: f64,
        variance_pct: f64,
    },
    /// Invoice amount outside tolerance of expected contract amount.
    OutOfBand {
        contract_id: Uuid,
        expected: f64,
        variance_pct: f64,
    },
    /// A contract exists but the invoice date is past the contract end date.
    Expired { contract_id: Uuid },
    /// No active contract found for this tenant+vendor.
    NoActiveContract,
}

/// Internal representation of a contract row. Uses f64 for NUMERIC columns
/// (SQL casts to DOUBLE PRECISION to avoid needing the bigdecimal sqlx feature).
///
/// `escalator_pct` and `tolerance_pct` are in **percentage units** (e.g. 3.0 = 3%).
#[derive(Debug, sqlx::FromRow)]
pub struct ContractRow {
    pub id: Uuid,
    pub monthly_amount: f64,
    pub escalator_pct: f64,
    pub escalator_anniversary_month: Option<i16>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub tolerance_pct: f64,
}

/// Match an invoice against active contracts for the given tenant+vendor.
///
/// Steps:
/// 1. Query active contracts where `invoice_date` falls within `[start_date, end_date]`.
/// 2. If none found, check for expired contract (past `end_date`).
/// 3. Compute expected amount with integer-year compounding of the escalator.
/// 4. Compare variance against tolerance.
/// 5. Persist the result into `contract_matches`.
pub async fn match_invoice_to_contract(
    pool: &PgPool,
    input: &ContractMatchInput,
    invoice_id: Uuid,
) -> Result<ContractMatchOutcome> {
    // 1. Find active contracts covering the invoice date.
    let active = sqlx::query_as::<_, ContractRow>(
        r#"SELECT id,
                  CAST(monthly_amount AS DOUBLE PRECISION) AS monthly_amount,
                  CAST(escalator_pct AS DOUBLE PRECISION) AS escalator_pct,
                  escalator_anniversary_month,
                  start_date, end_date,
                  CAST(tolerance_pct AS DOUBLE PRECISION) AS tolerance_pct
           FROM contracts
           WHERE tenant_id = $1
             AND vendor_id = $2
             AND status = 'active'
             AND start_date <= $3
             AND end_date >= $3
           ORDER BY start_date DESC
           LIMIT 1"#,
    )
    .bind(input.tenant_id)
    .bind(input.vendor_id)
    .bind(input.invoice_date)
    .fetch_optional(pool)
    .await
    .map_err(|e| anyhow::anyhow!("Failed to query contracts: {}", e))?;

    let contract = match active {
        Some(c) => c,
        None => {
            // Check for expired contract.
            let expired = sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM contracts
                   WHERE tenant_id = $1
                     AND vendor_id = $2
                     AND status = 'active'
                     AND end_date < $3
                   LIMIT 1"#,
            )
            .bind(input.tenant_id)
            .bind(input.vendor_id)
            .bind(input.invoice_date)
            .fetch_optional(pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query expired contracts: {}", e))?;

            return match expired {
                Some(cid) => {
                    persist_match(pool, invoice_id, cid, None, None, None, "expired").await?;
                    Ok(ContractMatchOutcome::Expired { contract_id: cid })
                }
                None => Ok(ContractMatchOutcome::NoActiveContract),
            };
        }
    };

    // 3. Compute expected amount with escalator.
    let expected = compute_expected_amount(&contract, input.invoice_date);

    // 4. Compute variance percentage.
    let variance_pct = if expected == 0.0 {
        100.0
    } else {
        ((input.amount - expected) / expected) * 100.0
    };

    let abs_variance = variance_pct.abs();

    let match_result = if abs_variance <= contract.tolerance_pct {
        "in_band"
    } else {
        "out_of_band"
    };

    let outcome = if match_result == "in_band" {
        ContractMatchOutcome::InBand {
            contract_id: contract.id,
            expected,
            variance_pct,
        }
    } else {
        ContractMatchOutcome::OutOfBand {
            contract_id: contract.id,
            expected,
            variance_pct,
        }
    };

    // 5. Persist.
    persist_match(
        pool,
        invoice_id,
        contract.id,
        Some(expected),
        Some(input.amount),
        Some(variance_pct),
        match_result,
    )
    .await?;

    Ok(outcome)
}

/// Compute expected monthly amount applying integer-year compounding of the
/// escalator. Only full years since the contract start (or since the last
/// anniversary month) count.
pub fn compute_expected_amount(contract: &ContractRow, invoice_date: NaiveDate) -> f64 {
    let anniversary_month = contract
        .escalator_anniversary_month
        .unwrap_or(contract.start_date.month() as i16) as u32;

    let mut years_elapsed: i32 = invoice_date.year() - contract.start_date.year();

    let invoice_month = invoice_date.month();
    if invoice_month < anniversary_month {
        years_elapsed -= 1;
    } else if invoice_month == anniversary_month {
        let anniversary_day = contract.start_date.day();
        if invoice_date.day() < anniversary_day {
            years_elapsed -= 1;
        }
    }

    let years_elapsed = years_elapsed.max(0);

    // base * (1 + escalator_pct/100) ^ years
    let multiplier = 1.0 + (contract.escalator_pct / 100.0);
    let expected = contract.monthly_amount * multiplier.powi(years_elapsed);

    // Round to 2 decimal places.
    (expected * 100.0).round() / 100.0
}

async fn persist_match(
    pool: &PgPool,
    invoice_id: Uuid,
    contract_id: Uuid,
    expected_amount: Option<f64>,
    actual_amount: Option<f64>,
    variance_pct: Option<f64>,
    match_result: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO contract_matches
               (invoice_id, contract_id, expected_amount, actual_amount, variance_pct, match_result)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (invoice_id) DO UPDATE
               SET contract_id = $2,
                   expected_amount = $3,
                   actual_amount = $4,
                   variance_pct = $5,
                   match_result = $6,
                   matched_at = NOW()"#,
    )
    .bind(invoice_id)
    .bind(contract_id)
    .bind(expected_amount)
    .bind(actual_amount)
    .bind(variance_pct)
    .bind(match_result)
    .execute(pool)
    .await
    .map_err(|e| anyhow::anyhow!("Failed to persist contract match: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_contract(
        monthly_amount: f64,
        escalator_pct: f64,
        start_date: NaiveDate,
        end_date: NaiveDate,
        tolerance_pct: f64,
        anniversary_month: Option<i16>,
    ) -> ContractRow {
        ContractRow {
            id: Uuid::new_v4(),
            monthly_amount,
            escalator_pct,
            escalator_anniversary_month: anniversary_month,
            start_date,
            end_date,
            tolerance_pct,
        }
    }

    #[test]
    fn no_escalator_returns_base_amount() {
        let contract = make_contract(
            10.00,
            0.0,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
            2.0,
            None,
        );
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let result = compute_expected_amount(&contract, date);
        assert!((result - 10.00).abs() < 0.001);
    }

    #[test]
    fn escalator_applied_after_one_year() {
        let contract = make_contract(
            1000.00,
            3.0,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
            2.0,
            None,
        );
        let date = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let result = compute_expected_amount(&contract, date);
        // 1000 * 1.03 = 1030.00
        assert!((result - 1030.00).abs() < 0.01);
    }

    #[test]
    fn escalator_applied_after_two_years() {
        let contract = make_contract(
            1000.00,
            3.0,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2028, 12, 31).unwrap(),
            2.0,
            None,
        );
        let date = NaiveDate::from_ymd_opt(2027, 3, 1).unwrap();
        let result = compute_expected_amount(&contract, date);
        // 1000 * 1.03^2 = 1060.90
        assert!((result - 1060.90).abs() < 0.01);
    }

    #[test]
    fn escalator_not_applied_before_anniversary() {
        let contract = make_contract(
            1000.00,
            3.0,
            NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
            NaiveDate::from_ymd_opt(2027, 12, 31).unwrap(),
            2.0,
            None,
        );
        let date = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let result = compute_expected_amount(&contract, date);
        assert!((result - 1000.00).abs() < 0.01);
    }

    #[test]
    fn custom_anniversary_month_overrides_start_month() {
        let contract = make_contract(
            1000.00,
            5.0,
            NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            NaiveDate::from_ymd_opt(2028, 12, 31).unwrap(),
            2.0,
            Some(1),
        );
        let date = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let result = compute_expected_amount(&contract, date);
        // 1000 * 1.05 = 1050.00
        assert!((result - 1050.00).abs() < 0.01);
    }
}
