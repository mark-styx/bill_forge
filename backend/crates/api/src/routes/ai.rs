//! AI Assistant (Winston) API routes
//!
//! Thin adapter that constructs a WinstonAgent from AppState's database pool
//! and delegates to the ai-agent crate's handler logic.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Serialize;
use uuid::Uuid;

use billforge_ai_agent::agent::WinstonAgent;
use billforge_ai_agent::models::{ChatRequest, ChatResponse, Conversation};

use crate::extractors::AuthUser;
use crate::state::AppState;

/// Error response shape matching ai-agent crate convention
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Create AI assistant sub-router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/chat", post(chat_handler))
        .route("/conversations", get(list_conversations_handler))
        .route("/conversations/{id}/messages", post(continue_conversation_handler))
}

/// POST /ai/chat
async fn chat_handler(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = (*state.db.metadata()).clone();
    let agent = WinstonAgent::new(pool);

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
async fn list_conversations_handler(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<Conversation>>, (StatusCode, Json<ErrorResponse>)> {
    let pool = (*state.db.metadata()).clone();
    let agent = WinstonAgent::new(pool);

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

/// POST /ai/conversations/:id/messages
async fn continue_conversation_handler(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(conversation_id): Path<Uuid>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = (*state.db.metadata()).clone();
    let agent = WinstonAgent::new(pool);

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
