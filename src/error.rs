use thiserror::Error;

/// Custom error types for the PDF extraction and parsing processes.
#[derive(Debug, Error)]
pub enum ExtractorError {
    /// Failed to open or read the PDF file.
    #[error("Failed to open PDF document: {0}")]
    PdfOpenError(String),
    /// Failed to extract layouts, text elements, or words from the PDF document.
    #[error("Failed to extract layout words from PDF: {0}")]
    PdfExtractionError(String),
    /// Error parsing JSON data (e.g. settings or configuration).
    #[error("JSON error: {0}")]
    JsonError(String),
    /// Error compiling or writing the output to an Excel spreadsheet (.xlsx).
    #[error("Excel writing error: {0}")]
    XlsxWriteError(String),
    /// Error formatting or writing the output to a CSV file.
    #[error("CSV writing error: {0}")]
    CsvWriteError(String),
    /// Standard Input/Output system error.
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    /// Failed to decrypt the PDF document using lopdf.
    #[error("PDF decryption error: {0}")]
    DecryptError(String),
    /// Invalid configuration parameters supplied.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    /// Password required or incorrect password supplied for PDF decryption.
    #[error("Password error: {0}")]
    PasswordError(String),
    /// The PDF appears to be a scanned image or contains no extractable digital text.
    #[error("Scanned PDF detected: {0}")]
    ScannedPdf(String),
}

impl From<serde_json::Error> for ExtractorError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonError(err.to_string())
    }
}
