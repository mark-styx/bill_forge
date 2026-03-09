#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_parsing() {
        // Arrange
        let valid_document = "path/to/valid/document.pdf";

        // Act
        let result = parse_document(valid_document);

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DocumentFormat::PDF);
    }

    #[test]
    fn test_invalid_file_format() {
        // Arrange
        let invalid_document = "path/to/invalid/document.doc";

        // Act
        let result = parse_document(invalid_document);

        // Assert
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            DocumentError::InvalidFormat("Unsupported file format".to_string())
        );
    }
}