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
    y: f64,
    items: Vec<WordItem>,
}

fn is_possible_date(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let cleaned = val.trim();
    if cleaned.len() > 15 {
        return false;
    }
    cleaned.chars().any(|c| c.is_ascii_digit())
}

fn is_possible_amount(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let cleaned = val
        .replace('$', "")
        .replace('£', "")
        .replace('€', "")
        .replace(',', "")
        .trim()
        .to_string();
    cleaned.parse::<f64>().is_ok()
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
        let decrypted_path = temp_path.with_extension("decrypted.pdf");
        let pwd = password.unwrap_or("");
        
        info!("PDF is encrypted or layout is unreadable. Attempting native decryption for '{}'...", temp_path.display());

        // Native lopdf decryption using load_with_password (handles empty password AES-256)
        let mut doc = lopdf::Document::load_with_password(temp_path, pwd).map_err(|e| {
            error!("LOPDF decryption failed for PDF '{}': {:?}", temp_path.display(), e);
            ExtractorError::DecryptError(format!("Failed to load/decrypt PDF with lopdf: {:?}", e))
        })?;

        if doc.is_encrypted() {
            doc.trailer.remove(b"Encrypt");
        }

        doc.save(&decrypted_path).map_err(|e| {
            error!("Failed to save decrypted PDF copy: {:?}", e);
            ExtractorError::DecryptError(format!("Failed to save decrypted PDF: {:?}", e))
        })?;
        
        info!("PDF successfully decrypted and saved to '{}'.", decrypted_path.display());
        Ok(decrypted_path)
    } else {
        debug!("PDF '{}' is already unencrypted and readable directly.", temp_path.display());
        Ok(temp_path.to_path_buf())
    }
}

/// Automatically detect column guides by analyzing the horizontal density of text spans
pub fn detect_column_guides<P: AsRef<Path>>(
    pdf_path: P,
    password: Option<&str>,
    y_top_trim: f64,
    y_bottom_trim: f64,
) -> Result<Vec<f64>, ExtractorError> {
    let path_ref = pdf_path.as_ref();
    info!("Starting layout analysis to auto-detect columns for '{}'...", path_ref.display());
    
    let processed_path = decrypt_pdf_if_needed(path_ref, password)?;

    let pdf = PdfDocument::open(&processed_path)
        .map_err(|e| {
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
    debug!("Analyzing first {} pages for horizontal text distribution...", pages_count);

    for page in sample_pages {
        let width = page.width as f64;
        let height = page.height as f64;
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

    info!("Layout analysis complete. Auto-detected {} column delimiters: {:?}", guides.len(), guides);
    Ok(guides)
}

/// Primary function to extract tabular data from a PDF file path
pub fn extract_from_file<P: AsRef<Path>>(
    pdf_path: P,
    config: &ExtractionConfig,
) -> Result<ExtractedTable, ExtractorError> {
    let path_ref = pdf_path.as_ref();
    info!("Starting PDF table extraction for '{}'...", path_ref.display());
    
    let processed_path = decrypt_pdf_if_needed(path_ref, config.password.as_deref())?;

    let pdf = PdfDocument::open(&processed_path)
        .map_err(|e| {
            error!("Failed to open PDF document: {:?}", e);
            ExtractorError::PdfOpenError(format!("{:?}", e))
        })?;

    let mut all_rows = Vec::new();
    let pages = pdf.pages();

    for (page_idx, page) in pages.iter().enumerate() {
        let width = page.width as f64;
        let height = page.height as f64;
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
                if (r.y - item.y).abs() <= config.y_tolerance {
                    found_row_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = found_row_idx {
                grouped_rows[idx].items.push(item);
                let total_y: f64 = grouped_rows[idx].items.iter().map(|it| it.y).sum();
                grouped_rows[idx].y = total_y / grouped_rows[idx].items.len() as f64;
            } else {
                grouped_rows.push(GroupedRow {
                    y: item.y,
                    items: vec![item],
                });
            }
        }

        for r in &mut grouped_rows {
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
                        if let Ok(col_idx) = col_idx_str.parse::<usize>() {
                            if col_idx < row.cells.len() {
                                row.cells[col_idx] = val.clone();
                            }
                        }
                    }
                }
            }
        }

        let desc_idx = config.col_mappings.iter().position(|m| m == "description");
        let date_idx = config.col_mappings.iter().position(|m| m == "date");
        let amount_idx = config.col_mappings.iter().position(|m| m == "amount");
        let debit_idx = config.col_mappings.iter().position(|m| m == "debit");
        let credit_idx = config.col_mappings.iter().position(|m| m == "credit");
        let balance_idx = config.col_mappings.iter().position(|m| m == "balance");

        if config.merge_multi_line {
            if let Some(desc_idx) = desc_idx {
                let mut merged = Vec::new();
                for row in page_rows {
                    let cell_desc = &row.cells[desc_idx];
                    let is_date_empty = date_idx.map_or(true, |idx| row.cells[idx].is_empty());
                    let is_amount_empty = amount_idx.map_or(true, |idx| row.cells[idx].is_empty());
                    let is_debit_empty = debit_idx.map_or(true, |idx| row.cells[idx].is_empty());
                    let is_credit_empty = credit_idx.map_or(true, |idx| row.cells[idx].is_empty());
                    let is_balance_empty =
                        balance_idx.map_or(true, |idx| row.cells[idx].is_empty());

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
            if config.filter_only_date {
                if let Some(date_idx) = date_idx {
                    if !is_possible_date(&row.cells[date_idx]) {
                        keep = false;
                    }
                }
            }
            if config.filter_only_amount {
                let mut has_val = false;
                if let Some(amount_idx) = amount_idx {
                    if is_possible_amount(&row.cells[amount_idx]) {
                        has_val = true;
                    }
                }
                if let Some(debit_idx) = debit_idx {
                    if is_possible_amount(&row.cells[debit_idx]) {
                        has_val = true;
                    }
                }
                if let Some(credit_idx) = credit_idx {
                    if is_possible_amount(&row.cells[credit_idx]) {
                        has_val = true;
                    }
                }
                if !has_val {
                    keep = false;
                }
            }
            if keep {
                filtered_rows.push(row);
            }
        }

        all_rows.extend(filtered_rows);
    }

    // Clean up decrypted file if it was created temporarily
    if processed_path != path_ref {
        let _ = fs::remove_file(&processed_path);
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
    
    info!("Successfully extracted {} total transaction rows.", all_rows.len());

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

    let result = extract_from_file(&temp_path, config);
    let _ = fs::remove_file(&temp_path);
    result
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
        assert!(!is_possible_date(""));
        assert!(!is_possible_date("Not a date"));
    }

    #[test]
    fn test_is_possible_amount() {
        assert!(is_possible_amount("123.45"));
        assert!(is_possible_amount("$1,234.50"));
        assert!(is_possible_amount("€100"));
        assert!(is_possible_amount("-50.00"));
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
}
