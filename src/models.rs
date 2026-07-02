use serde::{Deserialize, Serialize};

/// Bounding coordinates and text of a single word extracted from a PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordItem {
    /// The string text content of the word.
    pub text: String,
    /// Absolute left X coordinate of the word boundary (in points/pixels).
    pub x: f64,
    /// Absolute top Y coordinate of the word boundary (in points/pixels).
    pub y: f64,
    /// Horizontal width of the word boundary (in points/pixels).
    pub width: f64,
}

/// A single clustered row belonging to a specific page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRow {
    /// A unique identifier for the row, combining the page index and formatted Y-coordinate.
    pub id: String,
    /// Clustered vertical Y-coordinate representing this row.
    pub y: f64,
    /// 1-based index of the page where the row resides.
    pub page: usize,
    /// The text values of the cells divided by the configured column guides.
    pub cells: Vec<String>,
}

/// The final parsed structure containing headers and data records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedTable {
    /// List of user-facing column header names (excludes skipped columns).
    pub headers: Vec<String>,
    /// Indices of non-skipped columns used to extract row cells during export.
    pub active_indices: Vec<usize>,
    /// List of parsed transaction rows.
    pub rows: Vec<PageRow>,
}
