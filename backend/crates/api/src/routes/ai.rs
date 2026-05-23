//! AI Assistant (Winston) API routes
//!
//! Thin adapter that constructs a WinstonAgent from the authenticated tenant's
//! database pool and an injected AiProvider, then delegates to the ai-agent
//! crate's handler logic.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use billforge_ai_agent::agent::WinstonAgent;
use billforge_ai_agent::models::{
    BugReportDraftRequest, BugReportDraftResponse, ChatRequest, ChatResponse, Conversation,
};
use billforge_ai_agent::provider::AiProvider;
use billforge_ai_agent::OpenAiCompatibleProvider;

use billforge_db::repositories::{
    AiAnswerFeedbackRating, AiConversationRepositoryImpl, PersistAiAnswerFeedbackInput,
};

use crate::extractors::AiAssistantAccess;
use crate::state::AppState;

/// Error response shape matching the original AI route contract: `{"error":"..."}`.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Request body for submitting feedback on an assistant answer.
#[derive(Debug, Deserialize)]
struct SubmitFeedbackRequest {
    rating: AiAnswerFeedbackRating,
    comment: Option<String>,
}

/// Persisted feedback record returned to the client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct FeedbackResponse {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    rating: String,
    comment: Option<String>,
    metadata: serde_json::Value,
    created_at: String,
    updated_at: String,
}

/// Create AI assistant sub-router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/chat", post(chat_handler))
        .route("/bug-report-drafts", post(bug_report_draft_handler))
        .route("/conversations", get(list_conversations_handler))
        .route("/conversations/{id}/messages", post(continue_conversation_handler))
        .route(
            "/conversations/{conversation_id}/messages/{message_id}/feedback",
            post(submit_feedback_handler),
        )
}

/// Build the configured AiProvider for Winston.
fn build_provider() -> Arc<dyn AiProvider> {
    Arc::new(OpenAiCompatibleProvider::from_env())
}

/// POST /ai/chat
#[utoipa::path(post, path = "/api/v1/ai/chat", tag = "AI Assistant", request_body = serde_json::Value,
    responses((status = 200, description = "Chat response"), (status = 401, description = "Unauthorized")))]
async fn chat_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Resolve the tenant-scoped pool so conversation/message persistence
    // targets the tenant database (which has ai_conversations/ai_messages)
    // rather than the metadata database.
    let pool = match state.db.tenant(&user.tenant_id).await {
        Ok(pool) => (*pool).clone(),
        Err(e) => {
            tracing::error!("Tenant pool error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resolve tenant database: {}", e),
                }),
            ));
        }
    };

    let provider = build_provider();
    let agent = WinstonAgent::new(pool, provider)
        .with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    match agent.chat(request, tenant_id, user_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Chat error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to process message: {}", e),
                }),
            ))
        }
    }
}

/// GET /ai/conversations
#[utoipa::path(get, path = "/api/v1/ai/conversations", tag = "AI Assistant",
    responses((status = 200, description = "Conversation list"), (status = 401, description = "Unauthorized")))]
async fn list_conversations_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
) -> Result<Json<Vec<Conversation>>, (StatusCode, Json<ErrorResponse>)> {
    let pool = (*state.db.metadata()).clone();
    let provider = build_provider();
    let agent = WinstonAgent::new(pool, provider);

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    match agent.list_conversations(&tenant_id, user_id).await {
        Ok(conversations) => Ok(Json(conversations)),
        Err(e) => {
            tracing::error!("List conversations error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to list conversations: {}", e),
                }),
            ))
        }
    }
}

/// POST /ai/bug-report-drafts
async fn bug_report_draft_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Json(request): Json<BugReportDraftRequest>,
) -> Result<Json<BugReportDraftResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = match state.db.tenant(&user.tenant_id).await {
        Ok(pool) => (*pool).clone(),
        Err(e) => {
            tracing::error!("Tenant pool error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resolve tenant database: {}", e),
                }),
            ));
        }
    };

    let provider = build_provider();
    let agent = WinstonAgent::new(pool, provider)
        .with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    match agent.generate_bug_report_draft(request, tenant_id, user_id).await {
        Ok(draft) => Ok(Json(draft)),
        Err(e) => {
            tracing::error!("Bug report draft error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to generate bug report draft: {}", e),
                }),
            ))
        }
    }
}

/// POST /ai/conversations/:id/messages
#[utoipa::path(post, path = "/api/v1/ai/conversations/{id}/messages", tag = "AI Assistant", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Conversation ID")),
    responses((status = 200, description = "Chat response"), (status = 401, description = "Unauthorized")))]
async fn continue_conversation_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Path(conversation_id): Path<Uuid>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = match state.db.tenant(&user.tenant_id).await {
        Ok(pool) => (*pool).clone(),
        Err(e) => {
            tracing::error!("Tenant pool error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resolve tenant database: {}", e),
                }),
            ));
        }
    };

    let provider = build_provider();
    let agent = WinstonAgent::new(pool, provider)
        .with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let request_with_conversation = ChatRequest {
        message: request.message,
        conversation_id: Some(conversation_id),
    };

    match agent.chat(request_with_conversation, tenant_id, user_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Continue conversation error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to continue conversation: {}", e),
                }),
            ))
        }
    }
}

/// POST /ai/conversations/{conversation_id}/messages/{message_id}/feedback
async fn submit_feedback_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Path((conversation_id, message_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SubmitFeedbackRequest>,
) -> Result<Json<FeedbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = match state.db.tenant(&user.tenant_id).await {
        Ok(pool) => (*pool).clone(),
        Err(e) => {
            tracing::error!("Tenant pool error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resolve tenant database: {}", e),
                }),
            ));
        }
    };

    let repo = AiConversationRepositoryImpl::new(std::sync::Arc::new(pool));

    let input = PersistAiAnswerFeedbackInput {
        rating: body.rating,
        comment: body.comment,
        metadata: serde_json::json!({}),
    };

    match repo
        .persist_answer_feedback(&user.tenant_id, &user.user_id, conversation_id, message_id, input)
        .await
    {
        Ok(record) => Ok(Json(FeedbackResponse {
            id: record.id,
            tenant_id: record.tenant_id,
            user_id: record.user_id,
            conversation_id: record.conversation_id,
            message_id: record.message_id,
            rating: record.rating,
            comment: record.comment,
            metadata: record.metadata,
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
        })),
        Err(e) => {
            tracing::error!("Feedback error: {}", e);
            let status = match &e {
                billforge_core::Error::NotFound { .. } => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                status,
                Json(ErrorResponse {
                    error: format!("Failed to submit feedback: {}", e),
                }),
            ))
        }
    }
}
