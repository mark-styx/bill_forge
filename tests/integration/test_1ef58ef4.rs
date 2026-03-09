#[cfg(test)]
mod integration_tests {
    use super::*;
    use mockito::{mock, ServerUrl};
    use std::fs;

    #[test]
    fn test_tesseract_local_integration() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(200)
            .with_body(r#"{"results": "Sample OCR results from Tesseract Local"}"#)
            .create();

        let result = process_invoice(content, server_url, Provider::TesseractLocal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_awstextract_integration() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(200)
            .with_body(r#"{"results": "Sample OCR results from AWS Textract"}"#)
            .create();

        let result = process_invoice(content, server_url, Provider::AWSTextract);
        assert!(result.is_ok());
    }

    #[test]
    fn test_googlevision_integration() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(200)
            .with_body(r#"{"results": "Sample OCR results from Google Vision"}"#)
            .create();

        let result = process_invoice(content, server_url, Provider::GoogleVision);
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_handling_integration() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(500)
            .create();

        let result = process_invoice(content, server_url, Provider::TesseractLocal);
        assert!(matches!(result, Err(Error::ProviderError(_))));
    }
}