#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{mock, ServerUrl};
    use std::fs;

    #[test]
    fn test_tesseract_local_ocr() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let result = tesseract_local_ocr(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_awstextract_ocr() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(200)
            .with_body(r#"{"results": "Sample OCR results from AWS Textract"}"#)
            .create();

        let result = awstextract_ocr(content, &server_url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_googlevision_ocr() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let server_url = ServerUrl::from_str("http://localhost:8000").unwrap();
        let mock = mock("POST", "/ocr")
            .with_status(200)
            .with_body(r#"{"results": "Sample OCR results from Google Vision"}"#)
            .create();

        let result = googlevision_ocr(content, &server_url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_handling_tesseract_local() {
        let content = fs::read_to_string("path/to/sample/invoice.pdf").unwrap();
        let result = tesseract_local_ocr(content).err();
        assert!(matches!(result, Some(Error::TesseractLocalError(_))));
    }
}