use crate::error::ExtractorError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration parameters for PDF table extraction.
///
/// This struct defines the extraction behaviors, coordinates for column boundaries,
/// vertical row clustering tolerance, trimming limits, and filters.
/// Usually constructed using [`ExtractionConfigBuilder`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub manual_edits: HashMap<String, HashMap<String, HashMap<String, String>>>,
    /// Manual row deletions keyed by page number -> Y-coordinate (formatted) -> is_deleted
    #[serde(default)]
    pub deleted_rows: HashMap<String, HashMap<String, bool>>,
    /// Optional password to decrypt the PDF document
    #[serde(default)]
    pub password: Option<String>,
    /// Automatically categorize transactions into a Category column (default: false)
    #[serde(default)]
    pub categorize: bool,
    /// Optional start date filter (inclusive) in YYYY-MM-DD or DD-MM-YYYY format
    #[serde(default)]
    pub from_date: Option<String>,
    /// Optional end date filter (inclusive) in YYYY-MM-DD or DD-MM-YYYY format
    #[serde(default)]
    pub to_date: Option<String>,
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
            categorize: false,
            from_date: None,
            to_date: None,
        }
    }
}

impl ExtractionConfig {
    /// Returns a new [`ExtractionConfigBuilder`] instance.
    ///
    /// # Examples
    /// ```
    /// use vaultparser::ExtractionConfig;
    ///
    /// let config = ExtractionConfig::builder()
    ///     .col_guides(vec![0.11, 0.42, 0.52, 0.62, 0.75, 0.88])
    ///     .col_mappings(vec![
    ///         "date".to_string(),
    ///         "description".to_string(),
    ///         "reference".to_string(),
    ///         "skip".to_string(),
    ///         "debit".to_string(),
    ///         "credit".to_string(),
    ///         "balance".to_string(),
    ///     ])
    ///     .y_tolerance(6.0)
    ///     .build()
    ///     .expect("valid configuration");
    /// assert_eq!(config.col_mappings.len(), config.col_guides.len() + 1);
    /// ```
    #[must_use]
    pub fn builder() -> ExtractionConfigBuilder {
        ExtractionConfigBuilder::new()
    }

    /// Validates the configuration parameters.
    ///
    /// # Errors
    /// Returns `ExtractorError::InvalidConfig` if column guides/mappings length, coordinates, or crop bounds are invalid.
    pub fn validate(&self) -> Result<(), ExtractorError> {
        if (!self.col_guides.is_empty() || !self.col_mappings.is_empty())
            && self.col_mappings.len() != self.col_guides.len() + 1
        {
            return Err(ExtractorError::InvalidConfig(format!(
                "Column mappings length ({}) must be equal to column guides length ({}) + 1",
                self.col_mappings.len(),
                self.col_guides.len()
            )));
        }
        for (i, &g) in self.col_guides.iter().enumerate() {
            if !(0.0..=1.0).contains(&g) || g.is_nan() {
                return Err(ExtractorError::InvalidConfig(format!(
                    "Column guide at index {} has invalid value {}: must be between 0.0 and 1.0",
                    i, g
                )));
            }
            if i > 0 && g < self.col_guides[i - 1] {
                return Err(ExtractorError::InvalidConfig(format!(
                    "Column guides must be sorted in ascending order (found {} after {})",
                    g,
                    self.col_guides[i - 1]
                )));
            }
        }
        if self.y_tolerance <= 0.0 || self.y_tolerance.is_nan() {
            return Err(ExtractorError::InvalidConfig(format!(
                "y_tolerance must be a positive number, found {}",
                self.y_tolerance
            )));
        }
        if self.y_top_trim < 0.0 || self.y_top_trim > 1.0 || self.y_top_trim.is_nan() {
            return Err(ExtractorError::InvalidConfig(format!(
                "y_top_trim must be between 0.0 and 1.0, found {}",
                self.y_top_trim
            )));
        }
        if self.y_bottom_trim < 0.0 || self.y_bottom_trim > 1.0 || self.y_bottom_trim.is_nan() {
            return Err(ExtractorError::InvalidConfig(format!(
                "y_bottom_trim must be between 0.0 and 1.0, found {}",
                self.y_bottom_trim
            )));
        }
        if self.y_top_trim > self.y_bottom_trim {
            return Err(ExtractorError::InvalidConfig(format!(
                "y_top_trim ({}) cannot be greater than y_bottom_trim ({})",
                self.y_top_trim, self.y_bottom_trim
            )));
        }
        if let Some(from_str) = &self.from_date
            && parse_date(from_str).is_none()
        {
            return Err(ExtractorError::InvalidConfig(format!(
                "Invalid from_date '{}': expected YYYY-MM-DD or DD-MM-YYYY format",
                from_str
            )));
        }
        if let Some(to_str) = &self.to_date
            && parse_date(to_str).is_none()
        {
            return Err(ExtractorError::InvalidConfig(format!(
                "Invalid to_date '{}': expected YYYY-MM-DD or DD-MM-YYYY format",
                to_str
            )));
        }
        if let (Some(from_str), Some(to_str)) = (&self.from_date, &self.to_date)
            && let (Some(from_d), Some(to_d)) = (parse_date(from_str), parse_date(to_str))
            && from_d > to_d
        {
            return Err(ExtractorError::InvalidConfig(format!(
                "from_date ({}) cannot be after to_date ({})",
                from_str, to_str
            )));
        }
        Ok(())
    }
}

/// Helper to parse standard date strings into [`chrono::NaiveDate`].
pub fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    let clean = s.trim();
    if clean.is_empty() {
        return None;
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%d-%m-%Y") {
        return Some(d);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%d/%m/%Y") {
        return Some(d);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%Y/%m/%d") {
        return Some(d);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%d-%b-%Y") {
        return Some(d);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(clean, "%d %b %Y") {
        return Some(d);
    }
    None
}

/// A builder helper for configuring and creating an [`ExtractionConfig`].
pub struct ExtractionConfigBuilder {
    config: ExtractionConfig,
}

impl Default for ExtractionConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractionConfigBuilder {
    /// Creates a new `ExtractionConfigBuilder` with default configurations.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: ExtractionConfig::default(),
        }
    }

    /// Sets the horizontal column division guides (0.0 to 1.0 relative to page width).
    /// These guides are automatically sorted in ascending order.
    #[must_use]
    pub fn col_guides(mut self, mut guides: Vec<f64>) -> Self {
        guides.sort_by(f64::total_cmp);
        self.config.col_guides = guides;
        self
    }

    /// Sets the column mapping labels corresponding to the divided columns.
    /// The length of mappings must be equal to `col_guides.len() + 1`.
    #[must_use]
    pub fn col_mappings(mut self, mappings: Vec<String>) -> Self {
        self.config.col_mappings = mappings;
        self
    }

    /// Sets the vertical distance tolerance (in points/pixels) for grouping words on the same row line.
    #[must_use]
    pub fn y_tolerance(mut self, tolerance: f64) -> Self {
        self.config.y_tolerance = tolerance;
        self
    }

    /// Configures whether to merge description-only lines into the preceding transaction row's description.
    #[must_use]
    pub fn merge_multi_line(mut self, merge: bool) -> Self {
        self.config.merge_multi_line = merge;
        self
    }

    /// Sets the number of rows to skip at the top of the first page (useful for skipping main statement headers).
    #[must_use]
    pub fn skip_header_rows(mut self, count: usize) -> Self {
        self.config.skip_header_rows = count;
        self
    }

    /// Sets the number of rows to skip at the bottom of each page (useful for skipping page numbers and footers).
    #[must_use]
    pub fn skip_footer_rows(mut self, count: usize) -> Self {
        self.config.skip_footer_rows = count;
        self
    }

    /// Configures whether to filter out rows that don't have a valid date in the mapped `date` column.
    #[must_use]
    pub fn filter_only_date(mut self, filter: bool) -> Self {
        self.config.filter_only_date = filter;
        self
    }

    /// Configures whether to filter out rows that don't have an amount in any mapped `amount`, `debit`, or `credit` columns.
    #[must_use]
    pub fn filter_only_amount(mut self, filter: bool) -> Self {
        self.config.filter_only_amount = filter;
        self
    }

    /// Sets the relative top margin for exclusion (0.0 to 1.0). Content above this is ignored.
    #[must_use]
    pub fn y_top_trim(mut self, trim: f64) -> Self {
        self.config.y_top_trim = trim;
        self
    }

    /// Sets the relative bottom margin for exclusion (0.0 to 1.0). Content below this is ignored.
    #[must_use]
    pub fn y_bottom_trim(mut self, trim: f64) -> Self {
        self.config.y_bottom_trim = trim;
        self
    }

    /// Applies manual cell content edits/overrides.
    #[must_use]
    pub fn manual_edits(
        mut self,
        edits: HashMap<String, HashMap<String, HashMap<String, String>>>,
    ) -> Self {
        self.config.manual_edits = edits;
        self
    }

    /// Applies manual row deletions.
    #[must_use]
    pub fn deleted_rows(mut self, deleted: HashMap<String, HashMap<String, bool>>) -> Self {
        self.config.deleted_rows = deleted;
        self
    }

    /// Sets the decryption password for the PDF document.
    #[must_use]
    pub fn password(mut self, password: Option<String>) -> Self {
        self.config.password = password;
        self
    }

    /// Configures whether to automatically categorize transactions (default: false).
    #[must_use]
    pub fn categorize(mut self, categorize: bool) -> Self {
        self.config.categorize = categorize;
        self
    }

    /// Sets the inclusive start date filter (e.g. "2023-01-01" or "01-01-2023").
    #[must_use]
    pub fn from_date(mut self, from_date: Option<String>) -> Self {
        self.config.from_date = from_date;
        self
    }

    /// Sets the inclusive end date filter (e.g. "2023-12-31" or "31-12-2023").
    #[must_use]
    pub fn to_date(mut self, to_date: Option<String>) -> Self {
        self.config.to_date = to_date;
        self
    }

    /// Builds and returns the configured [`ExtractionConfig`].
    ///
    /// # Errors
    /// Returns `ExtractorError::InvalidConfig` if the configuration is invalid.
    ///
    /// # Examples
    /// ```
    /// use vaultparser::ExtractionConfig;
    ///
    /// // Mappings must be exactly one more than guides.
    /// let result = ExtractionConfig::builder()
    ///     .col_guides(vec![0.2, 0.5])
    ///     .col_mappings(vec!["date".to_string()])
    ///     .build();
    /// assert!(result.is_err());
    /// ```
    pub fn build(self) -> Result<ExtractionConfig, ExtractorError> {
        self.config.validate()?;
        Ok(self.config)
    }
}
