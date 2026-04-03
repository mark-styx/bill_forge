//! Feedback routes - stores user feedback to a JSONL file in the project directory

use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", post(submit_feedback).get(list_feedback))
}

#[derive(Debug, Deserialize)]
struct FeedbackInput {
    message: String,
    category: Option<String>,
    page: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FeedbackEntry {
    id: String,
    user_email: String,
    user_name: String,
    message: String,
    category: String,
    page: Option<String>,
    timestamp: String,
}

fn feedback_file_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Navigate from backend/crates/api to project root
    path.pop(); // crates
    path.pop(); // backend (we stay here since it's the working dir context)
    path.pop(); // up to project root
    path.push("feedback.jsonl");
    path
}

async fn submit_feedback(
    State(_state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<FeedbackInput>,
) -> Result<Json<FeedbackEntry>, axum::http::StatusCode> {
    let entry = FeedbackEntry {
        id: uuid::Uuid::new_v4().to_string(),
        user_email: user.email.clone(),
        user_name: user.name.clone(),
        message: input.message,
        category: input.category.unwrap_or_else(|| "general".to_string()),
        page: input.page,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let path = feedback_file_path();

    // Append as JSON line
    let line =
        serde_json::to_string(&entry).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            tracing::error!("Failed to open feedback file {:?}: {}", path, e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    writeln!(file, "{}", line).map_err(|e| {
        tracing::error!("Failed to write feedback: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(
        "Feedback saved from {}: {}",
        entry.user_email,
        entry.message
    );

    Ok(Json(entry))
}

async fn list_feedback(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<FeedbackEntry>>, axum::http::StatusCode> {
    let path = feedback_file_path();

    if !path.exists() {
        return Ok(Json(vec![]));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries: Vec<FeedbackEntry> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    Ok(Json(entries))
}
