#[cfg(test)]
mod integration_tests {
    use crate::{backend_client, frontend};
    
    #[test]
    fn test_vendor_data_upload() {
        let vendor_data = r#"
            {
                "name": "Example Vendor",
                "address": "123 Example St",
                "contact": {
                    "email": "info@example.com",
                    "phone": "555-1234"
                }
            }
        "#;
        
        let response = backend_client::upload_vendor_data(vendor_data);
        
        assert!(response.is_ok());
    }

    #[test]
    fn test_api_endpoint_integration() {
        let request_body = serde_json::json!({
            "vendor_data": r#"
                {
                    "name": "Example Vendor",
                    "address": "123 Example St",
                    "contact": {
                        "email": "info@example.com",
                        "phone": "555-1234"
                    }
                }
            "#,
        });
        
        let response = frontend::send_matching_request(request_body);
        
        assert!(response.is_ok());
    }
}