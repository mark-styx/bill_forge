pub enum Provider {
    TesseractLocal,
    AWSTextract,
    GoogleVision,
}

pub struct Error {
    kind: String,
    message: String,
}

impl Error {
    pub fn tesseract_local_error(msg: &str) -> Error {
        Error {
            kind: "TesseractLocalError".to_string(),
            message: msg.to_string(),
        }
    }

    pub fn provider_error(msg: &str) -> Error {
        Error {
            kind: "ProviderError".to_string(),
            message: msg.to_string(),
        }
    }
}