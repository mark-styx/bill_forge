// Example of a shared utility function
fn parse_document(file_path: &str) -> Result<DocumentFormat, DocumentError> {
    // Implementation details
}

#[derive(Debug)]
enum DocumentFormat {
    PDF,
    DOCX,
}

#[derive(Debug)]
enum DocumentError {
    InvalidFormat(String),
}