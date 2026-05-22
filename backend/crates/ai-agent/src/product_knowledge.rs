//! Product documentation knowledge index for Winston AI.
//!
//! Provides a small, deterministic, in-memory index built from compile-time
//! included repository files. No embeddings, no network calls, no database
//! migrations - just lexical matching against chunked Markdown content.

// ---------------------------------------------------------------------------
// Compile-time source file contents
// ---------------------------------------------------------------------------

const NORTHSTAR_MD: &str = include_str!("../../../../docs/northstar.md");
const CHANGELOG_MD: &str = include_str!("../../../../CHANGELOG.md");
const RELEASE_YML: &str = include_str!("../../../../.github/workflows/release.yml");

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single chunk of ingested product documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductKnowledgeSnippet {
    /// Repository-relative path of the source file (e.g. `"docs/northstar.md"`).
    pub source_path: &'static str,
    /// Heading or title under which this chunk was extracted.
    pub heading: String,
    /// Short excerpt of the chunk body.
    pub excerpt: String,
}

/// Internal representation of a single indexed chunk before scoring.
#[derive(Debug, Clone)]
struct Chunk {
    source_path: &'static str,
    heading: String,
    body: String,
}

// ---------------------------------------------------------------------------
// Index
// ---------------------------------------------------------------------------

/// In-memory product documentation index.
///
/// Built once at first use from compile-time `include_str!` content. Each
/// source is split into chunks by Markdown headings (or fixed line windows
/// for sections without headings). Querying uses a simple lexical scorer.
pub struct ProductKnowledgeIndex {
    chunks: Vec<Chunk>,
}

impl ProductKnowledgeIndex {
    /// Build the index from the hard-coded source files.
    fn build() -> Self {
        let mut chunks = Vec::new();

        ingest_markdown(&mut chunks, "docs/northstar.md", NORTHSTAR_MD);
        ingest_markdown(&mut chunks, "CHANGELOG.md", CHANGELOG_MD);
        ingest_yaml(&mut chunks, ".github/workflows/release.yml", RELEASE_YML);

        Self { chunks }
    }

    /// Return the shared static index, lazily initialized.
    fn instance() -> &'static Self {
        use std::sync::OnceLock;
        static INDEX: OnceLock<ProductKnowledgeIndex> = OnceLock::new();
        INDEX.get_or_init(Self::build)
    }

    /// Return all source paths present in the index.
    pub fn source_paths() -> Vec<&'static str> {
        let idx = Self::instance();
        let mut paths: Vec<&'static str> = idx
            .chunks
            .iter()
            .map(|c| c.source_path)
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Retrieve up to `limit` product-knowledge snippets relevant to `query`.
    ///
    /// Scoring is a simple lowercase token intersection between the query and
    /// the chunk's heading + body + path. Results are returned in descending
    /// score order, with ties broken by chunk index for stability.
    pub fn search(query: &str, limit: usize) -> Vec<ProductKnowledgeSnippet> {
        let idx = Self::instance();
        let query_tokens = tokenize(query);

        if query_tokens.is_empty() {
            return Vec::new();
        }

        let limit = limit.max(1);
        let mut scored: Vec<(usize, usize, &Chunk)> = Vec::new();

        for (i, chunk) in idx.chunks.iter().enumerate() {
            let score = score_chunk(&query_tokens, chunk);
            if score > 0 {
                scored.push((score, i, chunk));
            }
        }

        // Sort descending by score, ascending by index for stable ordering.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, chunk)| ProductKnowledgeSnippet {
                source_path: chunk.source_path,
                heading: chunk.heading.clone(),
                excerpt: truncate_excerpt(&chunk.body, 300),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Ingestion helpers
// ---------------------------------------------------------------------------

/// Maximum line window size when no heading is found.
const WINDOW_SIZE: usize = 20;

/// Split a Markdown document into chunks by top-level (`##`) headings.
/// Lines before the first heading are attached to a "Title" chunk using
/// the first line of the document as the heading name.
fn ingest_markdown(chunks: &mut Vec<Chunk>, source_path: &'static str, content: &str) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return;
    }

    // Find the positions of all `## ` headings (exactly two hashes + space).
    let mut heading_starts: Vec<(usize, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(rest) = line.strip_prefix("## ") {
            heading_starts.push((i, rest.trim().to_string()));
        }
    }

    if heading_starts.is_empty() {
        // No headings at all: chunk the whole file into fixed windows.
        ingest_windows(chunks, source_path, &lines, content);
        return;
    }

    // Everything before the first `## ` belongs to the document title.
    let title_line = lines
        .first()
        .map(|l| l.trim_start_matches('#').trim())
        .unwrap_or("Introduction");
    let first_heading = heading_starts[0].0;

    if first_heading > 0 {
        let body = lines[..first_heading].join("\n");
        let body = body.trim();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading: title_line.to_string(),
                body: body.to_string(),
            });
        }
    }

    // Each heading section: from this heading start to the next (or EOF).
    for (idx, (start, heading)) in heading_starts.iter().enumerate() {
        let end = heading_starts
            .get(idx + 1)
            .map(|(s, _)| *s)
            .unwrap_or(lines.len());
        let body = lines[*start..end].join("\n");
        let body = body.trim();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading: heading.clone(),
                body: body.to_string(),
            });
        }
    }

    // For sections with very long bodies and no sub-headings, split further.
    // This is a secondary pass that breaks oversized chunks into windows.
    let mut extra = Vec::new();
    let chunk_count_before = chunks.len();
    for i in 0..chunk_count_before {
        let chunk = &chunks[i];
        // Only split chunks from this source that are very long.
        if chunk.source_path != source_path {
            continue;
        }
        let body_lines: Vec<&str> = chunk.body.lines().collect();
        if body_lines.len() <= WINDOW_SIZE * 2 {
            continue;
        }
        // Replace the oversized chunk with windowed sub-chunks.
        let heading_prefix = chunk.heading.clone();
        for (wi, window) in body_lines.chunks(WINDOW_SIZE).enumerate() {
            let sub_heading = if wi == 0 {
                heading_prefix.clone()
            } else {
                format!("{} (part {})", heading_prefix, wi + 1)
            };
            let sub_body = window.join("\n");
            extra.push((
                i,
                Chunk {
                    source_path,
                    heading: sub_heading,
                    body: sub_body,
                },
            ));
        }
    }
    // Replace oversized chunks with their windowed sub-chunks.
    // Process in reverse to keep indices stable.
    for (original_idx, replacement) in extra.into_iter().rev() {
        chunks.remove(original_idx);
        chunks.insert(original_idx, replacement);
    }
}

/// Fallback: split lines into fixed-size windows when no headings exist.
fn ingest_windows(chunks: &mut Vec<Chunk>, source_path: &'static str, lines: &[&str], _content: &str) {
    let title = lines
        .first()
        .map(|l| l.trim_start_matches('#').trim())
        .unwrap_or("Document");

    for (i, window) in lines.chunks(WINDOW_SIZE).enumerate() {
        let heading = if i == 0 {
            title.to_string()
        } else {
            format!("{} (part {})", title, i + 1)
        };
        let body = window.join("\n");
        let body = body.trim();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading,
                body: body.to_string(),
            });
        }
    }
}

/// Ingest a YAML file (e.g. release workflow) by splitting into top-level
/// key sections. Uses line-based splitting on top-level keys (no indent).
fn ingest_yaml(chunks: &mut Vec<Chunk>, source_path: &'static str, content: &str) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return;
    }

    // Find top-level YAML keys (no leading whitespace, contains ":").
    let mut section_starts: Vec<(usize, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Top-level key: line starts at column 0 and contains ":"
        if !line.starts_with(|c: char| c.is_whitespace()) {
            if let Some(colon_pos) = trimmed.find(':') {
                let key = &trimmed[..colon_pos];
                // Skip YAML directives like "---"
                if !key.is_empty() && key != "---" {
                    section_starts.push((i, key.to_string()));
                }
            }
        }
    }

    if section_starts.is_empty() {
        // No sections found; ingest as a single chunk.
        let body = content.trim();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading: "Release Workflow".to_string(),
                body: body.to_string(),
            });
        }
        return;
    }

    // Everything before the first key is a preamble.
    let first_start = section_starts[0].0;
    if first_start > 0 {
        let body = lines[..first_start].join("\n").trim().to_string();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading: "Release Workflow".to_string(),
                body,
            });
        }
    }

    // Each section: from this key start to the next top-level key (or EOF).
    for (idx, (start, key)) in section_starts.iter().enumerate() {
        let end = section_starts
            .get(idx + 1)
            .map(|(s, _)| *s)
            .unwrap_or(lines.len());
        let body = lines[*start..end].join("\n");
        let body = body.trim();
        if !body.is_empty() {
            chunks.push(Chunk {
                source_path,
                heading: key.clone(),
                body: body.to_string(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Lower-case tokenize: split on whitespace and common punctuation, drop
/// empty tokens and single-char tokens.
fn tokenize(input: &str) -> Vec<String> {
    input
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '/' | '-' | '(' | ')'))
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect()
}

/// Score a chunk against query tokens. Returns the number of distinct query
/// tokens that appear as exact tokens in the chunk's heading, body, or source
/// path. Uses the same tokenization as the query to avoid partial-word matches
/// (e.g. "me" inside "time").
fn score_chunk(query_tokens: &[String], chunk: &Chunk) -> usize {
    let combined = format!(
        "{} {} {}",
        chunk.heading, chunk.body, chunk.source_path
    );
    let doc_tokens = tokenize(&combined);

    query_tokens
        .iter()
        .filter(|qt| doc_tokens.iter().any(|dt| dt == *qt))
        .count()
}

/// Truncate `text` to at most `max_chars` characters, breaking at the
/// nearest word boundary.
fn truncate_excerpt(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut end = max_chars;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // Try to break at a word boundary.
    if end > 0 {
        if let Some(space_pos) = text[..end].rfind(|c: char| c.is_whitespace()) {
            end = space_pos;
        }
    }
    let mut result = text[..end].to_string();
    result.push_str("...");
    result
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Retrieve product-knowledge snippets relevant to the given `query`.
///
/// Returns at most `limit` snippets (default 3) sorted by lexical relevance.
/// Each snippet carries its source path, heading, and a bounded excerpt.
pub fn product_knowledge_context_for_query(query: &str) -> Vec<ProductKnowledgeSnippet> {
    ProductKnowledgeIndex::search(query, 3)
}

/// Retrieve product-knowledge snippets with a custom limit.
pub fn product_knowledge_context_for_query_with_limit(
    query: &str,
    limit: usize,
) -> Vec<ProductKnowledgeSnippet> {
    ProductKnowledgeIndex::search(query, limit)
}

/// Format a list of snippets into a compact context block suitable for
/// injection into an LLM system prompt.
pub fn format_product_knowledge_block(snippets: &[ProductKnowledgeSnippet]) -> String {
    if snippets.is_empty() {
        return String::new();
    }

    let mut block = String::from("## Product Documentation Context\n\n");
    block.push_str("The following product documentation excerpts may help answer the user's question.\n\n");

    for snippet in snippets {
        block.push_str(&format!(
            "### [{}] {}\n{}\n\n",
            snippet.source_path, snippet.heading, snippet.excerpt
        ));
    }

    block
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Index content tests
    // -----------------------------------------------------------------------

    #[test]
    fn index_contains_northstar_md() {
        let paths = ProductKnowledgeIndex::source_paths();
        assert!(
            paths.contains(&"docs/northstar.md"),
            "expected docs/northstar.md in source paths, got: {:?}",
            paths
        );
    }

    #[test]
    fn index_contains_changelog_md() {
        let paths = ProductKnowledgeIndex::source_paths();
        assert!(
            paths.contains(&"CHANGELOG.md"),
            "expected CHANGELOG.md in source paths, got: {:?}",
            paths
        );
    }

    #[test]
    fn index_contains_release_yml() {
        let paths = ProductKnowledgeIndex::source_paths();
        assert!(
            paths.contains(&".github/workflows/release.yml"),
            "expected .github/workflows/release.yml in source paths, got: {:?}",
            paths
        );
    }

    // -----------------------------------------------------------------------
    // Query relevance tests
    // -----------------------------------------------------------------------

    #[test]
    fn product_intent_query_returns_northstar_snippet() {
        let results = product_knowledge_context_for_query("What is BillForge product vision?");
        assert!(
            !results.is_empty(),
            "expected at least one result for product-intent query"
        );
        let northstar_results: Vec<_> = results
            .iter()
            .filter(|s| s.source_path == "docs/northstar.md")
            .collect();
        assert!(
            !northstar_results.is_empty(),
            "expected at least one docs/northstar.md result, got: {:?}",
            results
                .iter()
                .map(|s| s.source_path)
                .collect::<Vec<_>>()
        );
        // The snippet should mention BillForge or product-related content.
        let body_match = northstar_results
            .iter()
            .any(|s| s.excerpt.to_lowercase().contains("billforge") || s.heading.to_lowercase().contains("vision"));
        assert!(
            body_match,
            "expected northstar snippet to mention BillForge or have Vision heading"
        );
    }

    #[test]
    fn release_query_returns_changelog_snippet() {
        let results = product_knowledge_context_for_query("recent release changes and fixes");
        assert!(
            !results.is_empty(),
            "expected at least one result for release-intent query"
        );
        let changelog_results: Vec<_> = results
            .iter()
            .filter(|s| s.source_path == "CHANGELOG.md")
            .collect();
        assert!(
            !changelog_results.is_empty(),
            "expected at least one CHANGELOG.md result, got: {:?}",
            results
                .iter()
                .map(|s| s.source_path)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn changelog_query_matches_via_path() {
        // "changelog" appears in the source path itself.
        let results = product_knowledge_context_for_query("changelog");
        assert!(
            !results.is_empty(),
            "expected results for 'changelog' query"
        );
        assert!(
            results.iter().any(|s| s.source_path == "CHANGELOG.md"),
            "expected CHANGELOG.md in results"
        );
    }

    #[test]
    fn release_process_query_returns_release_yml_snippet() {
        let results = product_knowledge_context_for_query("How are tag-triggered releases and Docker images published?");
        assert!(
            !results.is_empty(),
            "expected at least one result for release-process query"
        );
        let release_results: Vec<_> = results
            .iter()
            .filter(|s| s.source_path == ".github/workflows/release.yml")
            .collect();
        assert!(
            !release_results.is_empty(),
            "expected at least one .github/workflows/release.yml result, got: {:?}",
            results
                .iter()
                .map(|s| s.source_path)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn github_release_creation_query_returns_release_yml() {
        let results = product_knowledge_context_for_query("GitHub release creation workflow");
        assert!(
            !results.is_empty(),
            "expected results for GitHub release workflow query"
        );
        assert!(
            results.iter().any(|s| s.source_path == ".github/workflows/release.yml"),
            "expected .github/workflows/release.yml in results, got: {:?}",
            results.iter().map(|s| s.source_path).collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------------
    // Snippet structure tests
    // -----------------------------------------------------------------------

    #[test]
    fn snippets_carry_source_paths() {
        let results = product_knowledge_context_for_query("invoice processing");
        for snippet in &results {
            assert!(
                !snippet.source_path.is_empty(),
                "snippet source_path must not be empty"
            );
            assert!(
                snippet.source_path == "docs/northstar.md"
                    || snippet.source_path == "CHANGELOG.md"
                    || snippet.source_path == ".github/workflows/release.yml",
                "unexpected source_path: {}",
                snippet.source_path
            );
        }
    }

    #[test]
    fn snippets_have_bounded_excerpt_length() {
        let results = product_knowledge_context_for_query("BillForge mission vision pillars");
        for snippet in &results {
            // Excerpt includes the "..." suffix (3 chars), so effective max
            // is 300 + 3 = 303. We allow a small margin for multi-byte chars.
            assert!(
                snippet.excerpt.len() <= 310,
                "excerpt too long ({} chars): {:?}",
                snippet.excerpt.len(),
                &snippet.excerpt[..snippet.excerpt.len().min(80)]
            );
        }
    }

    #[test]
    fn snippets_have_non_empty_headings() {
        let results = product_knowledge_context_for_query("northstar product");
        for snippet in &results {
            assert!(
                !snippet.heading.is_empty(),
                "snippet heading must not be empty"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn empty_query_returns_no_results() {
        let results = product_knowledge_context_for_query("");
        assert!(results.is_empty(), "empty query should return no results");
    }

    #[test]
    fn single_char_query_returns_no_results() {
        // All tokens of length <= 1 are filtered out by tokenize.
        let results = product_knowledge_context_for_query("a b c");
        assert!(
            results.is_empty(),
            "single-char-only query should return no results"
        );
    }

    #[test]
    fn irrelevant_query_returns_no_results() {
        let results = product_knowledge_context_for_query("quantum entanglement fibonacci spiral");
        assert!(
            results.is_empty(),
            "irrelevant query should return no results, got: {:?}",
            results.len()
        );
    }

    #[test]
    fn limit_is_respected() {
        let results = product_knowledge_context_for_query_with_limit("invoice", 1);
        assert!(results.len() <= 1, "expected at most 1 result, got {}", results.len());
    }

    // -----------------------------------------------------------------------
    // Tokenizer tests
    // -----------------------------------------------------------------------

    #[test]
    fn tokenize_splits_on_whitespace_and_punctuation() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "test"]);
    }

    #[test]
    fn tokenize_drops_single_char_tokens() {
        let tokens = tokenize("I a b big");
        assert_eq!(tokens, vec!["big"]);
    }

    // -----------------------------------------------------------------------
    // Format block test
    // -----------------------------------------------------------------------

    #[test]
    fn format_block_produces_header_and_entries() {
        let snippets = vec![ProductKnowledgeSnippet {
            source_path: "docs/northstar.md",
            heading: "Mission".to_string(),
            excerpt: "Eliminate the manual grind...".to_string(),
        }];
        let block = format_product_knowledge_block(&snippets);
        assert!(block.contains("## Product Documentation Context"));
        assert!(block.contains("### [docs/northstar.md] Mission"));
        assert!(block.contains("Eliminate the manual grind"));
    }

    #[test]
    fn format_block_empty_snippets_returns_empty_string() {
        let block = format_product_knowledge_block(&[]);
        assert!(block.is_empty());
    }
}
