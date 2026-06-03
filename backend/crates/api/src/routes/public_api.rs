//! Public API routes at `/api/external/v1`
//!
//! PAT-authenticated, tenant-scoped, rate-limited external endpoints.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    async_trait,
    extract::{FromRequestParts, Path, Query, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use billforge_core::public_api::{
    self, verify_pat, RateLimiter, ALLOWED_EVENT_TYPES,
};
use billforge_core::{
    domain::{InvoiceFilters, InvoiceId},
    traits::InvoiceRepository,
    types::{Pagination, TenantId},
    Error,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared rate limiter state (stored in AppState extension)
// ---------------------------------------------------------------------------

/// Global in-memory rate limiter for public API keys.
#[derive(Clone)]
pub struct PublicApiRateLimiter(pub RateLimiter);

// ---------------------------------------------------------------------------
// Extractor: PublicApiAuth
// ---------------------------------------------------------------------------

/// Axum extractor that validates a PAT Bearer token, enforces rate limits,
/// and attaches the verified token to request extensions.
pub struct PublicApiAuth(pub billforge_core::public_api::PublicApiToken);

#[derive(Debug, Serialize)]
struct PublicApiErrorBody {
    error: PublicApiErrorDetail,
}

#[derive(Debug, Serialize)]
struct PublicApiErrorDetail {
    code: &'static str,
    message: String,
}

fn public_error(status: StatusCode, code: &'static str, message: &str) -> Response {
    let body = PublicApiErrorBody {
        error: PublicApiErrorDetail {
            code,
            message: message.to_string(),
        },
    };
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        Json(body),
    )
        .into_response()
}

#[async_trait]
impl<S> FromRequestParts<S> for PublicApiAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = parts
            .extensions
            .get::<AppState>()
            .cloned()
            .ok_or_else(|| {
                public_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "AppState not available",
                )
            })?;

        // Extract Bearer token from Authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                public_error(
                    StatusCode::UNAUTHORIZED,
                    "unauthenticated",
                    "Missing Authorization header",
                )
            })?;

        let bearer = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            public_error(
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "Invalid Authorization header format",
            )
        })?;

        // Verify PAT against the metadata database
        let token = verify_pat(&app_state.db.metadata(), bearer)
            .await
            .map_err(|msg| {
                public_error(StatusCode::UNAUTHORIZED, "invalid_token", &msg)
            })?;

        // Enforce per-key rate limit
        let limiter = parts
            .extensions
            .get::<PublicApiRateLimiter>()
            .cloned()
            .ok_or_else(|| {
                public_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Rate limiter not configured",
                )
            })?;

        if let Err(retry_after) = limiter.0.check(token.api_key_id, token.rate_limit_per_minute).await {
            return Err(
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [
                        (header::RETRY_AFTER, retry_after.to_string()),
                        (header::CONTENT_TYPE, "application/json".to_string()),
                    ],
                    Json(PublicApiErrorBody {
                        error: PublicApiErrorDetail {
                            code: "rate_limited",
                            message: format!(
                                "Rate limit exceeded. Retry after {} seconds.",
                                retry_after
                            ),
                        },
                    }),
                )
                    .into_response(),
            );
        }

        Ok(Self(token))
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListInvoicesQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// `GET /api/external/v1/invoices` — list invoices for the token's tenant.
async fn list_invoices(
    State(state): State<AppState>,
    PublicApiAuth(token): PublicApiAuth,
    Query(query): Query<ListInvoicesQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    public_api::require_scope(&token, "invoices:read").map_err(|msg| {
        ApiError(Error::Forbidden(msg))
    })?;

    let tenant_id = TenantId::from_uuid(token.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;

    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(25),
    };

    let filters = InvoiceFilters::default();
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let invoices = repo.list(&tenant_id, &filters, &pagination).await?;

    Ok(Json(serde_json::to_value(invoices).unwrap_or_default()))
}

/// `GET /api/external/v1/invoices/:id` — get a single invoice (tenant-scoped).
async fn get_invoice(
    State(state): State<AppState>,
    PublicApiAuth(token): PublicApiAuth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    public_api::require_scope(&token, "invoices:read").map_err(|msg| {
        ApiError(Error::Forbidden(msg))
    })?;

    let tenant_id = TenantId::from_uuid(token.tenant_id);
    let pool = state.db.tenant(&tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);

    let invoice = repo
        .get_by_id(&tenant_id, &InvoiceId(id))
        .await?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: id.to_string(),
        })?;

    Ok(Json(serde_json::to_value(&invoice).unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Webhook Subscription CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateWebhookSubscriptionRequest {
    pub target_url: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookSubscriptionResponse {
    pub id: String,
    pub target_url: String,
    pub event_types: Vec<String>,
    pub is_active: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookSubscriptionListResponse {
    pub subscriptions: Vec<WebhookSubscriptionResponse>,
}

/// `POST /api/external/v1/webhook-subscriptions`
async fn create_webhook_subscription(
    State(state): State<AppState>,
    PublicApiAuth(token): PublicApiAuth,
    Json(body): Json<CreateWebhookSubscriptionRequest>,
) -> Result<Json<WebhookSubscriptionResponse>, Response> {
    public_api::require_scope(&token, "webhooks:write").map_err(|msg| {
        public_error(StatusCode::FORBIDDEN, "insufficient_scope", &msg)
    })?;

    // Validate target_url is https
    if !body.target_url.starts_with("https://") {
        return Err(public_error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "target_url must use https",
        ));
    }

    // Validate event_types are within allowed set
    for et in &body.event_types {
        if !ALLOWED_EVENT_TYPES.contains(&et.as_str()) {
            return Err(public_error(
                StatusCode::BAD_REQUEST,
                "validation_error",
                &format!("Invalid event type: {}", et),
            ));
        }
    }

    if body.event_types.is_empty() {
        return Err(public_error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "At least one event_type is required",
        ));
    }

    let signing_secret = public_api::generate_signing_secret();
    let sub_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO webhook_subscriptions (id, tenant_id, api_key_id, target_url, event_types, signing_secret, is_active, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, true, NOW())"#,
    )
    .bind(sub_id)
    .bind(token.tenant_id)
    .bind(token.api_key_id)
    .bind(&body.target_url)
    .bind(&body.event_types)
    .bind(&signing_secret)
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Failed to create webhook subscription");
        public_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to create subscription",
        )
    })?;

    Ok(Json(WebhookSubscriptionResponse {
        id: sub_id.to_string(),
        target_url: body.target_url,
        event_types: body.event_types,
        is_active: true,
        created_at: chrono::Utc::now().to_rfc3339(),
        signing_secret: Some(signing_secret),
    }))
}

/// `GET /api/external/v1/webhook-subscriptions`
async fn list_webhook_subscriptions(
    State(state): State<AppState>,
    PublicApiAuth(token): PublicApiAuth,
) -> Result<Json<WebhookSubscriptionListResponse>, Response> {
    public_api::require_scope(&token, "webhooks:read").map_err(|msg| {
        public_error(StatusCode::FORBIDDEN, "insufficient_scope", &msg)
    })?;

    let rows = sqlx::query_as::<_, (Uuid, String, Vec<String>, bool, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, target_url, event_types, is_active, created_at
           FROM webhook_subscriptions
           WHERE tenant_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(token.tenant_id)
    .fetch_all(&*state.db.metadata())
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Failed to list webhook subscriptions");
        public_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to list subscriptions",
        )
    })?;

    let subscriptions = rows
        .into_iter()
        .map(|(id, url, events, active, created)| WebhookSubscriptionResponse {
            id: id.to_string(),
            target_url: url,
            event_types: events,
            is_active: active,
            created_at: created.to_rfc3339(),
            signing_secret: None, // Never return secret on list
        })
        .collect();

    Ok(Json(WebhookSubscriptionListResponse { subscriptions }))
}

/// `DELETE /api/external/v1/webhook-subscriptions/:id`
async fn delete_webhook_subscription(
    State(state): State<AppState>,
    PublicApiAuth(token): PublicApiAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, Response> {
    public_api::require_scope(&token, "webhooks:write").map_err(|msg| {
        public_error(StatusCode::FORBIDDEN, "insufficient_scope", &msg)
    })?;

    let result = sqlx::query(
        r#"DELETE FROM webhook_subscriptions WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(id)
    .bind(token.tenant_id)
    .execute(&*state.db.metadata())
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Failed to delete webhook subscription");
        public_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to delete subscription",
        )
    })?;

    if result.rows_affected() == 0 {
        return Err(public_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Webhook subscription not found",
        ));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/invoices", get(list_invoices))
        .route("/invoices/{id}", get(get_invoice))
        .route("/webhook-subscriptions", post(create_webhook_subscription))
        .route("/webhook-subscriptions", get(list_webhook_subscriptions))
        .route(
            "/webhook-subscriptions/{id}",
            delete(delete_webhook_subscription),
        )
}
