use std::fmt;

/// Custom error types for the PDF extraction and parsing processes.
#[derive(Debug)]
pub enum ExtractorError {
    /// Failed to open or read the PDF file.
    PdfOpenError(String),
    /// Failed to extract layouts, text elements, or words from the PDF document.
    PdfExtractionError(String),
    /// Error parsing JSON data (e.g. settings or configuration).
    JsonError(String),
    /// Error compiling or writing the output to an Excel spreadsheet (.xlsx).
    XlsxWriteError(String),
    /// Error formatting or writing the output to a CSV file.
    CsvWriteError(String),
    /// Standard Input/Output system error.
    IOError(std::io::Error),
    /// Failed to decrypt the PDF document using lopdf.
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
