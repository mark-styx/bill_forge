//! Document Q&A with clause-level citations
//!
//! Accepts a question about a specific document, extracts text chunks from the
//! PDF, scores them with a lightweight TF-IDF ranker, calls the AI provider
//! for an answer grounded in those excerpts, and returns structured citations
//! that the frontend can use to highlight the relevant spans on the PDF.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use lopdf::Document as PdfDocument;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use billforge_ai_agent::models::{ProviderChatMessage, ProviderChatRequest, ProviderMessageRole};
use billforge_ai_agent::provider::AiProvider;
use billforge_ai_agent::OpenAiCompatibleProvider;

use billforge_db::DocumentRepositoryImpl;

use crate::extractors::DocumentsAccess;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct QaRequest {
    pub question: String,
}

#[derive(Debug, Serialize)]
pub struct QaResponse {
    pub answer: String,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Citation {
    pub id: usize,
    pub page: u32,
    pub bbox: [f32; 4],
    pub quote: String,
}

// ---------------------------------------------------------------------------
// Internal chunk type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TextChunk {
    page: u32,
    bbox: [f32; 4],
    text: String,
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new().route("/{document_id}/qa", post(qa_handler))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

async fn qa_handler(
    State(state): State<AppState>,
    DocumentsAccess(_user, tenant): DocumentsAccess,
    Path(document_id): Path<String>,
    Json(request): Json<QaRequest>,
) -> Result<Json<QaResponse>, (StatusCode, Json<serde_json::Value>)> {
    let doc_uuid = Uuid::parse_str(&document_id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid document ID" })),
        )
    })?;

    // Load document metadata via RLS-aware tenant pool
    let pool = state.db.tenant(&tenant.tenant_id).await.map_err(|e| {
        tracing::error!("Tenant pool error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Database error" })),
        )
    })?;
    let doc_repo = DocumentRepositoryImpl::new(pool);
    let doc_meta = doc_repo
        .get_by_id(&tenant.tenant_id, doc_uuid)
        .await
        .map_err(|e| {
            tracing::error!("Document lookup error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Document not found" })),
            )
        })?;

    // Only PDF documents are supported
    if doc_meta.mime_type != "application/pdf" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Only PDF documents are supported for Q&A" })),
        ));
    }

    // Download the PDF bytes
    let pdf_bytes = state
        .storage
        .download(&tenant.tenant_id, doc_uuid)
        .await
        .map_err(|e| {
            tracing::error!("Document download error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve document" })),
            )
        })?;

    // Extract text chunks from PDF
    let chunks = extract_chunks(&pdf_bytes).map_err(|e| {
        tracing::error!("PDF extraction error: {}", e);
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Failed to extract text from PDF" })),
        )
    })?;

    if chunks.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "No extractable text found in document" })),
        ));
    }

    // TF-IDF top-k selection
    let top_chunks = tfidf_top_k(&chunks, &request.question, 5);

    // Build prompt for the LLM
    let excerpts_text = top_chunks
        .iter()
        .enumerate()
        .map(|(i, c)| format!("[#{}] (page {}) {}", i + 1, c.page, c.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt = "You are a document analysis assistant. Answer the user's question using ONLY the provided excerpts. After your answer, list the excerpt IDs you cited like this: Citations: [#1] [#2]. If you cannot answer from the excerpts, say so.".to_string();

    let user_message = format!(
        "Document excerpts:\n\n{}\n\nQuestion: {}",
        excerpts_text, request.question
    );

    // Call the AI provider
    let provider = OpenAiCompatibleProvider::from_env();
    let chat_request = ProviderChatRequest {
        model: provider.model_name().to_string(),
        model_route: billforge_ai_agent::models::ProviderModelRoute::Default,
        messages: vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: system_prompt,
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: user_message,
            },
        ],
        temperature: Some(0.1),
        max_tokens: Some(1024),
        stop: None,
        tools: None,
    };

    let response = provider.chat_completion(chat_request).await.map_err(|e| {
        tracing::error!("LLM provider error: {:?}", e);
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "AI service error" })),
        )
    })?;

    let answer_text = response.message.content;

    // Parse citation markers from the answer
    let cited_indices = parse_citation_markers(&answer_text);

    // Build citations from the top chunks that were cited
    let citations: Vec<Citation> = cited_indices
        .iter()
        .filter_map(|&idx| {
            if idx == 0 || idx > top_chunks.len() {
                return None;
            }
            let chunk = &top_chunks[idx - 1];
            Some(Citation {
                id: idx,
                page: chunk.page,
                bbox: chunk.bbox,
                quote: truncate_quote(&chunk.text, 200),
            })
        })
        .collect();

    Ok(Json(QaResponse {
        answer: answer_text,
        citations,
    }))
}

// ---------------------------------------------------------------------------
// PDF text extraction
// ---------------------------------------------------------------------------

fn extract_chunks(pdf_bytes: &[u8]) -> Result<Vec<TextChunk>, String> {
    let pdf =
        PdfDocument::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {}", e))?;

    let mut chunks = Vec::new();
    let max_pages = pdf.get_pages().len() as u32;

    for page_num in 1..=max_pages {
        let page_text = extract_page_text(&pdf, page_num);
        if page_text.trim().is_empty() {
            continue;
        }

        // Split page text into ~500-token spans (~2000 chars as rough approximation)
        let chunk_size = 2000;
        let page_height = get_page_height(&pdf, page_num);
        let chars = page_text.chars().collect::<Vec<_>>();
        let total_chars = chars.len();

        if total_chars == 0 {
            continue;
        }

        let num_spans = ((total_chars as f64) / chunk_size as f64).ceil() as usize;
        let span_height = page_height / num_spans.max(1) as f32;

        for (i, span) in chars.chunks(chunk_size).enumerate() {
            let text: String = span.iter().collect();
            if text.trim().is_empty() {
                continue;
            }
            // Approximate bounding box: full width, proportional vertical slice
            let top = page_height - (i as f32 + 1.0) * span_height;
            let bottom = page_height - (i as f32) * span_height;
            chunks.push(TextChunk {
                page: page_num,
                bbox: [0.0, top, 612.0, bottom], // 612pt is standard US letter width
                text,
            });
        }
    }

    Ok(chunks)
}

fn extract_page_text(pdf: &PdfDocument, page_num: u32) -> String {
    let pages = pdf.get_pages();
    let page_id = match pages.get(&page_num) {
        Some(id) => *id,
        None => return String::new(),
    };

    let content_obj_ids = pdf.get_page_contents(page_id);
    let mut text_parts: Vec<String> = Vec::new();

    for obj_id in content_obj_ids {
        if let Ok(obj) = pdf.get_object(obj_id) {
            if let Ok(stream) = obj.as_stream() {
                if let Ok(decompressed) = stream.decompressed_content() {
                    let content_str = String::from_utf8_lossy(&decompressed);
                    for line in content_str.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with('(') && trimmed.contains("Tj") {
                            if let Some(text) = extract_paren_text(trimmed) {
                                text_parts.push(text);
                            }
                        } else if trimmed.starts_with('[') && trimmed.contains("TJ") {
                            text_parts.push(extract_array_text(trimmed));
                        }
                    }
                }
            }
        }
    }

    text_parts.join(" ")
}

fn extract_paren_text(s: &str) -> Option<String> {
    // Extract text from "(text) Tj" pattern
    let start = s.find('(')?;
    let end = s.rfind(')')?;
    if end <= start {
        return None;
    }
    Some(s[start + 1..end].replace('\\', ""))
}

fn extract_array_text(s: &str) -> String {
    // Extract text from "[(text1) num (text2)] TJ" pattern
    let mut result = String::new();
    let mut in_paren = false;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                in_paren = true;
                current.clear();
            }
            ')' if in_paren => {
                in_paren = false;
                result.push_str(&current);
                result.push(' ');
            }
            _ if in_paren => {
                current.push(ch);
            }
            _ => {}
        }
    }

    result.trim().to_string()
}

fn get_page_height(pdf: &PdfDocument, page_num: u32) -> f32 {
    let pages = pdf.get_pages();
    let page_id = match pages.get(&page_num) {
        Some(id) => *id,
        None => return 792.0, // default US letter height
    };

    if let Ok(obj) = pdf.get_object(page_id) {
        if let Ok(page_dict) = obj.as_dict() {
            if let Ok(mediabox) = page_dict.get(b"MediaBox") {
                if let Ok(arr) = mediabox.as_array() {
                    if arr.len() >= 4 {
                        if let Ok(height) = arr[3].as_f32() {
                            return height;
                        }
                    }
                }
            }
        }
    }
    792.0 // default US letter height
}

// ---------------------------------------------------------------------------
// TF-IDF scorer (inline, no external deps)
// ---------------------------------------------------------------------------

/// Tokenize a string into lowercase words.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

/// Compute term frequency for a list of tokens.
fn term_frequency(tokens: &[String]) -> HashMap<String, f64> {
    let mut tf = HashMap::new();
    let total = tokens.len() as f64;
    if total == 0.0 {
        return tf;
    }
    for token in tokens {
        *tf.entry(token.clone()).or_insert(0.0) += 1.0;
    }
    for count in tf.values_mut() {
        *count /= total;
    }
    tf
}

/// Select the top-k chunks most relevant to the query using TF-IDF scoring.
fn tfidf_top_k(chunks: &[TextChunk], query: &str, k: usize) -> Vec<TextChunk> {
    let query_tokens = tokenize(query);
    let query_tf = term_frequency(&query_tokens);

    // Compute IDF across all chunks
    let n = chunks.len() as f64;
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    for chunk in chunks {
        let chunk_tokens = tokenize(&chunk.text);
        let unique: std::collections::HashSet<&String> = chunk_tokens.iter().collect();
        for token in unique {
            *doc_freq.entry(token.clone()).or_insert(0) += 1;
        }
    }

    // Score each chunk
    let mut scored: Vec<(f64, &TextChunk)> = chunks
        .iter()
        .map(|chunk| {
            let chunk_tokens = tokenize(&chunk.text);
            let chunk_tf = term_frequency(&chunk_tokens);
            let mut score = 0.0;
            for (term, &qtf) in &query_tf {
                if let Some(&ctf) = chunk_tf.get(term) {
                    let df = *doc_freq.get(term).unwrap_or(&1) as f64;
                    let idf = (n / df).ln() + 1.0;
                    score += qtf * ctf * idf;
                }
            }
            (score, chunk)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .into_iter()
        .take(k)
        .map(|(_, chunk)| chunk.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Citation parsing
// ---------------------------------------------------------------------------

/// Parse `[#1] [#2]` style markers from the LLM output.
fn parse_citation_markers(text: &str) -> Vec<usize> {
    let re = regex::Regex::new(r"\[#(\d+)\]").unwrap();
    re.captures_iter(text)
        .filter_map(|cap| cap[1].parse::<usize>().ok())
        .collect()
}

/// Truncate a quote for display in citations.
fn truncate_quote(text: &str, max_len: usize) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.len() <= max_len {
        cleaned
    } else {
        format!("{}...", &cleaned[..max_len - 3])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_correctly() {
        let tokens = tokenize("Hello, World! 123");
        assert_eq!(tokens, vec!["hello", "world", "123"]);
    }

    #[test]
    fn term_frequency_works() {
        let tokens = tokenize("hello hello world");
        let tf = term_frequency(&tokens);
        assert!(*tf.get("hello").unwrap() > *tf.get("world").unwrap());
    }

    #[test]
    fn tfidf_top_k_returns_relevant() {
        let chunks = vec![
            TextChunk {
                page: 1,
                bbox: [0.0, 0.0, 612.0, 792.0],
                text: "The total invoice amount is $5,000.00 for consulting services.".to_string(),
            },
            TextChunk {
                page: 2,
                bbox: [0.0, 0.0, 612.0, 792.0],
                text: "Payment is due within 30 days of the invoice date.".to_string(),
            },
            TextChunk {
                page: 3,
                bbox: [0.0, 0.0, 612.0, 792.0],
                text: "The vendor name is Acme Corporation located in Chicago.".to_string(),
            },
        ];

        let top = tfidf_top_k(&chunks, "What is the invoice amount?", 2);
        assert_eq!(top.len(), 2);
        assert!(top[0].text.contains("invoice amount"));
    }

    #[test]
    fn parse_citation_markers_extracts_ids() {
        let text = "The total is $5,000. Citations: [#1] [#3]";
        let ids = parse_citation_markers(text);
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn parse_citation_markers_empty_when_none() {
        let text = "No citations here.";
        let ids = parse_citation_markers(text);
        assert!(ids.is_empty());
    }

    #[test]
    fn truncate_quote_short() {
        assert_eq!(truncate_quote("short text", 200), "short text");
    }

    #[test]
    fn truncate_quote_long() {
        let long = "a ".repeat(200);
        let result = truncate_quote(&long, 50);
        assert!(result.len() <= 50);
        assert!(result.ends_with("..."));
    }
}
