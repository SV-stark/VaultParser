use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, error, info, warn};

use pdfsink_rs::PdfDocument;

use crate::config::ExtractionConfig;
use crate::error::ExtractorError;
use crate::models::{ExtractedTable, PageRow, WordItem};

#[derive(Debug, Clone)]
struct GroupedRow {
    anchor_y: f64,
    y: f64,
    items: Vec<WordItem>,
}

fn is_possible_date(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let cleaned = val.trim();
    if cleaned.len() > 25 || cleaned.len() < 5 {
        return false;
    }

    let has_separator = cleaned.contains('-') || cleaned.contains('/') || cleaned.contains('.');
    let has_space = cleaned.contains(' ');

    if !has_separator && !has_space {
        return false;
    }

    if has_space && !has_separator {
        let months = [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ];
        let lower = cleaned.to_lowercase();
        let contains_month = months.iter().any(|m| lower.contains(m));
        if !contains_month {
            return false;
        }
    }

    if cleaned.chars().filter(|c| c.is_ascii_alphabetic()).count() > 6 {
        return false;
    }
    cleaned.chars().filter(|c| c.is_ascii_digit()).count() >= 2
}

fn is_possible_amount(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let mut cleaned = val
        .replace(['$', '£', '€', '₹', ','], "")
        .trim()
        .to_string();

    let keywords = [
        "Rs.", "RS.", "rs.", "Rs", "RS", "rs", "INR", "inr", "Cr.", "CR.", "cr.", "Cr", "CR", "cr",
        "Dr.", "DR.", "dr.", "Dr", "DR", "dr",
    ];
    for kw in &keywords {
        cleaned = cleaned.replace(kw, "");
    }
    let cleaned = cleaned.trim();
    cleaned.parse::<f64>().is_ok()
}

fn standardize_date(val: &str) -> String {
    let cleaned = val.trim();
    if cleaned.is_empty() {
        return String::new();
    }

    // Try a series of common datetime formats first
    let datetime_formats = [
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y %H:%M",
        "%d-%m-%Y %H:%M:%S",
        "%d-%m-%Y %H:%M",
        "%d-%b-%Y %H:%M:%S",
        "%d-%b-%Y %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];

    for fmt in &datetime_formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(cleaned, fmt) {
            return dt.format("%d-%m-%Y").to_string();
        }
    }

    // Try a series of common date formats
    let formats = [
        "%d/%m/%y", // 30/04/25
        "%d/%m/%Y", // 30/04/2025
        "%d-%m-%y", // 30-04-25
        "%d-%m-%Y", // 30-04-2025
        "%d.%m.%y", // 30.04.25
        "%d.%m.%Y", // 30.04.2025
        "%d %b %y", // 30 Apr 25
        "%d %b %Y", // 30 Apr 2025
        "%d-%b-%y", // 30-Apr-25
        "%d-%b-%Y", // 30-Apr-2025
        "%d %B %Y", // 30 April 2025
        "%d-%B-%Y", // 30-April-2025
        "%Y-%m-%d", // 2025-04-30
    ];

    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(cleaned, fmt) {
            return dt.format("%d-%m-%Y").to_string();
        }
    }

    // Try parsing the first whitespace-separated token if there are trailing notes/timestamps
    if let Some(first_part) = cleaned.split_whitespace().next().filter(|p| *p != cleaned) {
        for fmt in &formats {
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(first_part, fmt) {
                return dt.format("%d-%m-%Y").to_string();
            }
        }
    }

    // Fall back to original string if no format matches
    cleaned.to_string()
}

struct TempFileGuard {
    path: Option<PathBuf>,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.as_ref().filter(|p| p.exists()) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Helper to decrypt a PDF natively with lopdf if it's encrypted
fn decrypt_pdf_if_needed(
    temp_path: &Path,
    password: Option<&str>,
) -> Result<PathBuf, ExtractorError> {
    let use_decrypted = match PdfDocument::open(temp_path) {
        Ok(pdf) => pdf.pages().is_empty(),
        Err(_) => true,
    };

    if use_decrypted {
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let decrypted_name = format!(
            "{}_decrypted_{}.pdf",
            temp_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("temp"),
            ts
        );
        let decrypted_path = temp_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(decrypted_name);
        let pwd = password.unwrap_or("");

        info!(
            "PDF is encrypted or layout is unreadable. Attempting native decryption for '{}'...",
            temp_path.display()
        );

        // Native lopdf decryption using load_with_password (handles empty password AES-256)
        let mut doc = lopdf::Document::load_with_password(temp_path, pwd).map_err(|e| {
            error!(
                "LOPDF decryption failed for PDF '{}': {:?}",
                temp_path.display(),
                e
            );
            let err_str = format!("{:?}", e);
            if err_str.to_lowercase().contains("password") {
                ExtractorError::PasswordError("Incorrect or missing password".to_string())
            } else {
                ExtractorError::DecryptError(format!(
                    "Failed to load/decrypt PDF with lopdf: {:?}",
                    e
                ))
            }
        })?;

        if doc.is_encrypted() {
            doc.trailer.remove(b"Encrypt");
        }

        doc.save(&decrypted_path).map_err(|e| {
            error!("Failed to save decrypted PDF copy: {:?}", e);
            ExtractorError::DecryptError(format!("Failed to save decrypted PDF: {:?}", e))
        })?;

        info!(
            "PDF successfully decrypted and saved to '{}'.",
            decrypted_path.display()
        );
        Ok(decrypted_path)
    } else {
        debug!(
            "PDF '{}' is already unencrypted and readable directly.",
            temp_path.display()
        );
        Ok(temp_path.to_path_buf())
    }
}

/// Automatically detect column guides by analyzing the horizontal density of text spans
#[allow(clippy::needless_range_loop)]
pub fn detect_column_guides<P: AsRef<Path>>(
    pdf_path: P,
    password: Option<&str>,
    y_top_trim: f64,
    y_bottom_trim: f64,
) -> Result<Vec<f64>, ExtractorError> {
    if !(0.0..=1.0).contains(&y_top_trim) {
        return Err(ExtractorError::InvalidConfig(format!(
            "y_top_trim must be between 0.0 and 1.0, found {}",
            y_top_trim
        )));
    }
    if !(0.0..=1.0).contains(&y_bottom_trim) {
        return Err(ExtractorError::InvalidConfig(format!(
            "y_bottom_trim must be between 0.0 and 1.0, found {}",
            y_bottom_trim
        )));
    }
    if y_top_trim > y_bottom_trim {
        return Err(ExtractorError::InvalidConfig(format!(
            "y_top_trim ({}) cannot be greater than y_bottom_trim ({})",
            y_top_trim, y_bottom_trim
        )));
    }

    let path_ref = pdf_path.as_ref();
    info!(
        "Starting layout analysis to auto-detect columns for '{}'...",
        path_ref.display()
    );

    let processed_path = decrypt_pdf_if_needed(path_ref, password)?;
    let _guard = if processed_path != path_ref {
        Some(TempFileGuard::new(processed_path.clone()))
    } else {
        None
    };

    let pdf = PdfDocument::open(&processed_path).map_err(|e| {
        error!("Failed to open PDF during layout analysis: {:?}", e);
        ExtractorError::PdfOpenError(format!("{:?}", e))
    })?;

    let pages = pdf.pages();
    if pages.is_empty() {
        warn!("PDF has no pages. Skipping layout analysis.");
        return Ok(Vec::new());
    }

    // We divide the horizontal page width into 1000 buckets (0.0 to 1.0)
    let mut histogram = vec![0usize; 1000];
    let sample_pages = pages.iter().take(5); // Analyze up to first 5 pages for layout
    let pages_count = sample_pages.len();
    debug!(
        "Analyzing first {} pages for horizontal text distribution...",
        pages_count
    );

    for page in sample_pages {
        let width = page.width;
        let height = page.height;
        let top_px = y_top_trim * height;
        let bottom_px = y_bottom_trim * height;

        let words = page.extract_words();
        for w in words {
            if w.top >= top_px && w.top <= bottom_px {
                let x0_pct = (w.x0 / width).clamp(0.0, 1.0);
                let x1_pct = (w.x1 / width).clamp(0.0, 1.0);

                let start_bucket = (x0_pct * 1000.0) as usize;
                let end_bucket = (x1_pct * 1000.0) as usize;

                for bucket in start_bucket..=end_bucket {
                    if bucket < 1000 {
                        histogram[bucket] += 1;
                    }
                }
            }
        }
    }

    // Clean up decrypted file if it was created temporarily
    if processed_path != path_ref {
        let _ = std::fs::remove_file(&processed_path);
    }

    // Find gaps / valleys. A valley is where the occupancy histogram is low.
    // We only look in the range of 5% to 95% of the page to avoid page margins.
    let mut guides = Vec::new();
    let mut in_valley = false;
    let mut valley_start = 0;

    let total_occupancy: usize = histogram.iter().sum();
    let avg_occupancy = total_occupancy as f64 / 1000.0;
    let threshold = (avg_occupancy * 0.05).max(1.0) as usize;

    for i in 50..950 {
        let val = histogram[i];
        if val <= threshold {
            if !in_valley {
                in_valley = true;
                valley_start = i;
            }
        } else {
            if in_valley {
                in_valley = false;
                let valley_end = i - 1;
                let width = valley_end - valley_start + 1;

                // Gaps should be at least 1% of the page width (10 buckets)
                if width >= 10 {
                    let center = (valley_start + valley_end) as f64 / 2000.0;
                    guides.push(center);
                }
            }
        }
    }

    if in_valley {
        let valley_end = 949;
        let width = valley_end - valley_start + 1;
        if width >= 10 {
            let center = (valley_start + valley_end) as f64 / 2000.0;
            guides.push(center);
        }
    }

    info!(
        "Layout analysis complete. Auto-detected {} column delimiters: {:?}",
        guides.len(),
        guides
    );
    Ok(guides)
}

/// Auto-detect bank preset from a PDF file path by scanning its text content.
pub fn detect_preset_from_file<P: AsRef<Path>>(
    pdf_path: P,
    password: Option<&str>,
) -> Result<Option<crate::presets::BankPreset>, ExtractorError> {
    let path_ref = pdf_path.as_ref();
    let processed_path = decrypt_pdf_if_needed(path_ref, password)?;
    let _guard = if processed_path != path_ref {
        Some(TempFileGuard::new(processed_path.clone()))
    } else {
        None
    };

    let pdf = PdfDocument::open(&processed_path)
        .map_err(|e| ExtractorError::PdfOpenError(format!("{:?}", e)))?;

    let pages = pdf.pages();
    if pages.is_empty() {
        return Ok(None);
    }

    let first_page = &pages[0];
    let words = first_page.extract_words();
    let full_text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<&str>>()
        .join(" ")
        .to_uppercase();

    if full_text.contains("HDFC BANK") || full_text.contains("HDFCBANK") {
        Ok(Some(crate::presets::BankPreset::Hdfc))
    } else if full_text.contains("STATE BANK OF INDIA") || full_text.contains("SBI ") {
        Ok(Some(crate::presets::BankPreset::Sbi))
    } else if full_text.contains("CANARA") {
        Ok(Some(crate::presets::BankPreset::Canara))
    } else if full_text.contains("UNION BANK") {
        Ok(Some(crate::presets::BankPreset::Union))
    } else if full_text.contains("UCO BANK") {
        Ok(Some(crate::presets::BankPreset::Uco))
    } else if full_text.contains("INDIAN BANK") || full_text.contains("ALLAHABAD") {
        Ok(Some(crate::presets::BankPreset::Indian))
    } else if full_text.contains("H P STATE CO-OP")
        || full_text.contains("CO-OPERATIVE BANK")
        || full_text.contains("HPSCB")
    {
        Ok(Some(crate::presets::BankPreset::Hpscb))
    } else if full_text.contains("ICICI BANK") {
        Ok(Some(crate::presets::BankPreset::Icici))
    } else if full_text.contains("PUNJAB NATIONAL BANK") || full_text.contains("PNB ") {
        Ok(Some(crate::presets::BankPreset::Pnb))
    } else if full_text.contains("KOTAK MAHINDRA") || full_text.contains("KOTAK BANK") {
        Ok(Some(crate::presets::BankPreset::Kotak))
    } else {
        Ok(None)
    }
}

/// Primary function to extract tabular data from a PDF file path
pub fn extract_from_file<P: AsRef<Path>>(
    pdf_path: P,
    config: &ExtractionConfig,
) -> Result<ExtractedTable, ExtractorError> {
    config.validate()?;

    let path_ref = pdf_path.as_ref();
    info!(
        "Starting PDF table extraction for '{}'...",
        path_ref.display()
    );

    let processed_path = decrypt_pdf_if_needed(path_ref, config.password.as_deref())?;
    let _guard = if processed_path != path_ref {
        Some(TempFileGuard::new(processed_path.clone()))
    } else {
        None
    };

    let pdf = PdfDocument::open(&processed_path).map_err(|e| {
        error!("Failed to open PDF document: {:?}", e);
        let err_str = format!("{:?}", e);
        if err_str.to_lowercase().contains("password") || err_str.contains("Xref") {
            ExtractorError::PasswordError("Incorrect or missing password".to_string())
        } else {
            ExtractorError::PdfOpenError(format!("{:?}", e))
        }
    })?;

    let mut all_rows = Vec::new();
    let pages = pdf.pages();

    for (page_idx, page) in pages.iter().enumerate() {
        let width = page.width;
        let height = page.height;
        let top_px = config.y_top_trim * height;
        let bottom_px = config.y_bottom_trim * height;

        let words = page.extract_words();
        let filtered_words: Vec<&pdfsink_rs::Word> = words
            .iter()
            .filter(|w| w.top >= top_px && w.top <= bottom_px)
            .collect();

        if filtered_words.is_empty() {
            continue;
        }

        let mut items: Vec<WordItem> = filtered_words
            .iter()
            .map(|w| WordItem {
                text: w.text.clone(),
                x: w.x0,
                y: w.top,
                width: w.x1 - w.x0,
            })
            .collect();

        items.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

        let mut grouped_rows: Vec<GroupedRow> = Vec::new();
        for item in items {
            let mut found_row_idx = None;
            for (idx, r) in grouped_rows.iter().enumerate() {
                if (r.anchor_y - item.y).abs() <= config.y_tolerance {
                    found_row_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = found_row_idx {
                grouped_rows[idx].items.push(item);
            } else {
                grouped_rows.push(GroupedRow {
                    anchor_y: item.y,
                    y: item.y,
                    items: vec![item],
                });
            }
        }

        for r in &mut grouped_rows {
            let total_y: f64 = r.items.iter().map(|it| it.y).sum();
            r.y = total_y / r.items.len() as f64;
            r.items
                .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        }
        grouped_rows.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

        let page_guides: Vec<f64> = config.col_guides.iter().map(|&g| g * width).collect();
        let num_cols = page_guides.len() + 1;

        let mut page_rows = Vec::new();
        for (row_idx, r) in grouped_rows.iter().enumerate() {
            let mut cell_contents = vec![Vec::new(); num_cols];
            for item in &r.items {
                let center_x = item.x + item.width / 2.0;
                let mut col_idx = 0;
                while col_idx < page_guides.len() && center_x > page_guides[col_idx] {
                    col_idx += 1;
                }
                cell_contents[col_idx].push(item.clone());
            }

            let mut cells = Vec::new();
            for items_in_col in &mut cell_contents {
                items_in_col
                    .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
                let cell_text = items_in_col
                    .iter()
                    .map(|it| it.text.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
                    .trim()
                    .to_string();
                cells.push(cell_text);
            }

            page_rows.push(PageRow {
                id: format!("row-{}-{}-{:.2}", page_idx, row_idx, r.y),
                y: r.y,
                page: page_idx + 1,
                cells,
            });
        }

        page_rows.retain(|row| row.cells.iter().any(|c| !c.is_empty()));

        let page_str = (page_idx + 1).to_string();

        // Handle manual row deletions
        if let Some(deletes) = config.deleted_rows.get(&page_str) {
            page_rows.retain(|row| {
                let y_key = format!("{:.2}", row.y);
                !deletes.contains_key(&y_key)
            });
        }

        // Handle manual cell edits
        if let Some(edits) = config.manual_edits.get(&page_str) {
            for row in &mut page_rows {
                let y_key = format!("{:.2}", row.y);
                if let Some(col_edits) = edits.get(&y_key) {
                    for (col_idx_str, val) in col_edits {
                        if let Some(cell) = col_idx_str
                            .parse::<usize>()
                            .ok()
                            .and_then(|idx| row.cells.get_mut(idx))
                        {
                            *cell = val.clone();
                        }
                    }
                }
            }
        }

        let desc_idx = config.col_mappings.iter().position(|m| m == "description");
        let date_idx = config.col_mappings.iter().position(|m| m == "date");
        let val_date_idx = config.col_mappings.iter().position(|m| m == "value_date");
        let amount_idx = config.col_mappings.iter().position(|m| m == "amount");
        let debit_idx = config.col_mappings.iter().position(|m| m == "debit");
        let credit_idx = config.col_mappings.iter().position(|m| m == "credit");
        let balance_idx = config.col_mappings.iter().position(|m| m == "balance");

        // Standardize date formats using chrono NaiveDate parser
        for idx in [date_idx, val_date_idx].into_iter().flatten() {
            for row in &mut page_rows {
                if idx < row.cells.len() && !row.cells[idx].is_empty() {
                    row.cells[idx] = standardize_date(&row.cells[idx]);
                }
            }
        }

        if let Some(desc_idx) = config.merge_multi_line.then_some(desc_idx).flatten() {
            let mut merged = Vec::new();
            for row in page_rows {
                let cell_desc = &row.cells[desc_idx];
                let is_date_empty = date_idx.is_none_or(|idx| row.cells[idx].is_empty());
                let is_amount_empty = amount_idx.is_none_or(|idx| row.cells[idx].is_empty());
                let is_debit_empty = debit_idx.is_none_or(|idx| row.cells[idx].is_empty());
                let is_credit_empty = credit_idx.is_none_or(|idx| row.cells[idx].is_empty());
                let is_balance_empty = balance_idx.is_none_or(|idx| row.cells[idx].is_empty());

                let is_continuation = is_date_empty
                    && is_amount_empty
                    && is_debit_empty
                    && is_credit_empty
                    && is_balance_empty
                    && !cell_desc.is_empty();

                if is_continuation && !merged.is_empty() {
                    let last_row: &mut PageRow = merged.last_mut().unwrap();
                    last_row.cells[desc_idx] =
                        format!("{} {}", last_row.cells[desc_idx], cell_desc)
                            .trim()
                            .to_string();
                } else {
                    merged.push(row);
                }
            }
            page_rows = merged;
        }

        // Skip header rows on page 1
        if page_idx == 0 && config.skip_header_rows > 0 {
            let skip = config.skip_header_rows.min(page_rows.len());
            page_rows.drain(0..skip);
        }
        // Skip footer rows on all pages
        if config.skip_footer_rows > 0 {
            let len = page_rows.len();
            let skip = config.skip_footer_rows.min(len);
            page_rows.truncate(len - skip);
        }

        let mut filtered_rows = Vec::new();
        for row in page_rows {
            let mut keep = true;
            if config.filter_only_date
                && date_idx.is_some_and(|idx| !is_possible_date(&row.cells[idx]))
            {
                keep = false;
            }
            if config.filter_only_amount {
                let has_amount = amount_idx.is_some_and(|idx| is_possible_amount(&row.cells[idx]));
                let has_debit = debit_idx.is_some_and(|idx| is_possible_amount(&row.cells[idx]));
                let has_credit = credit_idx.is_some_and(|idx| is_possible_amount(&row.cells[idx]));

                if !has_amount && !has_debit && !has_credit {
                    keep = false;
                }
            }
            if keep {
                filtered_rows.push(row);
            }
        }

        all_rows.extend(filtered_rows);
    }

    // Calculate active indices and headers
    let mut active_indices = Vec::new();
    let mut headers = Vec::new();
    for (idx, m) in config.col_mappings.iter().enumerate() {
        if m != "skip" {
            active_indices.push(idx);
            headers.push(m.to_uppercase());
        }
    }

    info!(
        "Successfully extracted {} total transaction rows.",
        all_rows.len()
    );

    Ok(ExtractedTable {
        headers,
        active_indices,
        rows: all_rows,
    })
}

/// Extract tabular data directly from PDF bytes
pub fn extract_from_bytes(
    pdf_bytes: &[u8],
    config: &ExtractionConfig,
) -> Result<ExtractedTable, ExtractorError> {
    config.validate()?;

    // Create a temp file to hold the bytes for parsing
    let temp_dir = Path::new("temp");
    fs::create_dir_all(temp_dir)?;

    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let temp_name = format!("temp_{}.pdf", ts);
    let temp_path = temp_dir.join(&temp_name);

    fs::write(&temp_path, pdf_bytes)?;
    let _guard = TempFileGuard::new(temp_path.clone());

    extract_from_file(&temp_path, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::BankPreset;

    #[test]
    fn test_is_possible_date() {
        assert!(is_possible_date("01/02/2023"));
        assert!(is_possible_date("15-08-1947"));
        assert!(is_possible_date("2026-07-01"));
        assert!(is_possible_date("30 Apr 2025"));
        assert!(is_possible_date("30 Oct 25"));
        assert!(is_possible_date("30/04/2025 10:15:30"));
        assert!(is_possible_date("01-Jan-2025"));
        assert!(!is_possible_date(""));
        assert!(!is_possible_date("Not a date"));
        assert!(!is_possible_date("DEP 123"));
    }

    #[test]
    fn test_is_possible_amount() {
        assert!(is_possible_amount("123.45"));
        assert!(is_possible_amount("$1,234.50"));
        assert!(is_possible_amount("€100"));
        assert!(is_possible_amount("-50.00"));
        assert!(is_possible_amount("1,250.00 Cr"));
        assert!(is_possible_amount("500.00 Dr"));
        assert!(is_possible_amount("Rs. 1,250.00"));
        assert!(is_possible_amount("₹ 5,000.50 DR"));
        assert!(is_possible_amount("100.00 INR"));
        assert!(!is_possible_amount("abc"));
        assert!(!is_possible_amount(""));
    }

    #[test]
    fn test_sample_hdfc_extraction() {
        let pdf_path = Path::new("static/sample_hdfc_decrypted.pdf");
        if pdf_path.exists() {
            let config = BankPreset::Hdfc.config();
            let table_res = extract_from_file(pdf_path, &config);
            assert!(
                table_res.is_ok(),
                "Failed to extract from HDFC statement: {:?}",
                table_res.err()
            );
            let table = table_res.unwrap();
            assert!(
                !table.rows.is_empty(),
                "Extracted table should not be empty"
            );

            // Check headers mapping (active column names in uppercase, excluding SKIP)
            assert_eq!(
                table.headers,
                vec![
                    "DATE",
                    "DESCRIPTION",
                    "REFERENCE",
                    "DEBIT",
                    "CREDIT",
                    "BALANCE"
                ]
            );

            // Check first extracted row
            let first_row = &table.rows[0];
            assert!(
                !first_row.cells.is_empty(),
                "First row cells should not be empty"
            );
            println!("First Row: {:?}", first_row);
        } else {
            eprintln!(
                "Warning: sample_hdfc_decrypted.pdf not found at static/sample_hdfc_decrypted.pdf, skipping integration test"
            );
        }
    }

    #[test]
    fn test_encrypted_sample_hdfc_extraction() {
        let pdf_path = Path::new("static/sample_hdfc.pdf");
        if pdf_path.exists() {
            let config = BankPreset::Hdfc.config();
            let table_res = extract_from_file(pdf_path, &config);
            assert!(
                table_res.is_ok(),
                "Failed to extract from encrypted HDFC statement: {:?}",
                table_res.err()
            );
            let table = table_res.unwrap();
            assert!(
                !table.rows.is_empty(),
                "Extracted table should not be empty"
            );
            println!("Encrypted HDFC Statement Headers: {:?}", table.headers);
            println!("Encrypted HDFC Statement First Row: {:?}", table.rows[0]);
            println!("Total Rows Extracted from Encrypted: {}", table.rows.len());
        } else {
            eprintln!(
                "Warning: sample_hdfc.pdf not found at static/sample_hdfc.pdf, skipping integration test"
            );
        }
    }

    #[test]
    fn test_rks_user_charges_extraction() {
        let pdf_path = Path::new("RKS USER CHARGES bank.pdf");
        if pdf_path.exists() {
            let config = BankPreset::Hdfc.config();
            let table_res = extract_from_file(pdf_path, &config);
            assert!(
                table_res.is_ok(),
                "Failed to extract from RKS statement: {:?}",
                table_res.err()
            );
            let table = table_res.unwrap();
            assert!(
                !table.rows.is_empty(),
                "Extracted table should not be empty"
            );

            println!("RKS Statement Headers: {:?}", table.headers);
            println!("RKS Statement First Row: {:?}", table.rows[0]);
            println!("Total Rows Extracted: {}", table.rows.len());
        } else {
            panic!(
                "Required PDF file 'RKS USER CHARGES bank.pdf' was not found in the root directory!"
            );
        }
    }

    #[test]
    fn test_standardize_date() {
        assert_eq!(standardize_date("30/04/2025"), "30-04-2025");
        assert_eq!(standardize_date("30/04/2025 10:15:30"), "30-04-2025");
        assert_eq!(standardize_date("01-Jan-2025"), "01-01-2025");
        assert_eq!(standardize_date("30/04/25"), "30-04-2025");
        assert_eq!(standardize_date("30-04-2025"), "30-04-2025");
        assert_eq!(standardize_date("30.04.25"), "30-04-2025");
        assert_eq!(standardize_date("30 Apr 2025"), "30-04-2025");
        assert_eq!(standardize_date("30 Apr 25"), "30-04-2025");
        assert_eq!(standardize_date("2025-04-30"), "30-04-2025");
        assert_eq!(standardize_date("not-a-date"), "not-a-date");
    }

    #[test]
    fn test_detect_column_guides() {
        let pdf_path = Path::new("static/sample_hdfc_decrypted.pdf");
        if pdf_path.exists() {
            let guides_res = detect_column_guides(pdf_path, None, 0.0, 1.0);
            assert!(
                guides_res.is_ok(),
                "Failed to auto-detect guides: {:?}",
                guides_res.err()
            );
            let guides = guides_res.unwrap();
            assert!(!guides.is_empty(), "Detected guides should not be empty");
            println!("Auto-detected column guides: {:?}", guides);
        }
    }

    #[test]
    fn test_detect_preset_from_file() {
        let pdf_path = Path::new("static/sample_hdfc_decrypted.pdf");
        if pdf_path.exists() {
            let preset_res = detect_preset_from_file(pdf_path, None);
            assert!(preset_res.is_ok());
            let preset = preset_res.unwrap();
            assert_eq!(preset, Some(BankPreset::Hdfc));
        }
    }

    #[test]
    fn test_invalid_config_validation() {
        // Mismatched mappings count (mappings: 2, guides: 2 -> mappings must be guides + 1 = 3)
        let config_res = ExtractionConfig::builder()
            .col_guides(vec![0.2, 0.5])
            .col_mappings(vec!["date".to_string(), "description".to_string()])
            .build();
        assert!(config_res.is_err());
        assert!(matches!(
            config_res.err().unwrap(),
            ExtractorError::InvalidConfig(_)
        ));

        // Bad y_top_trim range
        let config_res2 = ExtractionConfig::builder().y_top_trim(1.5).build();
        assert!(config_res2.is_err());

        // y_top_trim > y_bottom_trim
        let config_res3 = ExtractionConfig::builder()
            .y_top_trim(0.8)
            .y_bottom_trim(0.3)
            .build();
        assert!(config_res3.is_err());
    }
}
