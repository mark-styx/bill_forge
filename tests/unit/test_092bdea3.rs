#[cfg(test)]
mod vendor_data_tests {
    use crate::vendor_data::validate_vendor_data;
    
    #[test]
    fn test_validate_vendor_data_valid_json() {
        let valid_json = r#"
            {
                "name": "Example Vendor",
                "address": "123 Example St",
                "contact": {
                    "email": "info@example.com",
                    "phone": "555-1234"
                }
            }
        "#;
        
        assert!(validate_vendor_data(valid_json).is_ok());
    }

    #[test]
    fn test_validate_vendor_data_invalid_json() {
        let invalid_json = r#"
            {
                "name": "Example Vendor",
                "address": "123 Example St",
                contact: {  // Missing comma
                    "email": "info@example.com",
                    "phone": "555-1234"
                }
            }
        "#;
        
        assert!(validate_vendor_data(invalid_json).is_err());
    }

    #[test]
    fn test_validate_vendor_data_missing_field() {
        let missing_field_json = r#"
            {
                "name": "Example Vendor",
                "address": "123 Example St"  // Missing contact field
            }
        "#;
        
        assert!(validate_vendor_data(missing_field_json).is_err());
    }
}