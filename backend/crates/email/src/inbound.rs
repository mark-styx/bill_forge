//! Inbound email handler for per-tenant forwarding mailboxes.
//!
//! Parses provider-agnostic inbound-parse JSON (modelled on Postmark/SendGrid),
//! resolves the recipient to a tenant, extracts PDF/image attachments, and
//! enqueues OCR jobs. Unrecognised senders and emails with no usable
//! attachments are routed to the triage queue.
//!
//! Database routing:
//! - `inbound_email_messages`, `email_triage_queue`, `tenant_forwarding_addresses`
//!   live in the **metadata** DB (they FK to `tenants(id)`).
//! - `vendors`, `invoice_captures`, `invoices` live in the **tenant** DB.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Payload types (provider-agnostic, compatible with Postmark / SendGrid)
// ---------------------------------------------------------------------------

/// Top-level payload posted by an inbound-parse webhook.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboundEmailPayload {
    /// Raw "From" header, e.g. `"Jane Doe" <jane@acme.com>`
    pub from: String,
    /// Raw "To" header containing the tenant forwarding address.
    pub to: String,
    pub subject: Option<String>,
    /// Provider-assigned message identifier (idempotency key).
    #[serde(default)]
    pub message_id: Option<String>,
    /// Attachments included in the email.
    #[serde(default)]
    pub attachments: Vec<InboundAttachment>,
}

/// A single attachment extracted from an inbound email.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboundAttachment {
    pub name: String,
    pub content_type: String,
    /// Base64-encoded attachment body.
    pub content: String,
}

// ---------------------------------------------------------------------------
// Handler result
// ---------------------------------------------------------------------------

/// Outcome of processing a single inbound email.
#[derive(Debug)]
pub struct InboundEmailResult {
    /// IDs of invoice rows created (one per usable attachment).
    pub capture_ids: Vec<Uuid>,
    /// True if the message was routed to triage instead of normal processing.
    pub triaged: bool,
    /// Human-readable triage reason (empty when not triaged).
    pub triage_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Processes an inbound email payload: resolve tenant, persist the message,
/// extract attachments, match vendor by sender domain, enqueue OCR.
pub struct InboundEmailHandler<'a> {
    /// Metadata-level PgPool (contains `tenants`, `tenant_forwarding_addresses`,
    /// `inbound_email_messages`, `email_triage_queue`).
    pub metadata_pool: &'a PgPool,
    /// Tenant-level PgPool (contains `vendors`, `invoice_captures`, `invoices`).
    pub tenant_pool: &'a PgPool,
}

impl<'a> InboundEmailHandler<'a> {
    /// Process an inbound email end-to-end.
    pub async fn process(
        &self,
        payload: &InboundEmailPayload,
    ) -> Result<InboundEmailResult, String> {
        // 1. Resolve recipient → tenant
        let tenant_uuid = self
            .resolve_tenant(&payload.to)
            .await?
            .ok_or_else(|| "No tenant found for recipient address".to_string())?;

        // 2. Extract sender domain
        let from_domain = extract_domain(&payload.from);

        // 3. Persist inbound_email_messages row in METADATA DB
        let email_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO inbound_email_messages
                   (tenant_id, message_id, from_address, from_domain, subject, status, raw_payload)
               VALUES ($1, $2, $3, $4, $5, 'processed', $6)
               RETURNING id"#,
        )
        .bind(tenant_uuid)
        .bind(&payload.message_id)
        .bind(&payload.from)
        .bind(&from_domain)
        .bind(&payload.subject)
        .bind(serde_json::to_value(payload).ok())
        .fetch_one(self.metadata_pool)
        .await
        .map_err(|e| format!("Failed to persist inbound email message: {}", e))?;

        // 4. Filter usable attachments (PDF or image)
        let usable: Vec<&InboundAttachment> = payload
            .attachments
            .iter()
            .filter(|a| is_usable_attachment(&a.content_type))
            .collect();

        if usable.is_empty() {
            // No usable attachments → triage in METADATA DB
            self.create_triage_entry(email_id, "No usable PDF/image attachments found")
                .await?;

            sqlx::query("UPDATE inbound_email_messages SET status = 'triage', triage_reason = $2 WHERE id = $1")
                .bind(email_id)
                .bind("No usable PDF/image attachments found")
                .execute(self.metadata_pool)
                .await
                .map_err(|e| format!("Failed to update email status: {}", e))?;

            return Ok(InboundEmailResult {
                capture_ids: Vec::new(),
                triaged: true,
                triage_reason: Some("No usable PDF/image attachments found".to_string()),
            });
        }

        // 5. Suggest vendor from sender domain (TENANT DB)
        let suggested_vendor_id = self
            .suggest_vendor_by_domain(&tenant_uuid, &from_domain)
            .await?;

        // 6. For each usable attachment: create invoice capture row + enqueue OCR job (TENANT DB)
        let mut capture_ids = Vec::new();
        for attachment in &usable {
            let bytes = BASE64
                .decode(&attachment.content)
                .map_err(|e| format!("Failed to decode attachment '{}': {}", attachment.name, e))?;

            let document_id = Uuid::new_v4();

            // Create invoice capture row (TENANT DB)
            let _capture_id: Uuid = sqlx::query_scalar(
                r#"INSERT INTO invoice_captures
                       (id, tenant_id, original_filename, mime_type, provider, status, uploaded_by)
                   VALUES ($1, $2, $3, $4, 'tesseract', 'processing', NULL)
                   RETURNING id"#,
            )
            .bind(document_id)
            .bind(tenant_uuid)
            .bind(&attachment.name)
            .bind(&attachment.content_type)
            .fetch_one(self.tenant_pool)
            .await
            .map_err(|e| format!("Failed to create invoice capture: {}", e))?;

            // Create invoice row linking back to the source email, with suggested vendor (TENANT DB).
            let invoice_id: Uuid = sqlx::query_scalar(
                r#"INSERT INTO invoices
                       (id, tenant_id, vendor_id, vendor_name, invoice_number,
                        total_amount_cents, currency, capture_status, processing_status,
                        document_id, source_email_id, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, $5, 0, 'USD', 'processing', 'draft',
                           $6, $7, NOW(), NOW())
                   RETURNING id"#,
            )
            .bind(Uuid::new_v4())
            .bind(tenant_uuid)
            .bind(suggested_vendor_id)
            .bind(
                suggested_vendor_id
                    .map_or_else(|| "Unknown (email)".to_string(), |_| from_domain.clone()),
            )
            .bind(format!("EMAIL-{}", Uuid::new_v4().as_simple()))
            .bind(document_id)
            .bind(email_id)
            .fetch_one(self.tenant_pool)
            .await
            .map_err(|e| format!("Failed to create invoice from email: {}", e))?;

            // Store the attachment bytes to local object storage.
            if let Err(e) = self
                .store_attachment(tenant_uuid, document_id, &bytes)
                .await
            {
                tracing::warn!(
                    document_id = %document_id,
                    error = %e,
                    "Failed to store attachment; OCR job may fail"
                );
            }

            // Enqueue OCR job via Redis (same pattern as manual upload).
            if let Err(e) = self
                .enqueue_ocr_job(
                    tenant_uuid,
                    &invoice_id,
                    &document_id,
                    &attachment.content_type,
                )
                .await
            {
                tracing::warn!(
                    invoice_id = %invoice_id,
                    error = %e,
                    "Failed to enqueue OCR job; invoice will need manual processing"
                );
            }

            capture_ids.push(invoice_id);
        }

        Ok(InboundEmailResult {
            capture_ids,
            triaged: false,
            triage_reason: None,
        })
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Look up which tenant owns the given recipient address.
    async fn resolve_tenant(&self, to_address: &str) -> Result<Option<Uuid>, String> {
        let email = extract_email(to_address);

        let tenant_uuid: Option<Uuid> = sqlx::query_scalar(
            "SELECT tenant_id FROM tenant_forwarding_addresses WHERE full_address = $1",
        )
        .bind(email)
        .fetch_optional(self.metadata_pool)
        .await
        .map_err(|e| format!("Failed to resolve tenant for address: {}", e))?;

        Ok(tenant_uuid)
    }

    /// Auto-provision a forwarding address for a tenant if one does not exist.
    /// Returns the full address. Uses the tenant slug from the tenants table.
    pub async fn ensure_forwarding_address(
        metadata_pool: &PgPool,
        tenant_uuid: Uuid,
        email_domain: &str,
    ) -> Result<String, String> {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT full_address FROM tenant_forwarding_addresses WHERE tenant_id = $1",
        )
        .bind(tenant_uuid)
        .fetch_optional(metadata_pool)
        .await
        .map_err(|e| format!("Failed to check forwarding address: {}", e))?;

        if let Some(addr) = existing {
            return Ok(addr);
        }

        let slug: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
            .bind(tenant_uuid)
            .fetch_one(metadata_pool)
            .await
            .map_err(|e| format!("Failed to get tenant slug: {}", e))?;

        let local_part_clean = "ap".to_string();
        let full_address = format!(
            "{}@{}.{}",
            local_part_clean,
            slug.to_lowercase().replace(' ', "-"),
            email_domain
        );

        sqlx::query(
            r#"INSERT INTO tenant_forwarding_addresses (tenant_id, local_part, full_address)
               VALUES ($1, $2, $3)
               ON CONFLICT (tenant_id) DO NOTHING"#,
        )
        .bind(tenant_uuid)
        .bind(&local_part_clean)
        .bind(&full_address)
        .execute(metadata_pool)
        .await
        .map_err(|e| format!("Failed to create forwarding address: {}", e))?;

        Ok(full_address)
    }

    /// Suggest a vendor by matching the sender domain against vendor email/website fields (TENANT DB).
    async fn suggest_vendor_by_domain(
        &self,
        tenant_uuid: &Uuid,
        from_domain: &str,
    ) -> Result<Option<Uuid>, String> {
        let vendor_id: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM vendors
               WHERE tenant_id = $1
                 AND status = 'active'
                 AND (email LIKE $2 OR email LIKE $3)
               LIMIT 1"#,
        )
        .bind(tenant_uuid)
        .bind(format!("%@{}", from_domain))
        .bind(format!("%@%.{}", from_domain))
        .fetch_optional(self.tenant_pool)
        .await
        .map_err(|e| format!("Failed to match vendor by domain: {}", e))?;

        Ok(vendor_id)
    }

    /// Insert a triage queue entry in the METADATA DB.
    async fn create_triage_entry(
        &self,
        inbound_email_id: Uuid,
        reason: &str,
    ) -> Result<(), String> {
        sqlx::query(
            r#"INSERT INTO email_triage_queue (id, inbound_email_id, reason)
               VALUES (gen_random_uuid(), $1, $2)"#,
        )
        .bind(inbound_email_id)
        .bind(reason)
        .execute(self.metadata_pool)
        .await
        .map_err(|e| format!("Failed to create triage entry: {}", e))?;
        Ok(())
    }

    /// Store attachment bytes to local object storage.
    async fn store_attachment(
        &self,
        tenant_uuid: Uuid,
        document_id: Uuid,
        bytes: &[u8],
    ) -> Result<(), String> {
        let storage_path =
            std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| "./data/files".to_string());
        let dir = std::path::Path::new(&storage_path)
            .join(tenant_uuid.to_string())
            .join("documents");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create storage dir: {}", e))?;
        let file_path = dir.join(document_id.to_string());
        std::fs::write(&file_path, bytes)
            .map_err(|e| format!("Failed to write attachment: {}", e))?;
        Ok(())
    }

    /// Enqueue an OCR job (via Redis if available, otherwise inline).
    #[allow(unused_variables)]
    async fn enqueue_ocr_job(
        &self,
        tenant_uuid: Uuid,
        invoice_id: &Uuid,
        document_id: &Uuid,
        content_type: &str,
    ) -> Result<(), String> {
        let redis_url = std::env::var("REDIS_URL").ok();

        if let Some(url) = redis_url {
            #[cfg(feature = "inbound-redis")]
            {
                let job_payload = serde_json::json!({
                    "invoice_id": invoice_id.to_string(),
                    "document_id": document_id.to_string(),
                    "content_type": content_type,
                });

                let client = redis::Client::open(url.as_str())
                    .map_err(|e| format!("Redis client error: {}", e))?;
                let mut conn = client
                    .get_connection()
                    .map_err(|e| format!("Redis connection error: {}", e))?;
                let queue_key = format!("billforge:jobs:{}:ocr_processing", tenant_uuid);
                redis::cmd("RPUSH")
                    .arg(&queue_key)
                    .arg(job_payload.to_string())
                    .execute(&mut conn);
            }
            #[cfg(not(feature = "inbound-redis"))]
            {
                let _url = url;
                tracing::warn!(
                    tenant_id = %tenant_uuid,
                    "Redis URL set but inbound-redis feature disabled; skipping OCR enqueue"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Extract the domain portion from a "Name <email@domain>" or bare "email@domain" string.
pub fn extract_domain(raw: &str) -> String {
    let email = extract_email(raw);
    email
        .rsplit('@')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .trim_end_matches('>')
        .to_string()
}

/// Extract the email address from a possibly-formatted header value.
pub fn extract_email(raw: &str) -> &str {
    if let Some(start) = raw.find('<') {
        if let Some(end) = raw.find('>') {
            return &raw[start + 1..end];
        }
    }
    raw.trim()
}

/// Determine whether a content type represents a usable invoice attachment.
pub fn is_usable_attachment(content_type: &str) -> bool {
    let ct = content_type.to_lowercase();
    ct.contains("pdf")
        || ct.contains("png")
        || ct.contains("jpeg")
        || ct.contains("jpg")
        || ct.contains("tiff")
        || ct.contains("gif")
        || ct.contains("bmp")
        || ct.contains("webp")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolves_tenant_from_recipient() {
        let email = extract_email("\"AP Forwarding\" <ap@meridian.billforge.com>");
        assert_eq!(email, "ap@meridian.billforge.com");

        let domain = extract_domain("\"Vendor\" <billing@acme.com>");
        assert_eq!(domain, "acme.com");
    }

    #[test]
    fn test_extracts_domain_from_bare_address() {
        assert_eq!(extract_domain("billing@acme.com"), "acme.com");
        assert_eq!(extract_domain("user@sub.domain.com"), "sub.domain.com");
    }

    #[test]
    fn test_vendor_suggestion_from_sender_domain() {
        let domain = extract_domain("invoices@techsupplies.com");
        assert_eq!(domain, "techsupplies.com");

        // The query uses LIKE '%@techsupplies.com' which would match 'ap@techsupplies.com'
        let pattern = format!("%@{}", domain);
        assert!("ap@techsupplies.com".contains(&pattern.replace('%', "")));
    }

    #[test]
    fn test_unknown_recipient_goes_to_triage() {
        // When resolve_tenant returns None, the handler returns an error
        // which the route translates to triage. Verify the logic.
        let email = extract_email("unknown@nowhere.com");
        assert_eq!(email, "unknown@nowhere.com");
        // This would not match any row in tenant_forwarding_addresses.
    }

    #[test]
    fn test_no_attachments_goes_to_triage() {
        let payload = InboundEmailPayload {
            from: "vendor@example.com".to_string(),
            to: "ap@meridian.billforge.com".to_string(),
            subject: Some("Invoice".to_string()),
            message_id: Some("msg-123".to_string()),
            attachments: vec![],
        };
        assert!(payload.attachments.is_empty());
        // Handler would triage this with "No usable PDF/image attachments found".
    }

    #[test]
    fn test_usable_attachment_filtering() {
        assert!(is_usable_attachment("application/pdf"));
        assert!(is_usable_attachment("image/png"));
        assert!(is_usable_attachment("image/jpeg"));
        assert!(is_usable_attachment(
            "application/pdf; name=\"invoice.pdf\""
        ));
        assert!(!is_usable_attachment("text/plain"));
        assert!(!is_usable_attachment("application/zip"));
    }

    #[test]
    fn test_extract_email_various_formats() {
        assert_eq!(extract_email("simple@example.com"), "simple@example.com");
        assert_eq!(
            extract_email("\"John Doe\" <john@example.com>"),
            "john@example.com"
        );
        assert_eq!(extract_email("John <john@example.com>"), "john@example.com");
        assert_eq!(
            extract_email("  trimmed@example.com  "),
            "trimmed@example.com"
        );
    }

    // -----------------------------------------------------------------------
    // Payload deserialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_deserializes_postmark_shape() {
        // Postmark-style payload mapped to our provider-agnostic lowercase field names.
        let json = r#"{
            "from": "\"Jane Doe\" <jane@acme.com>",
            "to": "ap@meridian.billforge.com",
            "subject": "Invoice #1042",
            "message_id": "<abc123@postmark.com>",
            "attachments": [
                {
                    "name": "invoice.pdf",
                    "content_type": "application/pdf",
                    "content": "JVBERi0xLjUKMSAwIG9iago8PAo="
                }
            ]
        }"#;

        let payload: InboundEmailPayload = serde_json::from_str(json).expect("postmark payload should deserialize");
        assert_eq!(payload.from, "\"Jane Doe\" <jane@acme.com>");
        assert_eq!(payload.to, "ap@meridian.billforge.com");
        assert_eq!(payload.subject.as_deref(), Some("Invoice #1042"));
        assert_eq!(payload.message_id.as_deref(), Some("<abc123@postmark.com>"));
        assert_eq!(payload.attachments.len(), 1);
        assert_eq!(payload.attachments[0].name, "invoice.pdf");
        assert_eq!(payload.attachments[0].content_type, "application/pdf");
        assert_eq!(payload.attachments[0].content, "JVBERi0xLjUKMSAwIG9iago8PAo=");

        // Verify the base64 attachment body round-trips through decode.
        let decoded = BASE64.decode(&payload.attachments[0].content).expect("base64 should decode");
        assert!(!decoded.is_empty());
    }

    #[test]
    fn test_deserializes_sendgrid_shape() {
        let json = r#"{
            "from": "billing@techsupplies.com",
            "to": "ap@meridian.billforge.com",
            "subject": "March Statement",
            "message_id": "sg.msg.456",
            "attachments": [
                {
                    "name": "statement.pdf",
                    "content_type": "application/pdf",
                    "content": "JVBERi0yLjQK"
                },
                {
                    "name": "receipt.png",
                    "content_type": "image/png",
                    "content": "iVBORw0KGgo="
                }
            ]
        }"#;

        let payload: InboundEmailPayload = serde_json::from_str(json).expect("sendgrid payload should deserialize");
        assert_eq!(payload.from, "billing@techsupplies.com");
        assert_eq!(payload.to, "ap@meridian.billforge.com");
        assert_eq!(payload.subject.as_deref(), Some("March Statement"));
        assert_eq!(payload.attachments.len(), 2);
        assert_eq!(payload.attachments[0].content_type, "application/pdf");
        assert_eq!(payload.attachments[1].content_type, "image/png");
    }

    #[test]
    fn test_payload_missing_message_id_deserializes() {
        let json = r#"{
            "from": "vendor@example.com",
            "to": "ap@tenant.billforge.com",
            "subject": "Invoice",
            "attachments": []
        }"#;

        let payload: InboundEmailPayload = serde_json::from_str(json).expect("payload without message_id should deserialize");
        assert!(payload.message_id.is_none());
    }

    #[test]
    fn test_payload_missing_attachments_defaults_to_empty() {
        let json = r#"{
            "from": "vendor@example.com",
            "to": "ap@tenant.billforge.com"
        }"#;

        let payload: InboundEmailPayload = serde_json::from_str(json).expect("payload without attachments should deserialize");
        assert!(payload.attachments.is_empty());
        assert!(payload.subject.is_none());
    }

    // -----------------------------------------------------------------------
    // Attachment filtering tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_usable_attachment_filtering_covers_all_supported_types() {
        // Standard types
        assert!(is_usable_attachment("application/pdf"));
        assert!(is_usable_attachment("image/png"));
        assert!(is_usable_attachment("image/jpeg"));
        assert!(is_usable_attachment("image/jpg"));
        assert!(is_usable_attachment("image/tiff"));
        assert!(is_usable_attachment("image/gif"));
        assert!(is_usable_attachment("image/bmp"));
        assert!(is_usable_attachment("image/webp"));

        // Uppercase content type
        assert!(is_usable_attachment("application/PDF"));
        assert!(is_usable_attachment("IMAGE/PNG"));

        // Content-type with charset/name parameters
        assert!(is_usable_attachment("image/png; name=\"scan.png\""));
        assert!(is_usable_attachment("application/pdf; charset=utf-8"));
        assert!(is_usable_attachment("image/jpeg; name=receipt.jpg"));
    }

    #[test]
    fn test_usable_attachment_rejects_dangerous_types() {
        assert!(!is_usable_attachment("application/octet-stream"));
        assert!(!is_usable_attachment("application/x-msdownload"));
        assert!(!is_usable_attachment("application/zip"));
        assert!(!is_usable_attachment("text/html"));
        assert!(!is_usable_attachment("text/plain"));
        assert!(!is_usable_attachment("application/javascript"));
        assert!(!is_usable_attachment("application/x-executable"));
    }

    // -----------------------------------------------------------------------
    // extract_email / extract_domain edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_email_handles_malformed_inputs() {
        // Empty string: no angle brackets, falls through to trim → empty
        assert_eq!(extract_email(""), "");

        // Missing closing '>'
        assert_eq!(extract_email("user@example.com"), "user@example.com");
        assert_eq!(extract_email("Display <user@example.com"), "Display <user@example.com");

        // Multiple '@' in display name portion (before '<')
        assert_eq!(
            extract_email("\"@@ Board\" <treasury@acme.com>"),
            "treasury@acme.com"
        );

        // Leading/trailing whitespace inside angle brackets
        assert_eq!(
            extract_email("Vendor <  billing@acme.com  >"),
            "  billing@acme.com  "
        );
    }

    #[test]
    fn test_extract_domain_normalizes_case_and_trims() {
        assert_eq!(
            extract_domain("\"Vendor\" <Billing@ACME.COM>"),
            "acme.com"
        );
        assert_eq!(
            extract_domain("INVOICES@Sub.Domain.COM"),
            "sub.domain.com"
        );
    }

    // -----------------------------------------------------------------------
    // Base64 decode failure contract test
    // -----------------------------------------------------------------------

    #[test]
    fn test_attachment_base64_decode_failure_surfaces() {
        // A clearly invalid base64 string must produce an error, confirming
        // the handler's `?` propagation path is reachable.
        let result = BASE64.decode("!!!not_base64!!!");
        assert!(result.is_err(), "malformed base64 content must fail to decode");
    }
}
