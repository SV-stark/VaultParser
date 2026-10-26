use serde::{Deserialize, Serialize};

/// Bounding coordinates and text of a single word extracted from a PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedTable {
    /// List of user-facing column header names (excludes skipped columns).
    pub headers: Vec<String>,
    /// Indices of non-skipped columns used to extract row cells during export.
    pub active_indices: Vec<usize>,
    /// List of parsed transaction rows.
    pub rows: Vec<PageRow>,
}

/// Result of verifying the running balance equation across extracted transaction rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceReconciliation {
    /// Total number of consecutive rows evaluated against running balance arithmetic.
    pub total_evaluated: usize,
    /// Number of rows whose running balance matches arithmetic expectations.
    pub matching_rows: usize,
    /// Details of any arithmetic discrepancies detected.
    pub discrepancies: Vec<BalanceDiscrepancy>,
    /// Whether all evaluated rows reconciled completely (or no balance column was found).
    pub is_reconciled: bool,
}

/// Details of a running balance mismatch at a specific statement row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceDiscrepancy {
    /// 0-based index of the row within `table.rows`.
    pub row_index: usize,
    /// The unique row identifier (`PageRow::id`).
    pub row_id: String,
    /// Extracted date string for context.
    pub date: String,
    /// The balance recorded on the previous transaction row.
    pub previous_balance: f64,
    /// The debit/withdrawal amount for this row, if parsed.
    pub withdrawal: Option<f64>,
    /// The credit/deposit amount for this row, if parsed.
    pub deposit: Option<f64>,
    /// The actual balance reported on this row.
    pub reported_balance: f64,
    /// The expected balance calculated from running delta.
    pub expected_balance: f64,
    /// Difference between reported and expected balance (`reported - expected`).
    pub difference: f64,
}
