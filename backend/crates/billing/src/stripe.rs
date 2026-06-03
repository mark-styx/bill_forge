//! Stripe API integration
//!
//! This module provides integration with Stripe for payment processing,
//! making real HTTP calls to the Stripe API and cryptographically verifying
//! webhook signatures.

use billforge_core::{Error, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tracing::{debug, info};

type HmacSha256 = Hmac<Sha256>;

/// Stripe API client
pub struct StripeClient {
    api_key: String,
    client: Client,
    base_url: String,
}

impl StripeClient {
    /// Create a new Stripe client pointing at the live Stripe API.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
            base_url: "https://api.stripe.com/v1".to_string(),
        }
    }

    /// Create a new Stripe client with a custom base URL (for tests).
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
            base_url,
        }
    }

    /// Create a new Stripe client for testing
    #[cfg(test)]
    pub fn new_test() -> Self {
        Self::new("sk_test_fake".to_string())
    }

    /// Build an authenticated POST request with form-encoded body.
    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.api_key)
            .header("content-type", "application/x-www-form-urlencoded")
    }

    /// Build an authenticated GET request.
    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.api_key)
    }

    /// Build an authenticated DELETE request.
    fn delete(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .delete(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.api_key)
    }

    /// Check a Stripe response for errors, returning the text body on success.
    async fn handle_response(response: reqwest::Response) -> Result<String> {
        let status = response.status();
        let body = response.text().await.map_err(|e| Error::ExternalService {
            service: "stripe".to_string(),
            message: format!("failed to read response body: {}", e),
        })?;
        if !status.is_success() {
            return Err(Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }
        Ok(body)
    }

    /// Create a Stripe customer
    pub async fn create_customer(&self, params: CreateCustomerParams) -> Result<StripeCustomer> {
        info!(email = %params.email, "Creating Stripe customer");

        let mut form: Vec<(String, String)> = vec![("email".to_string(), params.email)];

        if let Some(name) = &params.name {
            form.push(("name".to_string(), name.clone()));
        }

        for (k, v) in &params.metadata {
            form.push((format!("metadata[{}]", k), v.clone()));
        }

        let resp = self
            .post("/customers")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: StripeCustomerResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("failed to parse customer response: {} - body: {}", e, body),
            })?;

        Ok(StripeCustomer {
            id: raw.id,
            email: raw.email,
            name: raw.name,
            metadata: raw.metadata.unwrap_or_default(),
        })
    }

    /// Create a checkout session for subscription
    pub async fn create_checkout_session(
        &self,
        params: CreateCheckoutSessionParams,
    ) -> Result<CheckoutSession> {
        info!(
            customer_id = %params.customer_id,
            price_id = %params.price_id,
            "Creating checkout session"
        );

        let mut form: Vec<(String, String)> = vec![
            ("customer".to_string(), params.customer_id),
            ("line_items[0][price]".to_string(), params.price_id),
            ("line_items[0][quantity]".to_string(), "1".to_string()),
            ("mode".to_string(), params.mode),
            ("success_url".to_string(), params.success_url),
            ("cancel_url".to_string(), params.cancel_url),
        ];

        for (i, item) in params.additional_line_items.iter().enumerate() {
            let idx = i + 1; // base plan is [0], add-ons start at [1]
            form.push((format!("line_items[{}][price]", idx), item.price_id.clone()));
            form.push((format!("line_items[{}][quantity]", idx), item.quantity.to_string()));
        }
        info!(additional_line_items = params.additional_line_items.len(), "Checkout session line items");

        for (k, v) in &params.metadata {
            form.push((format!("metadata[{}]", k), v.clone()));
        }

        let resp = self
            .post("/checkout/sessions")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: CheckoutSessionResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!(
                    "failed to parse checkout session response: {} - body: {}",
                    e, body
                ),
            })?;

        Ok(CheckoutSession {
            id: raw.id,
            url: raw.url,
            customer_id: raw.customer,
            status: raw.status,
        })
    }

    /// Create a customer portal session
    pub async fn create_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
    ) -> Result<PortalSession> {
        info!(customer_id = %customer_id, "Creating customer portal session");

        let form: Vec<(String, String)> = vec![
            ("customer".to_string(), customer_id.to_string()),
            ("return_url".to_string(), return_url.to_string()),
        ];

        let resp = self
            .post("/billing_portal/sessions")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: PortalSessionResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!(
                    "failed to parse portal session response: {} - body: {}",
                    e, body
                ),
            })?;

        Ok(PortalSession {
            id: raw.id,
            url: raw.url,
        })
    }

    /// Get subscription details
    pub async fn get_subscription(&self, subscription_id: &str) -> Result<StripeSubscription> {
        debug!(subscription_id = %subscription_id, "Getting subscription");

        let resp = self
            .get(&format!("/subscriptions/{}", subscription_id))
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: StripeSubscriptionResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!(
                    "failed to parse subscription response: {} - body: {}",
                    e, body
                ),
            })?;

        Ok(StripeSubscription {
            id: raw.id,
            customer: raw.customer,
            status: raw.status,
            current_period_start: raw.current_period_start,
            current_period_end: raw.current_period_end,
            cancel_at_period_end: raw.cancel_at_period_end,
            items: raw
                .items
                .data
                .into_iter()
                .map(|i| SubscriptionItem {
                    id: i.id,
                    price_id: i.price.id,
                    quantity: i.quantity,
                })
                .collect(),
        })
    }

    /// Cancel a subscription
    pub async fn cancel_subscription(&self, subscription_id: &str) -> Result<StripeSubscription> {
        info!(subscription_id = %subscription_id, "Canceling subscription");

        let resp = self
            .delete(&format!("/subscriptions/{}", subscription_id))
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: StripeSubscriptionResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!(
                    "failed to parse cancel subscription response: {} - body: {}",
                    e, body
                ),
            })?;

        Ok(StripeSubscription {
            id: raw.id,
            customer: raw.customer,
            status: raw.status,
            current_period_start: raw.current_period_start,
            current_period_end: raw.current_period_end,
            cancel_at_period_end: raw.cancel_at_period_end,
            items: raw
                .items
                .data
                .into_iter()
                .map(|i| SubscriptionItem {
                    id: i.id,
                    price_id: i.price.id,
                    quantity: i.quantity,
                })
                .collect(),
        })
    }

    /// Report a usage event to Stripe Billing Meter Events.
    pub async fn create_meter_event(
        &self,
        params: CreateMeterEventParams,
    ) -> Result<StripeMeterEvent> {
        info!(
            event_name = %params.event_name,
            stripe_customer_id = %params.stripe_customer_id,
            value = params.value,
            "Creating Stripe meter event"
        );

        let mut form: Vec<(String, String)> = vec![
            ("event_name".to_string(), params.event_name),
            (
                "payload[stripe_customer_id]".to_string(),
                params.stripe_customer_id,
            ),
            ("payload[value]".to_string(), params.value.to_string()),
            ("identifier".to_string(), params.identifier),
        ];

        if let Some(timestamp) = params.timestamp {
            form.push(("timestamp".to_string(), timestamp.to_string()));
        }

        for (k, v) in &params.payload {
            form.push((format!("payload[{}]", k), v.clone()));
        }

        let resp = self
            .post("/billing/meter_events")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!("request failed: {}", e),
            })?;
        let body = Self::handle_response(resp).await?;

        let raw: StripeMeterEventResponse =
            serde_json::from_str(&body).map_err(|e| Error::ExternalService {
                service: "stripe".to_string(),
                message: format!(
                    "failed to parse meter event response: {} - body: {}",
                    e, body
                ),
            })?;

        Ok(StripeMeterEvent {
            id: raw.id,
            event_name: raw.event_name,
            identifier: raw.identifier,
            timestamp: raw.timestamp,
        })
    }

    /// Verify webhook signature using HMAC-SHA256 per Stripe's specification.
    ///
    /// The `signature` header has the format `t=<timestamp>,v1=<sig>[,v1=<sig>...]`.
    /// We compute `HMAC-SHA256(webhook_secret, "{t}.{payload}")` and compare
    /// each `v1` value using constant-time comparison.
    pub fn verify_webhook_signature(
        &self,
        payload: &[u8],
        signature: &str,
        webhook_secret: &str,
    ) -> Result<bool> {
        debug!("Verifying webhook signature");

        if webhook_secret.is_empty() {
            return Err(Error::Validation("missing webhook secret".to_string()));
        }

        // Parse the signature header: t=<timestamp>,v1=<sig1>[,v1=<sig2>...]
        let mut timestamp: Option<&str> = None;
        let mut signatures: Vec<&str> = Vec::new();

        for part in signature.split(',') {
            let part = part.trim();
            if let Some(val) = part.strip_prefix("t=") {
                timestamp = Some(val);
            } else if let Some(val) = part.strip_prefix("v1=") {
                signatures.push(val);
            }
        }

        let ts = match timestamp {
            Some(t) => t,
            None => return Ok(false),
        };

        if signatures.is_empty() {
            return Ok(false);
        }

        // Compute signed payload: "{timestamp}.{payload}"
        let payload_str = std::str::from_utf8(payload).unwrap_or("");
        let signed_payload = format!("{}.{}", ts, payload_str);

        // Compute HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(webhook_secret.as_bytes())
            .map_err(|_| Error::Validation("invalid webhook secret".to_string()))?;
        mac.update(signed_payload.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        // Constant-time compare against each v1 signature
        let expected_bytes = expected.as_bytes();
        for sig in signatures {
            if expected_bytes.ct_eq(sig.as_bytes()).into() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Parse webhook event
    pub fn parse_webhook_event(&self, payload: &[u8]) -> Result<WebhookEvent> {
        serde_json::from_slice(payload)
            .map_err(|e| Error::Validation(format!("Invalid webhook payload: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Private intermediate response types for Stripe API JSON
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StripeCustomerResponse {
    id: String,
    email: String,
    name: Option<String>,
    metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct CheckoutSessionResponse {
    id: String,
    url: String,
    customer: String,
    status: String,
}

#[derive(Deserialize)]
struct PortalSessionResponse {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct StripeSubscriptionResponse {
    id: String,
    customer: String,
    status: String,
    current_period_start: i64,
    current_period_end: i64,
    cancel_at_period_end: bool,
    items: StripeSubscriptionItemsResponse,
}

#[derive(Deserialize)]
struct StripeSubscriptionItemsResponse {
    data: Vec<StripeSubscriptionItemResponse>,
}

#[derive(Deserialize)]
struct StripeSubscriptionItemResponse {
    id: String,
    price: StripePriceResponse,
    quantity: u32,
}

#[derive(Deserialize)]
struct StripePriceResponse {
    id: String,
}

#[derive(Deserialize)]
struct StripeMeterEventResponse {
    id: String,
    event_name: String,
    identifier: String,
    timestamp: i64,
}

/// Parameters for creating a customer
#[derive(Debug, Clone, Serialize)]
pub struct CreateCustomerParams {
    pub email: String,
    pub name: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// A single line item in a Stripe checkout session.
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutLineItem {
    pub price_id: String,
    pub quantity: u64,
}

/// Parameters for creating a checkout session
#[derive(Debug, Clone, Serialize)]
pub struct CreateCheckoutSessionParams {
    pub customer_id: String,
    pub price_id: String,
    pub success_url: String,
    pub cancel_url: String,
    pub mode: String,
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub additional_line_items: Vec<CheckoutLineItem>,
}

/// Parameters for creating a Stripe Billing Meter Event.
#[derive(Debug, Clone, Serialize)]
pub struct CreateMeterEventParams {
    pub event_name: String,
    pub stripe_customer_id: String,
    pub value: u64,
    pub identifier: String,
    pub timestamp: Option<i64>,
    pub payload: std::collections::HashMap<String, String>,
}

/// Stripe customer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeCustomer {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Checkout session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    pub url: String,
    pub customer_id: String,
    pub status: String,
}

/// Customer portal session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalSession {
    pub id: String,
    pub url: String,
}

/// Stripe subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscription {
    pub id: String,
    pub customer: String,
    pub status: String,
    pub current_period_start: i64,
    pub current_period_end: i64,
    pub cancel_at_period_end: bool,
    pub items: Vec<SubscriptionItem>,
}

/// Subscription item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionItem {
    pub id: String,
    pub price_id: String,
    pub quantity: u32,
}

/// Stripe Billing Meter Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeMeterEvent {
    pub id: String,
    pub event_name: String,
    pub identifier: String,
    pub timestamp: i64,
}

/// Webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: WebhookEventData,
}

/// Webhook event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEventData {
    pub object: serde_json::Value,
}

/// Webhook event types we handle
pub enum WebhookEventType {
    CustomerSubscriptionCreated,
    CustomerSubscriptionUpdated,
    CustomerSubscriptionDeleted,
    InvoicePaid,
    InvoicePaymentFailed,
    CheckoutSessionCompleted,
    Unknown(String),
}

impl From<&str> for WebhookEventType {
    fn from(s: &str) -> Self {
        match s {
            "customer.subscription.created" => WebhookEventType::CustomerSubscriptionCreated,
            "customer.subscription.updated" => WebhookEventType::CustomerSubscriptionUpdated,
            "customer.subscription.deleted" => WebhookEventType::CustomerSubscriptionDeleted,
            "invoice.paid" => WebhookEventType::InvoicePaid,
            "invoice.payment_failed" => WebhookEventType::InvoicePaymentFailed,
            "checkout.session.completed" => WebhookEventType::CheckoutSessionCompleted,
            other => WebhookEventType::Unknown(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_event_type_parsing() {
        assert!(matches!(
            WebhookEventType::from("customer.subscription.created"),
            WebhookEventType::CustomerSubscriptionCreated
        ));

        assert!(matches!(
            WebhookEventType::from("invoice.paid"),
            WebhookEventType::InvoicePaid
        ));

        assert!(matches!(
            WebhookEventType::from("unknown.event"),
            WebhookEventType::Unknown(_)
        ));
    }

    /// Helper: compute a valid Stripe webhook signature header for a given payload and secret.
    fn compute_signature_header(payload: &[u8], secret: &str, timestamp: i64) -> String {
        let payload_str = std::str::from_utf8(payload).unwrap_or("");
        let signed_payload = format!("{}.{}", timestamp, payload_str);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("valid secret length");
        mac.update(signed_payload.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("t={},v1={}", timestamp, sig)
    }

    #[test]
    fn verify_webhook_signature_accepts_valid_signature() {
        let client = StripeClient::new_test();
        let secret = "whsec_test_secret_123";
        let payload = br#"{"id":"evt_123","type":"invoice.paid","data":{"object":{}}}"#;
        let header = compute_signature_header(payload, secret, 1700000000);

        let result = client.verify_webhook_signature(payload, &header, secret);
        assert!(result.unwrap());
    }

    #[test]
    fn verify_webhook_signature_rejects_tampered_payload() {
        let client = StripeClient::new_test();
        let secret = "whsec_test_secret_123";
        let original_payload = br#"{"id":"evt_123","type":"invoice.paid","data":{"object":{}}}"#;
        let header = compute_signature_header(original_payload, secret, 1700000000);

        // Tamper with the payload after signing
        let tampered_payload =
            br#"{"id":"evt_TAMPERED","type":"invoice.paid","data":{"object":{}}}"#;
        let result = client.verify_webhook_signature(tampered_payload, &header, secret);
        assert!(!result.unwrap());
    }

    #[test]
    fn verify_webhook_signature_rejects_empty_secret() {
        let client = StripeClient::new_test();
        let payload = br#"{"id":"evt_123"}"#;
        let result = client.verify_webhook_signature(payload, "t=123,v1=abc", "");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing webhook secret"));
    }

    /// Pure helper that builds the form body for a checkout session without
    /// making any HTTP call. Returns `(key, value)` pairs for assertions.
    fn build_checkout_form_body(params: &CreateCheckoutSessionParams) -> Vec<(String, String)> {
        let mut form: Vec<(String, String)> = vec![
            ("customer".to_string(), params.customer_id.clone()),
            ("line_items[0][price]".to_string(), params.price_id.clone()),
            ("line_items[0][quantity]".to_string(), "1".to_string()),
            ("mode".to_string(), params.mode.clone()),
            ("success_url".to_string(), params.success_url.clone()),
            ("cancel_url".to_string(), params.cancel_url.clone()),
        ];

        for (i, item) in params.additional_line_items.iter().enumerate() {
            let idx = i + 1;
            form.push((format!("line_items[{}][price]", idx), item.price_id.clone()));
            form.push((format!("line_items[{}][quantity]", idx), item.quantity.to_string()));
        }

        for (k, v) in &params.metadata {
            form.push((format!("metadata[{}]", k), v.clone()));
        }

        form
    }

    #[test]
    fn test_additional_line_items_produce_correct_form_body() {
        use std::collections::HashMap;

        let params = CreateCheckoutSessionParams {
            customer_id: "cus_test123".to_string(),
            price_id: "price_starter_monthly".to_string(),
            mode: "subscription".to_string(),
            success_url: "https://example.com/success".to_string(),
            cancel_url: "https://example.com/cancel".to_string(),
            metadata: HashMap::new(),
            additional_line_items: vec![
                CheckoutLineItem {
                    price_id: "price_reporting_monthly".to_string(),
                    quantity: 1,
                },
                CheckoutLineItem {
                    price_id: "price_ai_assistant_monthly".to_string(),
                    quantity: 1,
                },
            ],
        };

        let form = build_checkout_form_body(&params);

        // Base plan at [0]
        assert!(form.contains(&("line_items[0][price]".to_string(), "price_starter_monthly".to_string())));
        assert!(form.contains(&("line_items[0][quantity]".to_string(), "1".to_string())));

        // Add-on 1 at [1]
        assert!(form.contains(&("line_items[1][price]".to_string(), "price_reporting_monthly".to_string())));
        assert!(form.contains(&("line_items[1][quantity]".to_string(), "1".to_string())));

        // Add-on 2 at [2]
        assert!(form.contains(&("line_items[2][price]".to_string(), "price_ai_assistant_monthly".to_string())));
        assert!(form.contains(&("line_items[2][quantity]".to_string(), "1".to_string())));

        // Total: 6 base fields + 4 add-on fields = 10
        assert_eq!(form.len(), 10);
    }

    #[test]
    fn test_no_additional_line_items_produces_only_base() {
        use std::collections::HashMap;

        let params = CreateCheckoutSessionParams {
            customer_id: "cus_test".to_string(),
            price_id: "price_starter_monthly".to_string(),
            mode: "subscription".to_string(),
            success_url: "https://example.com/success".to_string(),
            cancel_url: "https://example.com/cancel".to_string(),
            metadata: HashMap::new(),
            additional_line_items: vec![],
        };

        let form = build_checkout_form_body(&params);

        // Only the 6 base fields, no add-on entries
        assert_eq!(form.len(), 6);
        assert!(form.iter().all(|(k, _)| !k.starts_with("line_items[1]")));
    }
}
