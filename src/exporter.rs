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
pub fn export_to_csv(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    let mut wtr = csv::Writer::from_writer(Vec::new());

    // Write headers
    wtr.write_record(&table.headers)
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;

    // Write rows (only active indices)
    for r in &table.rows {
        let mut row_data = Vec::new();
        for &idx in &table.active_indices {
            if idx < r.cells.len() {
                row_data.push(r.cells[idx].clone());
            } else {
                row_data.push(String::new());
            }
        }
        wtr.write_record(row_data)
            .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;
    }

    wtr.into_inner()
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))
}

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};

fn parse_amount(val: &str) -> Option<f64> {
    let clean = val
        .replace([',', '₹'], "")
        .replace("Rs.", "")
        .replace("Rs", "")
        .replace("INR", "")
        .replace("CR", "")
        .replace("Cr", "")
        .replace("DR", "")
        .replace("Dr", "")
        .trim()
        .to_string();
    if clean.is_empty() {
        return None;
    }
    clean.parse::<f64>().ok()
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

    // Set column widths with dynamic padding
    for (col_idx, &width) in col_widths.iter().enumerate() {
        let final_width = (width + 4).clamp(12, 65) as f64;
        worksheet
            .set_column_width(col_idx as u16, final_width)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
    }

    worksheet.autofit();

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
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(Vec::new());

    // Write headers
    wtr.write_record(&table.headers)
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;

    // Write rows (only active indices)
    for r in &table.rows {
        let mut row_data = Vec::new();
        for &idx in &table.active_indices {
            if idx < r.cells.len() {
                row_data.push(r.cells[idx].clone());
            } else {
                row_data.push(String::new());
            }
        }
        wtr.write_record(row_data)
            .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))?;
    }

    wtr.into_inner()
        .map_err(|e| ExtractorError::CsvWriteError(e.to_string()))
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
                    "1,250.00".to_string(),
                    "10,500.50".to_string(),
                ],
            }],
        };
        let xlsx_bytes = export_to_xlsx(&table).unwrap();
        assert!(!xlsx_bytes.is_empty());
        assert_eq!(&xlsx_bytes[..2], b"PK");
    }
}
