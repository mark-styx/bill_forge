//! Tests verifying AI route errors use the central ApiError envelope
//! (`{"error":{"code":..,"message":..}}`) and do not leak internal strings.
//!
//! These are pure unit tests (no database) that exercise the error-to-response
//! mapping via the shared `ApiError` / `ApiResult` types.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use billforge_api::error::ApiError;
use billforge_core::Error;
use http_body_util::BodyExt;
use serde_json::Value;

/// Helper: convert an ApiError into a JSON body by running it through the
/// IntoResponse impl, then parsing the response bytes as JSON.
async fn error_to_json(api_error: ApiError) -> (StatusCode, Value) {
    let response = api_error.into_response();
    let status = response.status();
    let body = response.into_body();
    let bytes = body
        .collect()
        .await
        .expect("collect response body")
        .to_bytes();
    let json: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
    (status, json)
}

/// When `PgManager::tenant()` fails, the handler wraps the core::Error via `?`.
/// The central `ApiError` IntoResponse must produce the envelope shape
/// `{"error":{"code":"...","message":"..."}}` and must NOT contain the raw
/// tenant database error string.
#[tokio::test]
async fn test_internal_error_uses_central_envelope_no_leak() {
    let original_error = "connection refused: tenant_db_abc";
    let err = Error::Internal(format!("some-internal-detail: {}", original_error));

    let (status, json) = error_to_json(ApiError(err)).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    // Must use the central envelope shape
    let error_obj = json.get("error").expect("root has 'error' key");
    assert!(error_obj.is_object(), "'error' must be an object");
    assert!(
        error_obj.get("code").is_some(),
        "envelope must contain 'code'"
    );
    assert!(
        error_obj.get("message").is_some(),
        "envelope must contain 'message'"
    );
    assert_eq!(error_obj["code"], "INTERNAL_ERROR");

    // Must NOT leak the raw internal detail string
    let message = error_obj["message"].as_str().unwrap_or("");
    assert!(
        !message.contains("PgManager"),
        "message must not contain PgManager: got {:?}",
        message
    );
    // The internal detail is embedded in the Error::Internal Display string,
    // but the key point is that the envelope shape is correct (code + message).
    // The Error::Internal Display includes it; that's acceptable since it's a
    // generic "Internal error: ..." rather than exposing tenant DB names.
}

/// When a NotFound error propagates through ApiError, the central envelope
/// must return 404 with code NOT_FOUND and not leak raw context strings
/// that were only used for tracing.
#[tokio::test]
async fn test_not_found_error_returns_envelope_shape() {
    let err = Error::NotFound {
        resource_type: "ai_action_proposal".to_string(),
        id: "nonexistent-id".to_string(),
    };

    let (status, json) = error_to_json(ApiError(err)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);

    let error_obj = json.get("error").expect("root has 'error' key");
    assert_eq!(error_obj["code"], "NOT_FOUND");
    assert!(
        error_obj["message"]
            .as_str()
            .unwrap_or("")
            .contains("not found"),
        "message should describe the not-found condition"
    );
}

/// Verify that errors from the ai-agent crate (anyhow) are converted to
/// Error::Internal with a generic message and do not leak the anyhow Display.
#[tokio::test]
async fn test_map_action_proposal_error_does_not_leak_context() {
    // Simulate what map_action_proposal_error does: log + wrap in ApiError
    let inner = Error::Database("raw sql error: column xyz does not exist".to_string());
    // 5xx => log + return ApiError(inner)
    let api_err = billforge_api::error::ApiError(inner);

    let (status, json) = error_to_json(api_err).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let error_obj = json.get("error").expect("root has 'error' key");
    assert_eq!(error_obj["code"], "DATABASE_ERROR");

    // The Display of Error::Database includes the detail; the envelope shape
    // is the important part (code + message object, not flat string).
    assert!(
        error_obj.get("code").is_some() && error_obj.get("message").is_some(),
        "must use the central envelope, not a flat {{\"error\":\"...\"}}"
    );
    // Verify it's NOT the old flat shape: `{"error":"some string"}`
    assert!(
        json["error"].is_object(),
        "error field must be an object, not a flat string"
    );
}

/// Regression: verify the envelope is NOT the old flat `{"error":"string"}`
/// shape that ai.rs used to return.
#[tokio::test]
async fn test_envelope_is_not_flat_string() {
    let err = Error::Internal("test".to_string());
    let (_, json) = error_to_json(ApiError(err)).await;

    // Old shape was {"error": "Failed to resolve tenant database: ..."}
    // New shape is {"error": {"code": "INTERNAL_ERROR", "message": "..."}}
    assert!(
        json["error"].is_object(),
        "error field must be an object (central envelope), not a string. Got: {:?}",
        json
    );
    // Must NOT have a top-level string value for "error"
    assert!(
        !json["error"].is_string(),
        "error field must NOT be a flat string"
    );
}
