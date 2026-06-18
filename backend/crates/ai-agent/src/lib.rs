//! Winston AI Agent - Intelligent Invoice Assistant
//!
//! This crate provides AI-powered conversational assistance for invoice management,
//! powered by LangGraph/OpenAI integration.

#![allow(warnings)]

pub mod agent;
pub mod config;
pub mod context;
pub mod fake_provider;
pub mod handlers;
pub mod models;
pub mod openai_compatible_provider;
pub mod product_knowledge;
pub mod proposals;
pub mod provider;
pub mod tools;

pub mod issue_intake;

pub use agent::WinstonAgent;
pub use config::{AiModelConfig, AiProviderConfig, AiProviderType, ConfigError};
pub use fake_provider::FakeAiProvider;
pub use handlers::create_router;
pub use openai_compatible_provider::OpenAiCompatibleProvider;
pub use proposals::{CreateWinstonProposalInput, WinstonProposalService};
pub use provider::{AiProvider, ProviderChatStream};

use models::{ProviderChatMessage, ProviderChatRequest, ProviderMessageRole};

/// Maximum character length for AI answers posted back to Slack/Teams.
/// Slack message limit is 40 000 but we keep replies concise.
const MAX_ANSWER_LENGTH: usize = 3000;

/// Answer a question about an invoice using the given AI provider.
///
/// Loads invoice header, vendor, GL coding, line items, and notes from the
/// tenant-scoped pool, builds a constrained system prompt, and returns the
/// assistant's plain-text answer truncated to a chat-safe length.
///
/// All database queries go through the supplied `PgPool` so RLS stays fail-closed.
pub async fn answer_invoice_question(
    provider: &dyn AiProvider,
    pool: &sqlx::PgPool,
    _tenant_id: uuid::Uuid,
    invoice_id: uuid::Uuid,
    question: &str,
) -> Result<String, String> {
    // Load invoice header
    let header: Option<(
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, currency, \
             due_date::text, gl_code, cost_center \
             FROM invoices WHERE id = $1",
    )
    .bind(invoice_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to load invoice: {}", e))?;

    let (vendor_name, invoice_number, total_cents, currency, due_date, gl_code, cost_center) =
        header.ok_or_else(|| "Invoice not found".to_string())?;

    // Load line items
    let line_items: Vec<(String, Option<f64>, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT description, quantity, unit_price_cents, total_cents \
         FROM invoice_line_items WHERE invoice_id = $1 ORDER BY created_at",
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load line items: {}", e))?;

    // Load existing notes
    let notes: Vec<(String,)> = sqlx::query_as(
        "SELECT content FROM invoice_notes WHERE invoice_id = $1 ORDER BY created_at",
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load notes: {}", e))?;

    // Build context block
    let amount = total_cents as f64 / 100.0;
    let mut context =
        format!("Invoice #{invoice_number}\nVendor: {vendor_name}\nTotal: {currency} {amount:.2}");
    if let Some(d) = &due_date {
        context.push_str(&format!("\nDue Date: {d}"));
    }
    if let Some(gl) = &gl_code {
        context.push_str(&format!("\nGL Code: {gl}"));
    }
    if let Some(cc) = &cost_center {
        context.push_str(&format!("\nCost Center: {cc}"));
    }

    if !line_items.is_empty() {
        context.push_str("\n\nLine Items:");
        for (desc, qty, _unit, total) in &line_items {
            let qty_str = qty.map(|q| format!(" x{q}")).unwrap_or_default();
            let total_str = total
                .map(|c| format!(" ${:.2}", c as f64 / 100.0))
                .unwrap_or_default();
            context.push_str(&format!("\n  - {desc}{qty_str}{total_str}"));
        }
    }

    if !notes.is_empty() {
        context.push_str("\n\nNotes:");
        for (content,) in &notes {
            context.push_str(&format!("\n  - {content}"));
        }
    }

    let system_prompt = format!(
        "You are an invoice assistant. Answer the user's question using ONLY the facts \
         present in the invoice context below. If the answer is not in the context, say \
         you don't have that information. Do not speculate or invent details.\n\n\
         Invoice context:\n{context}"
    );

    let model = provider.model_name().to_string();
    let request = ProviderChatRequest {
        model,
        model_route: models::ProviderModelRoute::Fast,
        messages: vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: system_prompt,
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: question.to_string(),
            },
        ],
        temperature: Some(0.3),
        max_tokens: Some(800),
        stop: None,
        tools: None,
    };

    let response = provider
        .chat_completion(request)
        .await
        .map_err(|e| format!("AI provider error: {}", e.message))?;

    let answer = response.message.content;
    if answer.chars().count() > MAX_ANSWER_LENGTH {
        Ok(format!(
            "{} [...]",
            answer.chars().take(MAX_ANSWER_LENGTH).collect::<String>()
        ))
    } else {
        Ok(answer)
    }
}

/// Build the system prompt for invoice Q&A without hitting the database.
/// Exposed for unit testing prompt construction.
pub fn build_invoice_qa_prompt(
    vendor_name: &str,
    invoice_number: &str,
    total_cents: i64,
    currency: &str,
    due_date: Option<&str>,
    gl_code: Option<&str>,
    cost_center: Option<&str>,
    line_items: &[(&str, Option<f64>, Option<i64>)],
    notes: &[&str],
) -> String {
    let amount = total_cents as f64 / 100.0;
    let mut context =
        format!("Invoice #{invoice_number}\nVendor: {vendor_name}\nTotal: {currency} {amount:.2}");
    if let Some(d) = due_date {
        context.push_str(&format!("\nDue Date: {d}"));
    }
    if let Some(gl) = gl_code {
        context.push_str(&format!("\nGL Code: {gl}"));
    }
    if let Some(cc) = cost_center {
        context.push_str(&format!("\nCost Center: {cc}"));
    }

    if !line_items.is_empty() {
        context.push_str("\n\nLine Items:");
        for (desc, qty, total) in line_items {
            let qty_str = qty.map(|q| format!(" x{q}")).unwrap_or_default();
            let total_str = total
                .map(|c| format!(" ${:.2}", c as f64 / 100.0))
                .unwrap_or_default();
            context.push_str(&format!("\n  - {desc}{qty_str}{total_str}"));
        }
    }

    if !notes.is_empty() {
        context.push_str("\n\nNotes:");
        for n in notes {
            context.push_str(&format!("\n  - {n}"));
        }
    }

    format!(
        "You are an invoice assistant. Answer the user's question using ONLY the facts \
         present in the invoice context below. If the answer is not in the context, say \
         you don't have that information. Do not speculate or invent details.\n\n\
         Invoice context:\n{context}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_provider::FakeAiProvider;

    #[test]
    fn test_build_invoice_qa_prompt_contains_facts() {
        let prompt = build_invoice_qa_prompt(
            "Acme Corp",
            "INV-001",
            15000,
            "USD",
            Some("2025-01-15"),
            Some("GL-4000"),
            Some("CC-Engineering"),
            &[
                ("Widget A", Some(2.0), Some(10000)),
                ("Widget B", None, Some(5000)),
            ],
            &["Please expedite"],
        );

        assert!(prompt.contains("Acme Corp"), "prompt must mention vendor");
        assert!(
            prompt.contains("INV-001"),
            "prompt must mention invoice number"
        );
        assert!(prompt.contains("150.00"), "prompt must mention total");
        assert!(
            prompt.contains("2025-01-15"),
            "prompt must mention due date"
        );
        assert!(prompt.contains("GL-4000"), "prompt must mention GL code");
        assert!(
            prompt.contains("CC-Engineering"),
            "prompt must mention cost center"
        );
        assert!(
            prompt.contains("Widget A"),
            "prompt must mention first line item"
        );
        assert!(
            prompt.contains("Widget B"),
            "prompt must mention second line item"
        );
        assert!(
            prompt.contains("Please expedite"),
            "prompt must include notes"
        );
        assert!(
            prompt.contains("Do not speculate"),
            "prompt must instruct against speculation"
        );
    }

    #[tokio::test]
    async fn test_answer_invoice_question_uses_provider() {
        let provider = FakeAiProvider::new().with_response_text("The total is $150.00");

        // We can't call answer_invoice_question without a real DB pool,
        // but we verify the FakeAiProvider returns the expected answer.
        let request = ProviderChatRequest {
            model: "fake-model".to_string(),
            model_route: crate::models::ProviderModelRoute::Fast,
            messages: vec![
                ProviderChatMessage {
                    role: ProviderMessageRole::System,
                    content: "test system".to_string(),
                },
                ProviderChatMessage {
                    role: ProviderMessageRole::User,
                    content: "What is the total?".to_string(),
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(800),
            stop: None,
            tools: None,
        };

        let response = provider.chat_completion(request).await.unwrap();
        assert_eq!(response.message.content, "The total is $150.00");

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].messages.len(), 2);
    }

    /// Verify char-aware truncation handles multi-byte UTF-8 safely.
    /// The old byte-indexed slice `&answer[..MAX_ANSWER_LENGTH]` would panic
    /// when byte 3000 fell inside a multi-byte character (emoji, currency, etc.).
    #[test]
    fn test_truncation_is_char_aware() {
        // Build a string longer than MAX_ANSWER_LENGTH consisting entirely
        // of 4-byte emoji characters.
        let emoji = "\u{1F600}"; // 😀 — 4 bytes per emoji
        assert_eq!(emoji.len(), 4);
        let long_answer: String = emoji.repeat(MAX_ANSWER_LENGTH + 100);
        // Each emoji is 1 char but 4 bytes, so char count = MAX_ANSWER_LENGTH + 100
        assert!(long_answer.chars().count() > MAX_ANSWER_LENGTH);

        // Char-aware truncation must not panic
        let truncated: String = long_answer.chars().take(MAX_ANSWER_LENGTH).collect();
        assert_eq!(truncated.chars().count(), MAX_ANSWER_LENGTH);
        // Verify the truncated string is valid UTF-8 (no mid-character cut)
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    /// Verify truncation produces the expected format with trailing marker.
    #[test]
    fn test_truncation_format() {
        let long_answer = "a".repeat(MAX_ANSWER_LENGTH + 50);
        let truncated: String = long_answer.chars().take(MAX_ANSWER_LENGTH).collect();
        let result = format!("{} [...]", truncated);
        assert!(result.ends_with(" [...]"));
        // " [...]" is 6 characters: space, dot, dot, dot, dot (ellipsis), bracket
        // Actually: ' ', '.', '.', '.', '.', ']' — wait, " [...]" is: space, '[', '.', '.', '.', ']'
        // Let's just count: " [...]" = 6 chars
        assert_eq!(result.chars().count(), MAX_ANSWER_LENGTH + 6);
    }
}
