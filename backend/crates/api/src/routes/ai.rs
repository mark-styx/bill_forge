//! AI Assistant (Winston) API routes
//!
//! Thin adapter that constructs a WinstonAgent from the authenticated tenant's
//! database pool and an injected AiProvider, then delegates to the ai-agent
//! crate's handler logic.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use billforge_ai_agent::agent::WinstonAgent;
use billforge_ai_agent::models::{
    AiActionProposalDecisionRequest, AiActionProposalResponse, BugReportDraftRequest,
    BugReportDraftResponse, ChatRequest, ChatResponse, Conversation, FeatureRequestDraftRequest,
    FeatureRequestDraftResponse,
};
use billforge_ai_agent::proposals::validate_action_proposal_decision;
use billforge_ai_agent::provider::AiProvider;
use billforge_ai_agent::OpenAiCompatibleProvider;
use billforge_core::{Error, TenantContext, UserContext};

use billforge_db::repositories::{
    AiActionProposalRecord, AiActionProposalRepositoryImpl, AiActionProposalStatus,
    AiAnswerFeedbackRating, AiConversationRepositoryImpl, PersistAiAnswerFeedbackInput,
    UpdateAiActionProposalStatusInput,
};

use crate::error::{ApiError, ApiResult};
use crate::extractors::AiAssistantAccess;
use crate::state::AppState;

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
        .route(
            "/feature-request-drafts",
            post(feature_request_draft_handler),
        )
        .route("/conversations", get(list_conversations_handler))
        .route(
            "/conversations/{id}/messages",
            post(continue_conversation_handler),
        )
        .route(
            "/conversations/{conversation_id}/messages/{message_id}/feedback",
            post(submit_feedback_handler),
        )
        .route(
            "/conversations/{conversation_id}/action-proposals/pending",
            get(list_pending_action_proposals_handler),
        )
        .route(
            "/action-proposals/{proposal_id}/approve",
            post(approve_action_proposal_handler),
        )
        .route(
            "/action-proposals/{proposal_id}/reject",
            post(reject_action_proposal_handler),
        )
}

/// Build the configured AiProvider for Winston.
fn build_provider() -> Arc<dyn AiProvider> {
    Arc::new(OpenAiCompatibleProvider::from_env())
}

fn action_proposal_response_from_record(
    record: AiActionProposalRecord,
) -> AiActionProposalResponse {
    AiActionProposalResponse {
        id: record.id,
        tenant_id: record.tenant_id,
        user_id: record.user_id,
        conversation_id: record.conversation_id,
        tool_name: record.tool_name,
        payload: record.payload,
        risk: record.risk.as_str().to_string(),
        permission: record.permission,
        status: record.status.as_str().to_string(),
        execution_error_code: record.execution_error_code,
        execution_error_message: record.execution_error_message,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn map_action_proposal_error(context: &str, error: Error) -> ApiError {
    if error.status_code() >= 500 {
        tracing::error!("{}: {}", context, error);
    }
    ApiError(error)
}

async fn update_action_proposal_status(
    state: &AppState,
    tenant: &TenantContext,
    user: &UserContext,
    proposal_id: Uuid,
    status: AiActionProposalStatus,
) -> ApiResult<Json<AiActionProposalResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let repo = AiActionProposalRepositoryImpl::new(std::sync::Arc::new(pool));
    let proposal = repo
        .get_proposal(&user.tenant_id, &user.user_id, proposal_id)
        .await
        .and_then(|proposal| {
            proposal.ok_or_else(|| Error::NotFound {
                resource_type: "ai_action_proposal".to_string(),
                id: proposal_id.to_string(),
            })
        })
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))?;

    validate_action_proposal_decision(tenant, user, &proposal)
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))?;

    let input = UpdateAiActionProposalStatusInput {
        status,
        execution_error_code: None,
        execution_error_message: None,
    };

    repo.update_pending_proposal_status(&user.tenant_id, &user.user_id, proposal_id, input)
        .await
        .map(action_proposal_response_from_record)
        .map(Json)
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))
}

async fn approve_action_proposal(
    state: &AppState,
    tenant: &TenantContext,
    user: &UserContext,
    proposal_id: Uuid,
) -> ApiResult<Json<AiActionProposalResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let repo = AiActionProposalRepositoryImpl::new(std::sync::Arc::new(pool));
    let proposal = repo
        .get_proposal(&user.tenant_id, &user.user_id, proposal_id)
        .await
        .and_then(|proposal| {
            proposal.ok_or_else(|| Error::NotFound {
                resource_type: "ai_action_proposal".to_string(),
                id: proposal_id.to_string(),
            })
        })
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))?;

    validate_action_proposal_decision(tenant, user, &proposal)
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))?;

    repo.approve_pending_proposal(&user.tenant_id, &user.user_id, proposal_id)
        .await
        .map(action_proposal_response_from_record)
        .map(Json)
        .map_err(|e| map_action_proposal_error("Failed to update action proposal", e))
}

/// POST /ai/chat
#[utoipa::path(post, path = "/api/v1/ai/chat", tag = "AI Assistant", request_body = serde_json::Value,
    responses((status = 200, description = "Chat response"), (status = 401, description = "Unauthorized")))]
async fn chat_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Json(request): Json<ChatRequest>,
) -> ApiResult<Json<ChatResponse>> {
    // Resolve the tenant-scoped pool so conversation/message persistence
    // targets the tenant database (which has ai_conversations/ai_messages)
    // rather than the metadata database.
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let provider = build_provider();
    let agent =
        WinstonAgent::new(pool, provider).with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let response = agent.chat(request, tenant_id, user_id).await.map_err(|e| {
        tracing::error!("Chat error: {}", e);
        Error::Internal("Failed to process message".into())
    })?;
    Ok(Json(response))
}

/// GET /ai/conversations
#[utoipa::path(get, path = "/api/v1/ai/conversations", tag = "AI Assistant",
    responses((status = 200, description = "Conversation list"), (status = 401, description = "Unauthorized")))]
async fn list_conversations_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
) -> ApiResult<Json<Vec<Conversation>>> {
    let pool = (*state.db.metadata()).clone();
    let provider = build_provider();
    let agent = WinstonAgent::new(pool, provider);

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let conversations = agent
        .list_conversations(&tenant_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!("List conversations error: {}", e);
            Error::Internal("Failed to list conversations".into())
        })?;
    Ok(Json(conversations))
}

/// POST /ai/bug-report-drafts
async fn bug_report_draft_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Json(request): Json<BugReportDraftRequest>,
) -> ApiResult<Json<BugReportDraftResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let provider = build_provider();
    let agent =
        WinstonAgent::new(pool, provider).with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let draft = agent
        .generate_bug_report_draft(request, tenant_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!("Bug report draft error: {}", e);
            Error::Internal("Failed to generate bug report draft".into())
        })?;
    Ok(Json(draft))
}

/// POST /ai/feature-request-drafts
async fn feature_request_draft_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Json(request): Json<FeatureRequestDraftRequest>,
) -> ApiResult<Json<FeatureRequestDraftResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let provider = build_provider();
    let agent =
        WinstonAgent::new(pool, provider).with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let draft = agent
        .generate_feature_request_draft(request, tenant_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!("Feature request draft error: {}", e);
            Error::Internal("Failed to generate feature request draft".into())
        })?;
    Ok(Json(draft))
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
) -> ApiResult<Json<ChatResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let provider = build_provider();
    let agent =
        WinstonAgent::new(pool, provider).with_enabled_modules(_tenant.enabled_modules.clone());

    let tenant_id = user.tenant_id.0.to_string();
    let user_id = user.user_id.0;

    let request_with_conversation = ChatRequest {
        message: request.message,
        conversation_id: Some(conversation_id),
    };

    let response = agent
        .chat(request_with_conversation, tenant_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!("Continue conversation error: {}", e);
            Error::Internal("Failed to continue conversation".into())
        })?;
    Ok(Json(response))
}

/// POST /ai/conversations/{conversation_id}/messages/{message_id}/feedback
async fn submit_feedback_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Path((conversation_id, message_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SubmitFeedbackRequest>,
) -> ApiResult<Json<FeedbackResponse>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let repo = AiConversationRepositoryImpl::new(std::sync::Arc::new(pool));

    let input = PersistAiAnswerFeedbackInput {
        rating: body.rating,
        comment: body.comment,
        metadata: serde_json::json!({}),
    };

    let record = repo
        .persist_answer_feedback(
            &user.tenant_id,
            &user.user_id,
            conversation_id,
            message_id,
            input,
        )
        .await
        .map_err(|e| {
            if e.status_code() >= 500 {
                tracing::error!("Feedback error: {}", e);
            }
            e
        })?;

    Ok(Json(FeedbackResponse {
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
    }))
}

/// GET /ai/conversations/{conversation_id}/action-proposals/pending
#[utoipa::path(get, path = "/api/v1/ai/conversations/{conversation_id}/action-proposals/pending", tag = "AI Assistant",
    params(("conversation_id" = Uuid, Path, description = "Conversation ID")),
    responses((status = 200, description = "Pending action proposals"), (status = 401, description = "Unauthorized")))]
async fn list_pending_action_proposals_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, _tenant): AiAssistantAccess,
    Path(conversation_id): Path<Uuid>,
) -> ApiResult<Json<Vec<AiActionProposalResponse>>> {
    let pool = state.db.tenant(&user.tenant_id).await?;
    let pool = (*pool).clone();

    let repo = AiActionProposalRepositoryImpl::new(std::sync::Arc::new(pool));

    let records = repo
        .list_pending_proposals_for_conversation(&user.tenant_id, &user.user_id, conversation_id)
        .await
        .map_err(|e| map_action_proposal_error("Failed to list pending action proposals", e))?;

    Ok(Json(
        records
            .into_iter()
            .map(action_proposal_response_from_record)
            .collect(),
    ))
}

/// POST /ai/action-proposals/{proposal_id}/approve
#[utoipa::path(post, path = "/api/v1/ai/action-proposals/{proposal_id}/approve", tag = "AI Assistant",
    request_body = serde_json::Value,
    params(("proposal_id" = Uuid, Path, description = "Action proposal ID")),
    responses((status = 200, description = "Approved action proposal"), (status = 400, description = "Invalid proposal decision"), (status = 401, description = "Unauthorized"), (status = 403, description = "Forbidden"), (status = 404, description = "Action proposal not found")))]
async fn approve_action_proposal_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, tenant): AiAssistantAccess,
    Path(proposal_id): Path<Uuid>,
    Json(_request): Json<AiActionProposalDecisionRequest>,
) -> ApiResult<Json<AiActionProposalResponse>> {
    approve_action_proposal(&state, &tenant, &user, proposal_id).await
}

/// POST /ai/action-proposals/{proposal_id}/reject
#[utoipa::path(post, path = "/api/v1/ai/action-proposals/{proposal_id}/reject", tag = "AI Assistant",
    request_body = serde_json::Value,
    params(("proposal_id" = Uuid, Path, description = "Action proposal ID")),
    responses((status = 200, description = "Rejected action proposal"), (status = 400, description = "Invalid proposal decision"), (status = 401, description = "Unauthorized"), (status = 403, description = "Forbidden"), (status = 404, description = "Action proposal not found")))]
async fn reject_action_proposal_handler(
    State(state): State<AppState>,
    AiAssistantAccess(user, tenant): AiAssistantAccess,
    Path(proposal_id): Path<Uuid>,
    Json(_request): Json<AiActionProposalDecisionRequest>,
) -> ApiResult<Json<AiActionProposalResponse>> {
    update_action_proposal_status(
        &state,
        &tenant,
        &user,
        proposal_id,
        AiActionProposalStatus::Rejected,
    )
    .await
}
