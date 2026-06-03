//! Tests for per-line-item GL categorization (issue #315)
//!
//! Covers:
//!  1. Three distinct line items -> three different GL codes, no splits.
//!  2. Compound line with vendor-history split -> two LineSplitSuggestions.
//!  3. Vendor-history fallback -> prior coding reused with high confidence.
//!  4. Unit-level helpers (keyword detection, split detection, corrections).

use billforge_invoice_processing::categorization::{
    apply_line_correction, categorize_line_by_keywords, collect_gl_signals,
    detect_historical_splits, detect_line_splits, find_matching_prior, HistoricalSplit,
    LineItemInput, PriorLineCoding, VendorHistory,
};
use billforge_invoice_processing::categorization::CategoryType;
use billforge_invoice_processing::feedback_loop::CorrectionRule;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test 1: Three distinct line items -> different GL codes, no splits
// ---------------------------------------------------------------------------

#[test]
fn test_three_distinct_lines_different_gl_codes_no_splits() {
    let items = vec![
        LineItemInput {
            description: "AWS EC2 compute instances".to_string(),
            quantity: Some(1.0),
            amount: 500.00,
        },
        LineItemInput {
            description: "Office chairs for main floor".to_string(),
            quantity: Some(10.0),
            amount: 3000.00,
        },
        LineItemInput {
            description: "Marketing ads - Google Ads campaign".to_string(),
            quantity: Some(1.0),
            amount: 1500.00,
        },
    ];

    let gl_codes: Vec<String> = items
        .iter()
        .map(|item| categorize_line_by_keywords(&item.description).gl_code.clone())
        .collect();

    assert_eq!(gl_codes[0], "6000-Software & Subscriptions");
    assert_eq!(gl_codes[1], "5000-Office Supplies & Equipment");
    assert_eq!(gl_codes[2], "7000-Marketing");

    // No splits for simple single-topic descriptions
    for item in &items {
        let splits = detect_line_splits(&item.description, item.amount, None);
        assert!(
            splits.is_empty(),
            "Expected no splits for '{}', got {:?}",
            item.description,
            splits
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Compound line with vendor-history 60/40 split
// ---------------------------------------------------------------------------

#[test]
fn test_compound_line_with_vendor_history_split() {
    let vendor_id = Uuid::new_v4();
    let line_description = "Travel + Meals - client trip";
    let line_amount = 1000.00;

    let vendor_history = VendorHistory {
        vendor_id,
        prior_codings: vec![
            PriorLineCoding {
                description: "Travel - client trip".to_string(),
                gl_code: "8000-Travel & Entertainment".to_string(),
                department: None,
                cost_center: None,
                amount: 600.00,
            },
            PriorLineCoding {
                description: "Meals - client trip".to_string(),
                gl_code: "8000-Travel & Entertainment".to_string(),
                department: None,
                cost_center: None,
                amount: 400.00,
            },
        ],
        splits: vec![
            HistoricalSplit {
                gl_code: "8000-Travel & Entertainment".to_string(),
                department: None,
                cost_center: None,
                ratio: 0.60,
            },
            HistoricalSplit {
                gl_code: "8001-Meals".to_string(),
                department: None,
                cost_center: None,
                ratio: 0.40,
            },
        ],
    };

    let splits = detect_line_splits(line_description, line_amount, Some(&vendor_history));

    assert_eq!(splits.len(), 2, "Expected 2 splits for compound line");

    // Amounts should sum to the line total
    let total_amount: f64 = splits.iter().map(|s| s.amount).sum();
    assert!(
        (total_amount - line_amount).abs() < 0.01,
        "Split amounts should sum to {}, got {}",
        line_amount,
        total_amount
    );

    // Percentages should sum to ~1.0
    let total_pct: f64 = splits.iter().map(|s| s.percentage).sum();
    assert!(
        (total_pct - 1.0).abs() < 0.01,
        "Split percentages should sum to 1.0, got {}",
        total_pct
    );

    // Verify proportional allocation (60/40)
    let travel_split = splits
        .iter()
        .find(|s| s.gl_code == "8000-Travel & Entertainment")
        .expect("Should have travel split");
    let meals_split = splits
        .iter()
        .find(|s| s.gl_code == "8001-Meals")
        .expect("Should have meals split");

    assert!(
        (travel_split.percentage - 0.60).abs() < 0.01,
        "Travel split should be 60%, got {}",
        travel_split.percentage
    );
    assert!(
        (meals_split.percentage - 0.40).abs() < 0.01,
        "Meals split should be 40%, got {}",
        meals_split.percentage
    );
}

// ---------------------------------------------------------------------------
// Test 3: Vendor-history fallback -> prior coding reused
// ---------------------------------------------------------------------------

#[test]
fn test_vendor_history_fallback_picks_prior_gl() {
    let prior_codings = vec![PriorLineCoding {
        description: "AWS EC2 compute".to_string(),
        gl_code: "6000-Software & Subscriptions".to_string(),
        department: Some("Engineering".to_string()),
        cost_center: Some("CC-Dev".to_string()),
        amount: 500.00,
    }];

    let new_description = "AWS EC2 compute instances - monthly";
    let match_result = find_matching_prior(new_description, &prior_codings);

    assert!(
        match_result.is_some(),
        "Should match prior coding for similar description"
    );
    let matched = match_result.unwrap();
    assert_eq!(matched.gl_code, "6000-Software & Subscriptions");
    assert_eq!(matched.department.as_deref(), Some("Engineering"));
    assert_eq!(matched.cost_center.as_deref(), Some("CC-Dev"));
}

#[test]
fn test_vendor_history_no_match_falls_back_to_keywords() {
    let prior_codings = vec![PriorLineCoding {
        description: "Office supplies".to_string(),
        gl_code: "5000-Office Supplies & Equipment".to_string(),
        department: None,
        cost_center: None,
        amount: 200.00,
    }];

    let new_description = "AWS cloud infrastructure";
    let match_result = find_matching_prior(new_description, &prior_codings);
    assert!(
        match_result.is_none(),
        "Should not match unrelated description"
    );

    // Falls back to keywords
    let kw = categorize_line_by_keywords(new_description);
    assert_eq!(kw.gl_code, "6000-Software & Subscriptions");
    assert!(kw.confidence > 0.40);
}

// ---------------------------------------------------------------------------
// Test 4: Helper function tests
// ---------------------------------------------------------------------------

#[test]
fn test_collect_gl_signals_compound_description() {
    let signals = collect_gl_signals("software license and marketing ads bundle");
    assert!(
        signals.len() >= 2,
        "Expected >=2 GL signals for software+marketing, got {:?}",
        signals.iter().map(|s| &s.0).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_historical_splits_single_gl() {
    let codings = vec![
        PriorLineCoding {
            description: "Software license".to_string(),
            gl_code: "6000-Software & Subscriptions".to_string(),
            department: None,
            cost_center: None,
            amount: 500.00,
        },
        PriorLineCoding {
            description: "Software license".to_string(),
            gl_code: "6000-Software & Subscriptions".to_string(),
            department: None,
            cost_center: None,
            amount: 300.00,
        },
    ];

    let splits = detect_historical_splits(&codings);
    assert!(
        splits.is_empty(),
        "Single GL code should not produce splits"
    );
}

#[test]
fn test_detect_historical_splits_multiple_gls() {
    let codings = vec![
        PriorLineCoding {
            description: "Travel expenses".to_string(),
            gl_code: "8000-Travel & Entertainment".to_string(),
            department: None,
            cost_center: None,
            amount: 600.00,
        },
        PriorLineCoding {
            description: "Meal expenses".to_string(),
            gl_code: "8001-Meals".to_string(),
            department: None,
            cost_center: None,
            amount: 400.00,
        },
    ];

    let splits = detect_historical_splits(&codings);
    assert_eq!(splits.len(), 2, "Expected 2 splits from 2 distinct GLs");

    let total_ratio: f64 = splits.iter().map(|s| s.ratio).sum();
    assert!(
        (total_ratio - 1.0).abs() < 0.01,
        "Ratios should sum to 1.0, got {}",
        total_ratio
    );
}

#[test]
fn test_apply_line_correction_no_match() {
    let rules = vec![CorrectionRule {
        category_type: CategoryType::GlCode,
        suggested_value: "7000-Marketing".to_string(),
        correct_value: "7100-Digital Ads".to_string(),
        frequency: 5,
    }];

    let result = apply_line_correction("6000-Software & Subscriptions", &rules);
    assert_eq!(result, "6000-Software & Subscriptions");
}

#[test]
fn test_apply_line_correction_with_match() {
    let rules = vec![CorrectionRule {
        category_type: CategoryType::GlCode,
        suggested_value: "6000-Software & Subscriptions".to_string(),
        correct_value: "6100-SaaS Licenses".to_string(),
        frequency: 10,
    }];

    let result = apply_line_correction("6000-Software & Subscriptions", &rules);
    assert_eq!(result, "6100-SaaS Licenses");
}
