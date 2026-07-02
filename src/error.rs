use std::fmt;

#[derive(Debug)]
pub enum ExtractorError {
    PdfOpenError(String),
    PdfExtractionError(String),
    JsonError(String),
    XlsxWriteError(String),
    CsvWriteError(String),
    IOError(std::io::Error),
    DecryptError(String),
}

impl std::error::Error for ExtractorError {}

impl fmt::Display for ExtractorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PdfOpenError(e) => write!(f, "Failed to open PDF document: {}", e),
            Self::PdfExtractionError(e) => {
                write!(f, "Failed to extract layout words from PDF: {}", e)
            }
            Self::JsonError(e) => write!(f, "JSON error: {}", e),
            Self::XlsxWriteError(e) => write!(f, "Excel writing error: {}", e),
            Self::CsvWriteError(e) => write!(f, "CSV writing error: {}", e),
            Self::IOError(e) => write!(f, "I/O error: {}", e),
            Self::DecryptError(e) => write!(f, "PDF decryption error: {}", e),
        }
    }
}

impl From<std::io::Error> for ExtractorError {
    fn from(err: std::io::Error) -> Self {
        Self::IOError(err)
    }
}

impl From<serde_json::Error> for ExtractorError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonError(err.to_string())
    }
}
