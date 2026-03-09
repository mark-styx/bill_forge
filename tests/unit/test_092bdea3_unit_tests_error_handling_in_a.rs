#[cfg(test)]
mod api_error_tests {
    use crate::api_client::{make_api_call, make_authed_api_call};
    
    #[test]
    fn test_make_api_call_invalid_token() {
        let invalid_token = "invalid_token";
        
        assert!(matches!(
            make_api_call(invalid_token),
            Err(ApiError::Unauthorized(_))
        ));
    }

    #[test]
    fn test_make_authed_api_call_invalid_token() {
        let invalid_token = "invalid_token";
        
        assert!(matches!(
            make_authed_api_call(invalid_token, Some("data")),
            Err(ApiError::Unauthorized(_))
        ));
    }
}