//! Shared text-similarity utilities (OCR-aware Levenshtein).
//!
//! Extracted from `billforge_analytics::anomaly_detection` so both the analytics
//! and API crates can call it without creating a circular dependency.

/// OCR character substitution pairs - treating these as zero-cost substitutions
/// handles common OCR misreads in invoice numbers and vendor names.
const OCR_CONFUSABLE: &[(char, char)] = &[
    ('O', '0'),
    ('0', 'O'),
    ('I', '1'),
    ('1', 'I'),
    ('l', '1'),
    ('1', 'l'),
    ('I', 'l'),
    ('l', 'I'),
    ('S', '5'),
    ('5', 'S'),
    ('B', '8'),
    ('8', 'B'),
];

/// Returns true when two characters are an OCR confusable pair.
fn ocr_char_eq(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    a.eq_ignore_ascii_case(&b)
        || OCR_CONFUSABLE
            .iter()
            .any(|&(ca, cb)| (a == ca && b == cb) || (a == cb && b == ca))
}

/// OCR-aware normalized Levenshtein similarity (0.0 - 1.0).
/// OCR confusable substitutions cost 0 instead of 1.
pub fn ocr_levenshtein_similarity(s1: &str, s2: &str) -> f64 {
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    let c1: Vec<char> = s1.chars().collect();
    let c2: Vec<char> = s2.chars().collect();
    let len1 = c1.len();
    let len2 = c2.len();
    if len1 == 0 || len2 == 0 {
        return 0.0;
    }
    // Single-row DP
    let mut prev = vec![0usize; len2 + 1];
    let mut curr = vec![0usize; len2 + 1];
    for (j, slot) in prev.iter_mut().enumerate() {
        *slot = j;
    }
    for i in 1..=len1 {
        curr[0] = i;
        for j in 1..=len2 {
            let sub_cost = if ocr_char_eq(c1[i - 1], c2[j - 1]) {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // delete
                .min(curr[j - 1] + 1) // insert
                .min(prev[j - 1] + sub_cost); // substitute
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    let dist = prev[len2] as f64;
    let max_len = len1.max(len2) as f64;
    1.0 - dist / max_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings() {
        assert!((ocr_levenshtein_similarity("Acme Corp", "Acme Corp") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_strings() {
        assert!((ocr_levenshtein_similarity("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn one_empty() {
        assert!((ocr_levenshtein_similarity("abc", "") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_confusable_zero_cost() {
        // B <-> 8 is free
        let sim = ocr_levenshtein_similarity("ABCD", "A8CD");
        assert!(
            sim > 0.9,
            "OCR confusable should be high similarity: {}",
            sim
        );
    }

    #[test]
    fn lookalike_vendor_names() {
        let sim = ocr_levenshtein_similarity("Acme Corp", "Acme C0rp");
        assert!(
            sim >= 0.85,
            "Acme Corp vs Acme C0rp should be >= 0.85: {}",
            sim
        );
    }
}
