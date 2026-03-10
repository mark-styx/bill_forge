//! HTTP handlers for Winston AI API

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::agent::WinstonAgent;
use super::models::{ChatRequest, ChatResponse, Conversation};

/// Application state for AI agent
#[derive(Clone)]
pub struct AgentState {
    pub pool: PgPool,
    pub agent: WinstonAgent,
}

/// Create router for AI agent endpoints
pub fn create_router(pool: PgPool) -> Router {
    let agent = WinstonAgent::new(pool.clone());
    let state = AgentState { pool, agent };

    Router::new()
        .route("/api/ai/chat", post(chat_handler))
        .route("/api/ai/conversations", get(list_conversations_handler))
        .route(
            "/api/ai/conversations/:id/messages",
            post(continue_conversation_handler),
        )
        .with_state(state)
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// POST /api/ai/chat - Send message to Winston AI
pub async fn chat_handler(
    State(state): State<AgentState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    // In production, extract tenant_id and user_id from auth token
    // For now, use placeholder values
    let tenant_id = "acme-mfg".to_string();
    let user_id = Uuid::nil();

    match state.agent.chat(request, tenant_id, user_id).await {
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

/// GET /api/ai/conversations - List conversation history
pub async fn list_conversations_handler(
    State(state): State<AgentState>,
) -> Result<Json<Vec<Conversation>>, (StatusCode, Json<ErrorResponse>)> {
    // In production, extract tenant_id and user_id from auth token
    let tenant_id = "acme-mfg";
    let user_id = Uuid::nil();

    match state.agent.list_conversations(tenant_id, user_id).await {
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

/// POST /api/ai/conversations/:id/messages - Continue conversation
pub async fn continue_conversation_handler(
    State(state): State<AgentState>,
    Path(conversation_id): Path<Uuid>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    // In production, extract tenant_id and user_id from auth token
    let tenant_id = "acme-mfg".to_string();
    let user_id = Uuid::nil();

    let request_with_conversation = ChatRequest {
        message: request.message,
        conversation_id: Some(conversation_id),
    };

    match state.agent.chat(request_with_conversation, tenant_id, user_id).await {
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
