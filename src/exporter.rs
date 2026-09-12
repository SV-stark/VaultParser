use crate::error::ExtractorError;
use crate::models::ExtractedTable;

/// Converts an [`ExtractedTable`] into a UTF-8 CSV formatted byte vector.
///
/// Only non-skipped columns (based on active indices) are exported.
///
/// # Examples
/// ```
/// use vaultparser::{ExtractedTable, PageRow, exporter};
///
/// let table = ExtractedTable {
///     headers: vec!["DATE".to_string(), "DESCRIPTION".to_string()],
///     active_indices: vec![0, 1],
///     rows: vec![PageRow {
///         id: "row-0".to_string(),
///         y: 100.0,
///         page: 1,
///         cells: vec!["01/02/2023".to_string(), "Test".to_string()],
///     }],
/// };
/// let csv = exporter::export_to_csv(&table).unwrap();
/// let text = String::from_utf8(csv).unwrap();
/// assert!(text.contains("DATE,DESCRIPTION"));
/// assert!(text.contains("01/02/2023,Test"));
/// ```
fn compute_column_totals(table: &ExtractedTable) -> Vec<(bool, f64, usize)> {
    let num_cols = table.active_indices.len();
    let mut is_num = vec![false; num_cols];
    let mut sums = vec![0.0; num_cols];
    let mut counts = vec![0; num_cols];

    for (col_idx, _active_idx) in table.active_indices.iter().enumerate() {
        if col_idx == 0 {
            continue;
        }
        let header_upper = table
            .headers
            .get(col_idx)
            .map(|h| h.to_uppercase())
            .unwrap_or_default();

        is_num[col_idx] = header_upper.contains("DEBIT")
            || header_upper.contains("CREDIT")
            || header_upper.contains("AMOUNT")
            || header_upper.contains("WITHDRAWAL")
            || header_upper.contains("DEPOSIT");
    }

    for r in &table.rows {
        for (col_idx, &active_idx) in table.active_indices.iter().enumerate() {
            if is_num[col_idx]
                && let Some(val) = r.cells.get(active_idx).and_then(|c| parse_amount(c))
            {
                sums[col_idx] += val;
                counts[col_idx] += 1;
            }
        }
    }

    let mut result = Vec::with_capacity(num_cols);
    for col_idx in 0..num_cols {
        result.push((is_num[col_idx], sums[col_idx], counts[col_idx]));
    }
    result
}

fn export_delimited(table: &ExtractedTable, delimiter: u8) -> Result<Vec<u8>, ExtractorError> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    // Write headers
    wtr.write_record(&table.headers)
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;

    // Write rows (only active indices)
    let mut row_data = Vec::with_capacity(table.active_indices.len());
    for r in &table.rows {
        row_data.clear();
        for &idx in &table.active_indices {
            if idx < r.cells.len() {
                row_data.push(r.cells[idx].as_str());
            } else {
                row_data.push("");
            }
        }
        wtr.write_record(&row_data)
            .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;
    }

    // Append summary row if table has rows
    if !table.rows.is_empty() {
        let totals = compute_column_totals(table);
        let mut summary_row_data = Vec::with_capacity(table.active_indices.len());
        for (col_idx, &(is_number_col, sum, count)) in totals.iter().enumerate() {
            if col_idx == 0 {
                summary_row_data.push(format!("TOTALS ({} rows)", table.rows.len()));
            } else if is_number_col && count > 0 {
                summary_row_data.push(format!("{:.2}", sum));
            } else {
                summary_row_data.push(String::new());
            }
        }
        wtr.write_record(summary_row_data)
            .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;
    }

    wtr.into_inner()
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))
}

pub fn export_to_csv(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    export_delimited(table, b',')
}

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};

/// Parses a monetary amount string with support for currency symbols,
/// comma grouping, accounting parentheses, and trailing negative signs.
#[must_use]
pub fn parse_amount(val: &str) -> Option<f64> {
    let clean = val.trim();
    if clean.is_empty() {
        return None;
    }

    let is_parenthesized_outer = clean.starts_with('(') && clean.ends_with(')');
    let s = if is_parenthesized_outer {
        clean[1..clean.len() - 1].trim()
    } else {
        clean
    };

    // Filter out currency symbols, commas, and parenthetical markers into a single buffer
    let mut filtered = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '$' | '£' | '€' | '₹' | ',' => {}
            _ => filtered.push(c),
        }
    }

    let mut inner = filtered.trim();
    let is_parenthesized_inner = inner.starts_with('(') && inner.ends_with(')');
    if is_parenthesized_inner {
        inner = inner[1..inner.len() - 1].trim();
    }

    let is_negative = is_parenthesized_outer || is_parenthesized_inner;

    // Strip case-insensitive keyword tokens (Rs., INR, Dr., Cr., etc.)
    const KEYWORDS: &[&str] = &["rs.", "rs", "inr", "cr.", "cr", "dr.", "dr"];
    let mut changed = true;
    while changed {
        changed = false;
        let lower = inner.to_ascii_lowercase();
        for &kw in KEYWORDS {
            if lower.starts_with(kw) {
                inner = inner[kw.len()..].trim();
                changed = true;
                break;
            } else if lower.ends_with(kw) {
                inner = inner[..inner.len() - kw.len()].trim();
                changed = true;
                break;
            }
        }
    }

    if inner.is_empty() {
        return None;
    }

    let mut trailing_minus = false;
    if let Some(stripped) = inner.strip_suffix('-') {
        trailing_minus = true;
        inner = stripped.trim();
    } else if let Some(stripped) = inner.strip_prefix('-') {
        inner = stripped.trim();
        trailing_minus = true;
    }

    let num = inner.parse::<f64>().ok()?;
    if is_negative || trailing_minus {
        Some(-num.abs())
    } else {
        Some(num)
    }
}

/// Converts an [`ExtractedTable`] into an Excel workbook file (`.xlsx`) byte vector.
///
/// Exports formatted tables with dark header styling, zebra striping, currency/numeric
/// formatting, freeze header panes, and auto-adjusted column widths.
///
/// # Examples
/// ```
/// use vaultparser::{ExtractedTable, PageRow, exporter};
///
/// let table = ExtractedTable {
///     headers: vec!["DATE".to_string(), "DESCRIPTION".to_string()],
///     active_indices: vec![0, 1],
///     rows: vec![PageRow {
///         id: "row-0".to_string(),
///         y: 100.0,
///         page: 1,
///         cells: vec!["01/02/2023".to_string(), "Test".to_string()],
///     }],
/// };
/// let xlsx = exporter::export_to_xlsx(&table).unwrap();
/// assert!(!xlsx.is_empty());
/// // A valid XLSX is a ZIP (PK) archive.
/// assert_eq!(&xlsx[..2], b"PK");
/// ```
pub fn export_to_xlsx(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name("Transactions")
        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;

    // Freeze header row
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;

    // Colors
    let header_bg = Color::RGB(0x1E293B); // Dark slate
    let zebra_bg = Color::RGB(0xF8FAFC); // Very light gray/blue
    let border_color = Color::RGB(0xE2E8F0); // Light gray border

    // Header Format
    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(header_bg)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);

    // Formats for regular (even) rows
    let text_format = Format::new()
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    let center_format = Format::new()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    let num_format = Format::new()
        .set_num_format("#,##0.00")
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    // Formats for zebra (odd) rows
    let text_zebra_format = Format::new()
        .set_background_color(zebra_bg)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    let center_zebra_format = Format::new()
        .set_background_color(zebra_bg)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    let num_zebra_format = Format::new()
        .set_background_color(zebra_bg)
        .set_num_format("#,##0.00")
        .set_align(FormatAlign::Right)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(border_color);

    // Write headers
    worksheet
        .set_row_height(0, 26.0)
        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
    let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();

    for (col_idx, header) in table.headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col_idx as u16, header, &header_format)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
    }

    // Write rows (only active indices)
    for (row_idx, r) in table.rows.iter().enumerate() {
        let excel_row = (row_idx + 1) as u32;
        worksheet
            .set_row_height(excel_row, 20.0)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;

        let is_zebra = row_idx % 2 == 1;

        for (col_idx, &active_idx) in table.active_indices.iter().enumerate() {
            let header_upper = table
                .headers
                .get(col_idx)
                .map(|h| h.to_uppercase())
                .unwrap_or_default();

            let cell_value = if active_idx < r.cells.len() {
                &r.cells[active_idx]
            } else {
                ""
            };

            // Update column width calculation
            if col_idx < col_widths.len() && cell_value.len() > col_widths[col_idx] {
                col_widths[col_idx] = cell_value.len();
            }

            let is_number_col = header_upper.contains("DEBIT")
                || header_upper.contains("CREDIT")
                || header_upper.contains("BALANCE")
                || header_upper.contains("AMOUNT")
                || header_upper.contains("WITHDRAWAL")
                || header_upper.contains("DEPOSIT");

            let is_center_col = header_upper.contains("DATE")
                || header_upper.contains("CHQ")
                || header_upper.contains("REF")
                || header_upper.contains("TXN")
                || header_upper.contains("PAGE")
                || header_upper.contains("S_NO");

            if is_number_col {
                if let Some(num) = parse_amount(cell_value) {
                    let fmt = if is_zebra {
                        &num_zebra_format
                    } else {
                        &num_format
                    };
                    worksheet
                        .write_number_with_format(excel_row, col_idx as u16, num, fmt)
                        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
                } else {
                    let fmt = if is_zebra {
                        &text_zebra_format
                    } else {
                        &text_format
                    };
                    worksheet
                        .write_string_with_format(excel_row, col_idx as u16, cell_value, fmt)
                        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
                }
            } else if is_center_col {
                let fmt = if is_zebra {
                    &center_zebra_format
                } else {
                    &center_format
                };
                worksheet
                    .write_string_with_format(excel_row, col_idx as u16, cell_value, fmt)
                    .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
            } else {
                let fmt = if is_zebra {
                    &text_zebra_format
                } else {
                    &text_format
                };
                worksheet
                    .write_string_with_format(excel_row, col_idx as u16, cell_value, fmt)
                    .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
            }
        }
    }

    // Write Summary & Totals Row if rows exist
    if !table.rows.is_empty() {
        let summary_row = (table.rows.len() + 1) as u32;
        worksheet
            .set_row_height(summary_row, 24.0)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;

        let summary_bg = Color::RGB(0xCBD5E1); // Slate gray fill
        let summary_label_fmt = Format::new()
            .set_bold()
            .set_background_color(summary_bg)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border_top(FormatBorder::Thin)
            .set_border_bottom(FormatBorder::Double)
            .set_border_color(Color::RGB(0x64748B));

        let summary_num_fmt = Format::new()
            .set_bold()
            .set_num_format("#,##0.00")
            .set_background_color(summary_bg)
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border_top(FormatBorder::Thin)
            .set_border_bottom(FormatBorder::Double)
            .set_border_color(Color::RGB(0x64748B));

        let total_label = format!("TOTALS ({} rows)", table.rows.len());
        if !col_widths.is_empty() {
            col_widths[0] = col_widths[0].max(total_label.len());
        }
        worksheet
            .write_string_with_format(summary_row, 0, &total_label, &summary_label_fmt)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;

        let totals = compute_column_totals(table);
        for (col_idx, &(is_number_col, sum, count)) in totals.iter().enumerate() {
            if col_idx == 0 {
                continue;
            }
            if is_number_col && count > 0 {
                worksheet
                    .write_number_with_format(summary_row, col_idx as u16, sum, &summary_num_fmt)
                    .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
            } else {
                worksheet
                    .write_string_with_format(summary_row, col_idx as u16, "", &summary_label_fmt)
                    .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
            }
        }
    }

    // Set column widths with dynamic padding
    for (col_idx, &width) in col_widths.iter().enumerate() {
        let final_width = (width + 4).clamp(12, 65) as f64;
        worksheet
            .set_column_width(col_idx as u16, final_width)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
    }

    workbook
        .save_to_buffer()
        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))
}

/// Converts an [`ExtractedTable`] into a pretty JSON formatted byte vector.
///
/// # Examples
/// ```
/// use vaultparser::{ExtractedTable, PageRow, exporter};
///
/// let table = ExtractedTable {
///     headers: vec!["DATE".to_string()],
///     active_indices: vec![0],
///     rows: vec![PageRow {
///         id: "row-0".to_string(),
///         y: 100.0,
///         page: 1,
///         cells: vec!["01/02/2023".to_string()],
///     }],
/// };
/// let json = exporter::export_to_json(&table).unwrap();
/// let text = String::from_utf8(json).unwrap();
/// assert!(text.contains("\"DATE\""));
/// assert!(text.contains("01/02/2023"));
/// ```
pub fn export_to_json(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    serde_json::to_vec_pretty(table).map_err(Into::into)
}

/// Converts an [`ExtractedTable`] into a UTF-8 TSV (Tab-Separated Values) formatted byte vector.
///
/// Only non-skipped columns (based on active indices) are exported.
///
/// # Examples
/// ```
/// use vaultparser::{ExtractedTable, PageRow, exporter};
///
/// let table = ExtractedTable {
///     headers: vec!["DATE".to_string(), "DESCRIPTION".to_string()],
///     active_indices: vec![0, 1],
///     rows: vec![PageRow {
///         id: "row-0".to_string(),
///         y: 100.0,
///         page: 1,
///         cells: vec!["01/02/2023".to_string(), "Test".to_string()],
///     }],
/// };
/// let tsv = exporter::export_to_tsv(&table).unwrap();
/// let text = String::from_utf8(tsv).unwrap();
/// assert!(text.contains("DATE\tDESCRIPTION"));
/// assert!(text.contains("01/02/2023\tTest"));
/// ```
pub fn export_to_tsv(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    export_delimited(table, b'\t')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PageRow;

    #[test]
    fn test_export_to_tsv() {
        let table = ExtractedTable {
            headers: vec!["DATE".to_string(), "AMOUNT".to_string()],
            active_indices: vec![0, 1],
            rows: vec![PageRow {
                id: "row-0".to_string(),
                y: 100.0,
                page: 1,
                cells: vec!["2023-01-01".to_string(), "100.00".to_string()],
            }],
        };
        let tsv_bytes = export_to_tsv(&table).unwrap();
        let tsv_str = String::from_utf8(tsv_bytes).unwrap();
        assert!(tsv_str.contains("DATE\tAMOUNT"));
        assert!(tsv_str.contains("2023-01-01\t100.00"));
    }

    #[test]
    fn test_parse_amount_various_formats() {
        assert_eq!(parse_amount("1,234.50"), Some(1234.50));
        assert_eq!(parse_amount("(1,234.50)"), Some(-1234.50));
        assert_eq!(parse_amount("$(1,234.50)"), Some(-1234.50));
        assert_eq!(parse_amount("₹ (5,000.00)"), Some(-5000.00));
        assert_eq!(parse_amount("(₹ 5,000.00)"), Some(-5000.00));
        assert_eq!(parse_amount("1,234.50-"), Some(-1234.50));
        assert_eq!(parse_amount("₹ 5,000.00"), Some(5000.00));
        assert_eq!(parse_amount("$250.75"), Some(250.75));
        assert_eq!(parse_amount("€100.00"), Some(100.00));
        assert_eq!(parse_amount("500.00 Dr"), Some(500.00));
        assert_eq!(parse_amount("1,000.00 Cr"), Some(1000.00));
        assert_eq!(parse_amount("Rs. 1,500.00"), Some(1500.00));
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("invalid"), None);
    }

    #[test]
    fn test_export_to_xlsx_formatting() {
        let table = ExtractedTable {
            headers: vec![
                "DATE".to_string(),
                "DESCRIPTION".to_string(),
                "DEBIT".to_string(),
                "BALANCE".to_string(),
            ],
            active_indices: vec![0, 1, 2, 3],
            rows: vec![PageRow {
                id: "row-0".to_string(),
                y: 100.0,
                page: 1,
                cells: vec![
                    "01-01-2023".to_string(),
                    "Salary Deposit".to_string(),
                    "(1,250.00)".to_string(),
                    "₹ 10,500.50".to_string(),
                ],
            }],
        };
        let xlsx_bytes = export_to_xlsx(&table).unwrap();
        assert!(!xlsx_bytes.is_empty());
        assert_eq!(&xlsx_bytes[..2], b"PK");
    }
}
