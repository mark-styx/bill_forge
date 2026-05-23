//! Approval-required issue creation boundary.
//!
//! Defines provider-neutral types for preparing issue creation requests that
//! require explicit approval before any external issue is created. No network
//! calls or database writes are performed.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Target provider for issue creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCreationTarget {
    Github,
    Linear,
    Jira,
    InternalFeedbackTable,
}

/// Kind of issue being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCreationKind {
    Bug,
    FeatureRequest,
    SupportRequest,
    Other,
}

/// An incoming issue creation request, before approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCreationRequest {
    pub target: IssueCreationTarget,
    pub kind: IssueCreationKind,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_id: Option<String>,
    /// Deterministic deep-link back to the source conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_link: Option<String>,
    /// Arbitrary structured metadata for downstream consumers.
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

fn default_metadata() -> serde_json::Value {
    serde_json::json!({})
}

/// The approval envelope returned by `prepare_issue_creation_for_approval`.
/// No external issue has been created; an approval step is required first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequiredIssueCreation {
    pub approval_request_id: Uuid,
    pub request: IssueCreationRequest,
    pub status: String,
    pub message: String,
}

/// Validation errors for issue creation requests.
#[derive(Debug, thiserror::Error)]
pub enum IssueCreationError {
    #[error("Title must not be empty")]
    EmptyTitle,
    #[error("Body must not be empty")]
    EmptyBody,
    #[error("Unsupported issue creation target: {0}")]
    UnsupportedTarget(String),
}

/// Validate an `IssueCreationRequest`, returning an error for invalid inputs.
pub fn validate_issue_creation_request(
    request: &IssueCreationRequest,
) -> Result<(), IssueCreationError> {
    if request.title.trim().is_empty() {
        return Err(IssueCreationError::EmptyTitle);
    }
    if request.body.trim().is_empty() {
        return Err(IssueCreationError::EmptyBody);
    }
    Ok(())
}

/// Parse a target string into `IssueCreationTarget`, returning an error for
/// unsupported values.
pub fn parse_target(target: &str) -> Result<IssueCreationTarget, IssueCreationError> {
    match target.trim().to_lowercase().as_str() {
        "github" => Ok(IssueCreationTarget::Github),
        "linear" => Ok(IssueCreationTarget::Linear),
        "jira" => Ok(IssueCreationTarget::Jira),
        "internal_feedback_table" => Ok(IssueCreationTarget::InternalFeedbackTable),
        other => Err(IssueCreationError::UnsupportedTarget(other.to_string())),
    }
}

/// Prepare an issue creation request for approval.
///
/// Always returns an `ApprovalRequiredIssueCreation` with `status:
/// "approval_required"`. No external API call, no database write, and no issue
/// is actually created.
pub fn prepare_issue_creation_for_approval(
    request: IssueCreationRequest,
) -> Result<ApprovalRequiredIssueCreation, IssueCreationError> {
    validate_issue_creation_request(&request)?;
    Ok(ApprovalRequiredIssueCreation {
        approval_request_id: Uuid::new_v4(),
        request,
        status: "approval_required".to_string(),
        message: "No external issue has been created. This request requires approval before it can be dispatched.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> IssueCreationRequest {
        IssueCreationRequest {
            target: IssueCreationTarget::Github,
            kind: IssueCreationKind::Bug,
            title: "Test issue".to_string(),
            body: "Something broke.".to_string(),
            labels: vec![],
            source_conversation_id: None,
            source_conversation_link: None,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn prepare_returns_approval_required() {
        let result = prepare_issue_creation_for_approval(valid_request()).unwrap();
        assert_eq!(result.status, "approval_required");
        assert!(!result.approval_request_id.is_nil());
        assert!(result.message.contains("No external issue"));
    }

    #[test]
    fn prepare_preserves_target() {
        let req = valid_request();
        let result = prepare_issue_creation_for_approval(req.clone()).unwrap();
        assert_eq!(result.request.target, IssueCreationTarget::Github);
    }

    #[test]
    fn empty_title_rejected() {
        let mut req = valid_request();
        req.title = "  ".to_string();
        let err = prepare_issue_creation_for_approval(req).unwrap_err();
        assert!(matches!(err, IssueCreationError::EmptyTitle));
    }

    #[test]
    fn empty_body_rejected() {
        let mut req = valid_request();
        req.body = "".to_string();
        let err = prepare_issue_creation_for_approval(req).unwrap_err();
        assert!(matches!(err, IssueCreationError::EmptyBody));
    }

    #[test]
    fn unsupported_target_rejected() {
        let err = parse_target("bitbucket").unwrap_err();
        assert!(matches!(err, IssueCreationError::UnsupportedTarget(_)));
    }

    #[test]
    fn parse_target_all_variants() {
        assert!(matches!(
            parse_target("github"),
            Ok(IssueCreationTarget::Github)
        ));
        assert!(matches!(
            parse_target("linear"),
            Ok(IssueCreationTarget::Linear)
        ));
        assert!(matches!(
            parse_target("jira"),
            Ok(IssueCreationTarget::Jira)
        ));
        assert!(matches!(
            parse_target("internal_feedback_table"),
            Ok(IssueCreationTarget::InternalFeedbackTable)
        ));
    }

    #[test]
    fn serde_roundtrip() {
        let req = valid_request();
        let json = serde_json::to_string(&req).unwrap();
        let back: IssueCreationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.target, req.target);
        assert_eq!(back.kind, req.kind);
        assert_eq!(back.title, req.title);
    }

    // -------------------------------------------------------------------------
    // Source conversation link & metadata preservation tests
    // -------------------------------------------------------------------------

    #[test]
    fn prepare_preserves_source_conversation_id_and_link() {
        let conv_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let req = IssueCreationRequest {
            target: IssueCreationTarget::Github,
            kind: IssueCreationKind::Bug,
            title: "Bug from chat".to_string(),
            body: "Details here.".to_string(),
            labels: vec![],
            source_conversation_id: Some(conv_id.to_string()),
            source_conversation_link: Some(format!("/ai-assistant?conversation_id={}", conv_id)),
            metadata: serde_json::json!({ "intake_channel": "winston_ai" }),
        };
        let result = prepare_issue_creation_for_approval(req.clone()).unwrap();
        assert_eq!(
            result.request.source_conversation_id,
            req.source_conversation_id
        );
        assert_eq!(
            result.request.source_conversation_link,
            req.source_conversation_link
        );
    }

    #[test]
    fn prepare_preserves_arbitrary_metadata() {
        let meta = serde_json::json!({
            "intake_channel": "winston_ai",
            "issue_kind": "bug",
            "custom_field": 42
        });
        let req = IssueCreationRequest {
            target: IssueCreationTarget::Linear,
            kind: IssueCreationKind::FeatureRequest,
            title: "Feature from chat".to_string(),
            body: "Body text.".to_string(),
            labels: vec!["enhancement".to_string()],
            source_conversation_id: None,
            source_conversation_link: None,
            metadata: meta.clone(),
        };
        let result = prepare_issue_creation_for_approval(req).unwrap();
        assert_eq!(result.request.metadata, meta);
    }

    #[test]
    fn old_caller_without_metadata_defaults_to_empty_object() {
        let json = r#"{
            "target": "github",
            "kind": "bug",
            "title": "Legacy caller",
            "body": "No metadata fields."
        }"#;
        let req: IssueCreationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.metadata, serde_json::json!({}));
        assert_eq!(req.source_conversation_link, None);
        assert_eq!(req.source_conversation_id, None);
    }
}
