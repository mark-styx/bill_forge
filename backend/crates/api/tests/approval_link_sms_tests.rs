//! Integration tests for the SMS / WhatsApp approval-link delivery surface.
//!
//! Covers: signed-URL generation + provider invocation (SMS + WhatsApp), the
//! biometric-attestation requirement on consume for SMS/WhatsApp channels, and
//! tenant isolation on recipient resolution. Mirrors the split used by the
//! sibling `approval_link_tests.rs`: pure-logic tests run unconditionally,
//! database-backed assertions are gated behind `#[ignore]` + `DATABASE_URL`.

#![allow(warnings)]

use billforge_api::notifications::sms::{NoopSmsProvider, SmsChannel};
use billforge_api::routes::approval_links::{
    approval_link_sent_metadata, dispatch_sms_approval_link, require_biometric_for_channel,
    resolve_recipient_phone, verify_approval_token, DeliveryChannel,
};
use billforge_core::TenantId;
use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
const OTHER_TENANT_ID: &str = "22222222-2222-2222-2222-222222222222";
const FIXTURE_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

// ===========================================================================
// Happy path: SMS dispatch mints a signed URL and invokes the provider
// ===========================================================================

#[tokio::test]
async fn send_sms_link_generates_signed_url_and_invokes_provider() {
    let provider = NoopSmsProvider::new();
    let invoice_id = Uuid::new_v4();
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();

    let outcome = dispatch_sms_approval_link(
        "INV-001",
        12500,
        invoice_id,
        "exec@example.com".to_string(),
        tenant_id,
        "+15551234567",
        SmsChannel::Sms,
        &provider,
    )
    .await
    .expect("sms dispatch should succeed");

    // The provider should have recorded exactly one message embedding the URL.
    let recorded = provider.recorded();
    assert_eq!(recorded.len(), 1, "exactly one provider send expected");
    let msg = &recorded[0];
    assert_eq!(msg.to, "+15551234567");
    assert_eq!(msg.channel, SmsChannel::Sms);
    assert!(msg.body.contains(outcome.short_url.as_str()));
    assert!(outcome.short_url.contains("/a/"), "short url shape: {}", outcome.short_url);

    // The token embedded in the URL must verify and carry the SMS channel.
    let token = outcome.short_url.rsplit('/').next().expect("token segment");
    let claims = verify_approval_token(token).await.expect("verify embedded token");
    assert_eq!(claims.invoice_id, invoice_id);
    assert_eq!(claims.tenant_id, tenant_id);
    assert_eq!(claims.delivery_channel, DeliveryChannel::Sms);

    // Audit metadata must record the sms channel.
    let meta = approval_link_sent_metadata(DeliveryChannel::Sms, Uuid::nil(), "+15551234567", "x");
    assert_eq!(meta["channel"], "sms");
    assert_eq!(meta["event"], "approval_link.sent");
}

// ===========================================================================
// WhatsApp channel is plumbed through end-to-end
// ===========================================================================

#[tokio::test]
async fn send_whatsapp_link_uses_whatsapp_channel() {
    let provider = NoopSmsProvider::new();
    let outcome = dispatch_sms_approval_link(
        "INV-002",
        9999,
        Uuid::new_v4(),
        "cfo@example.com".to_string(),
        Uuid::parse_str(SANDBOX_TENANT_ID).unwrap(),
        "+447700900123",
        SmsChannel::WhatsApp,
        &provider,
    )
    .await
    .expect("whatsapp dispatch should succeed");

    let recorded = provider.recorded();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].channel, SmsChannel::WhatsApp);
    assert_eq!(recorded[0].to, "+447700900123");
    assert_eq!(outcome.channel, DeliveryChannel::WhatsApp);

    let token = outcome.short_url.rsplit('/').next().unwrap();
    let claims = verify_approval_token(token).await.expect("verify");
    assert_eq!(claims.delivery_channel, DeliveryChannel::WhatsApp);
}

// ===========================================================================
// Biometric attestation requirement on the consume side
// ===========================================================================

#[test]
fn approve_via_sms_token_requires_biometric_attested() {
    // SMS / WhatsApp without biometric -> rejected (handler returns 400).
    assert!(require_biometric_for_channel(DeliveryChannel::Sms, false).is_err());
    assert!(require_biometric_for_channel(DeliveryChannel::WhatsApp, false).is_err());

    // SMS / WhatsApp with biometric -> accepted (handler proceeds and writes audit).
    assert!(require_biometric_for_channel(DeliveryChannel::Sms, true).is_ok());
    assert!(require_biometric_for_channel(DeliveryChannel::WhatsApp, true).is_ok());

    // Email never requires biometric, regardless of the flag.
    assert!(require_biometric_for_channel(DeliveryChannel::Email, false).is_ok());
}

// ===========================================================================
// Tenant isolation: recipient resolution fails closed for cross-tenant users
// ===========================================================================

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test approval_link_sms_tests -- --ignored
async fn tenant_isolation_send_sms_other_tenant_user_404() {
    let pool = get_pool().await;
    let tenant_a = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let tenant_b = Uuid::parse_str(OTHER_TENANT_ID).unwrap();
    let user_in_a = Uuid::parse_str(FIXTURE_USER_ID).unwrap();

    billforge_db::tenant_db::run_tenant_migrations(&pool, &TenantId(tenant_a))
        .await
        .expect("tenant_a migrations");
    billforge_db::tenant_db::run_tenant_migrations(&pool, &TenantId(tenant_b))
        .await
        .expect("tenant_b migrations");

    // Fixture user lives in tenant A with a phone configured.
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name, settings) \
         VALUES ($1, $2, 'sms-isolation@example.com', '', 'SMS Test User', $3::jsonb) \
         ON CONFLICT (id) DO UPDATE SET settings = EXCLUDED.settings",
    )
    .bind(user_in_a)
    .bind(tenant_a)
    .bind(serde_json::json!({ "phone": "+15550000001" }).to_string())
    .execute(&pool)
    .await
    .expect("create fixture user");

    // Caller in tenant B asking to send to a user id that only exists in tenant A
    // must fail closed (None -> handler returns NotFound / 404).
    let cross = resolve_recipient_phone(&pool, &TenantId(tenant_b), user_in_a)
        .await
        .expect("query should not error");
    assert!(cross.is_none(), "cross-tenant lookup must fail closed");

    // Same-tenant lookup resolves correctly.
    let (phone, email) = resolve_recipient_phone(&pool, &TenantId(tenant_a), user_in_a)
        .await
        .expect("query should not error")
        .expect("in-tenant user should resolve");
    assert_eq!(phone, "+15550000001");
    assert_eq!(email, "sms-isolation@example.com");
}
