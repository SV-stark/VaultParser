use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// X-coordinate column dividers, sorted in ascending order (values between 0.0 and 1.0 relative to page width)
    pub col_guides: Vec<f64>,
    /// Column purpose mappings corresponding to the divided columns (length should be col_guides.len() + 1)
    pub col_mappings: Vec<String>,
    /// Vertical distance tolerance (in points/pixels) for grouping words on the same line
    pub y_tolerance: f64,
    /// Merge lines that only have description text into the preceding row's description
    pub merge_multi_line: bool,
    /// Number of rows to skip at the beginning of the first page (e.g. table headers)
    pub skip_header_rows: usize,
    /// Number of rows to skip at the bottom of each page (e.g. running footers)
    pub skip_footer_rows: usize,
    /// Filter out rows that do not have a recognizable date in the mapped 'date' column
    pub filter_only_date: bool,
    /// Filter out rows that do not have a numerical amount in any mapped 'amount', 'debit', or 'credit' columns
    pub filter_only_amount: bool,
    /// Page top trim factor (0.0 to 1.0) - contents above this relative Y coordinate are excluded
    pub y_top_trim: f64,
    /// Page bottom trim factor (0.0 to 1.0) - contents below this relative Y coordinate are excluded
    pub y_bottom_trim: f64,
    /// Manual cell content overrides keyed by page number -> Y-coordinate (formatted) -> column index -> new value
    pub manual_edits: HashMap<String, HashMap<String, HashMap<String, String>>>,
    /// Manual row deletions keyed by page number -> Y-coordinate (formatted) -> is_deleted
    pub deleted_rows: HashMap<String, HashMap<String, bool>>,
    /// Optional password to decrypt the PDF document
    pub password: Option<String>,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            col_guides: Vec::new(),
            col_mappings: Vec::new(),
            y_tolerance: 6.0,
            merge_multi_line: true,
            skip_header_rows: 0,
            skip_footer_rows: 0,
            filter_only_date: false,
            filter_only_amount: false,
            y_top_trim: 0.0,
            y_bottom_trim: 1.0,
            manual_edits: HashMap::new(),
            deleted_rows: HashMap::new(),
            password: None,
        }
    }
}

impl ExtractionConfig {
    pub fn builder() -> ExtractionConfigBuilder {
        ExtractionConfigBuilder::new()
    }
}

pub struct ExtractionConfigBuilder {
    config: ExtractionConfig,
}

impl ExtractionConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: ExtractionConfig::default(),
        }
    }

    pub fn col_guides(mut self, guides: Vec<f64>) -> Self {
        self.config.col_guides = guides;
        self.config
            .col_guides
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self
    }

    pub fn col_mappings(mut self, mappings: Vec<String>) -> Self {
        self.config.col_mappings = mappings;
        self
    }

    pub fn y_tolerance(mut self, tolerance: f64) -> Self {
        self.config.y_tolerance = tolerance;
        self
    }

    pub fn merge_multi_line(mut self, merge: bool) -> Self {
        self.config.merge_multi_line = merge;
        self
    }

    pub fn skip_header_rows(mut self, count: usize) -> Self {
        self.config.skip_header_rows = count;
        self
    }

    pub fn skip_footer_rows(mut self, count: usize) -> Self {
        self.config.skip_footer_rows = count;
        self
    }

    pub fn filter_only_date(mut self, filter: bool) -> Self {
        self.config.filter_only_date = filter;
        self
    }

    pub fn filter_only_amount(mut self, filter: bool) -> Self {
        self.config.filter_only_amount = filter;
        self
    }

    pub fn y_top_trim(mut self, trim: f64) -> Self {
        self.config.y_top_trim = trim;
        self
    }

    pub fn y_bottom_trim(mut self, trim: f64) -> Self {
        self.config.y_bottom_trim = trim;
        self
    }

    pub fn manual_edits(
        mut self,
        edits: HashMap<String, HashMap<String, HashMap<String, String>>>,
    ) -> Self {
        self.config.manual_edits = edits;
        self
    }

    pub fn deleted_rows(mut self, deleted: HashMap<String, HashMap<String, bool>>) -> Self {
        self.config.deleted_rows = deleted;
        self
    }

    pub fn password(mut self, password: Option<String>) -> Self {
        self.config.password = password;
        self
    }

    pub fn build(self) -> ExtractionConfig {
        self.config
    }
}
