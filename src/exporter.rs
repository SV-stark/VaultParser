use crate::error::ExtractorError;
use crate::models::ExtractedTable;

/// Aggregates per-column sums for the summary/totals row.
///
/// Returns `(is_numeric_column, sum, populated_cell_count)` for every active
/// column, in the same order as [`ExtractedTable::active_indices`]. Columns are
/// only treated as numeric when their header names a monetary column; running
/// balances are deliberately excluded because summing them is meaningless.
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

    // Keep only numeric/separator/sign characters. This robustly removes any
    // currency glyph, unit label (Rs, INR, Dr, Cr, ...) and thousands
    // separators-agnostic noise; the keyword stripping below still handles the
    // few alphabetic labels that may survive.
    // Keep alphanumerics and the separators we understand. Letters are
    // retained so that unit/currency labels (Rs., INR, Cr, Dr, ...) can be
    // stripped by the keyword pass below; unknown currency glyphs are
    // dropped. Commas and whitespace are dropped immediately.
    let mut filtered = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '0'..='9' | 'A'..='Z' | 'a'..='z' | '.' | '-' | '(' | ')' => filtered.push(c),
            _ => {}
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

    // Resolve Indian vs. European decimal conventions when both separators
    // appear: the decimal separator is whichever occurs last.
    let normalized_owned: String;
    let inner = if inner.contains(',') && inner.contains('.') {
        let comma_pos = inner.rfind(',').unwrap();
        let dot_pos = inner.rfind('.').unwrap();
        normalized_owned = if comma_pos > dot_pos {
            inner.replace('.', "").replace(',', ".")
        } else {
            inner.replace(',', "")
        };
        normalized_owned.trim()
    } else if inner.contains(',') {
        // A single trailing comma with 1-2 digits is treated as a decimal
        // separator; otherwise commas are thousands separators.
        let last = inner.rfind(',').unwrap();
        let frac_len = inner.len() - last - 1;
        normalized_owned = if (1..=2).contains(&frac_len) {
            inner.replace(',', ".")
        } else {
            inner.replace(',', "")
        };
        normalized_owned.trim()
    } else {
        inner
    };

    // Treat common zero-value spellings consistently.
    let lower = inner.to_ascii_lowercase();
    if lower == "nil" || lower == "n/a" || lower == "na" || lower == "-" {
        return Some(0.0);
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

/// Exports transaction records into Quicken Interchange Format (.qif) for financial apps.
pub fn export_to_qif(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    let mut out = String::from("!Type:Bank\n");

    let mut date_col: Option<usize> = None;
    let mut desc_col: Option<usize> = None;
    let mut debit_col: Option<usize> = None;
    let mut credit_col: Option<usize> = None;
    let mut ref_col: Option<usize> = None;

    for (col_idx, _) in table.active_indices.iter().enumerate() {
        let h = table
            .headers
            .get(col_idx)
            .map(|s| s.to_uppercase())
            .unwrap_or_default();
        if h.contains("DATE") && date_col.is_none() {
            date_col = Some(col_idx);
        } else if (h.contains("DESC") || h.contains("NARR") || h.contains("PARTICULAR"))
            && desc_col.is_none()
        {
            desc_col = Some(col_idx);
        } else if (h.contains("WITHDRAWAL") || h.contains("DEBIT") || h.contains("DR"))
            && !h.contains("BALANCE")
            && debit_col.is_none()
        {
            debit_col = Some(col_idx);
        } else if (h.contains("DEPOSIT") || h.contains("CREDIT") || h.contains("CR"))
            && !h.contains("BALANCE")
            && credit_col.is_none()
        {
            credit_col = Some(col_idx);
        } else if (h.contains("REF") || h.contains("CHQ") || h.contains("CHEQUE"))
            && ref_col.is_none()
        {
            ref_col = Some(col_idx);
        }
    }

    for r in &table.rows {
        // D: Date
        if let Some(d_idx) = date_col.and_then(|c| table.active_indices.get(c)) {
            if let Some(date_val) = r.cells.get(*d_idx).filter(|s| !s.trim().is_empty()) {
                out.push_str(&format!("D{}\n", date_val.trim()));
            }
        }

        // T: Amount (Debit is negative, Credit is positive)
        let debit = debit_col
            .and_then(|c| table.active_indices.get(c))
            .and_then(|&idx| r.cells.get(idx))
            .and_then(|s| parse_amount(s))
            .map(|v| v.abs());

        let credit = credit_col
            .and_then(|c| table.active_indices.get(c))
            .and_then(|&idx| r.cells.get(idx))
            .and_then(|s| parse_amount(s))
            .map(|v| v.abs());

        if let Some(cr) = credit {
            if cr > 0.0 {
                out.push_str(&format!("T{:.2}\n", cr));
            } else if let Some(db) = debit {
                out.push_str(&format!("T-{:.2}\n", db));
            }
        } else if let Some(db) = debit {
            out.push_str(&format!("T-{:.2}\n", db));
        }

        // P: Payee / Description
        if let Some(p_idx) = desc_col.and_then(|c| table.active_indices.get(c)) {
            if let Some(desc) = r.cells.get(*p_idx).filter(|s| !s.trim().is_empty()) {
                let clean_desc = desc.replace('\n', " ").replace('\r', " ");
                out.push_str(&format!("P{}\n", clean_desc.trim()));
            }
        }

        // N: Reference / Cheque
        if let Some(n_idx) = ref_col.and_then(|c| table.active_indices.get(c)) {
            if let Some(ref_val) = r.cells.get(*n_idx).filter(|s| !s.trim().is_empty()) {
                out.push_str(&format!("N{}\n", ref_val.trim()));
            }
        }

        out.push_str("^\n");
    }

    Ok(out.into_bytes())
}

/// Audits and reconciles consecutive running balances against withdrawals and deposits.
///
/// Automatically determines whether the statement is in chronological (earliest-first)
/// or reverse-chronological (latest-first) order. If discrepancies are found, they
/// are returned with expected vs reported differences.
#[must_use]
pub fn reconcile_running_balance(table: &ExtractedTable) -> crate::models::BalanceReconciliation {
    use crate::models::{BalanceDiscrepancy, BalanceReconciliation};

    if table.rows.len() < 2 {
        return BalanceReconciliation {
            total_evaluated: 0,
            matching_rows: 0,
            discrepancies: Vec::new(),
            is_reconciled: true,
        };
    }

    let mut withdrawal_idx: Option<usize> = None;
    let mut deposit_idx: Option<usize> = None;
    let mut balance_idx: Option<usize> = None;
    let mut date_idx: Option<usize> = None;

    for (col_idx, _active_idx) in table.active_indices.iter().enumerate() {
        let h = table
            .headers
            .get(col_idx)
            .map(|s| s.to_uppercase())
            .unwrap_or_default();

        if (h.contains("WITHDRAWAL") || h.contains("DEBIT") || h.contains("DR"))
            && !h.contains("BALANCE")
            && withdrawal_idx.is_none()
        {
            withdrawal_idx = Some(col_idx);
        } else if (h.contains("DEPOSIT") || h.contains("CREDIT") || h.contains("CR"))
            && !h.contains("BALANCE")
            && deposit_idx.is_none()
        {
            deposit_idx = Some(col_idx);
        } else if h.contains("BALANCE") && balance_idx.is_none() {
            balance_idx = Some(col_idx);
        } else if h.contains("DATE") && date_idx.is_none() {
            date_idx = Some(col_idx);
        }
    }

    let balance_col = match balance_idx {
        Some(idx) => idx,
        None => {
            return BalanceReconciliation {
                total_evaluated: 0,
                matching_rows: 0,
                discrepancies: Vec::new(),
                is_reconciled: true,
            };
        }
    };

    struct ParsedRow<'a> {
        orig_index: usize,
        row: &'a crate::models::PageRow,
        date: String,
        withdrawal: Option<f64>,
        deposit: Option<f64>,
        balance: f64,
    }

    let parsed_rows: Vec<ParsedRow> = table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(orig_index, r)| {
            let active_bal_idx = table.active_indices.get(balance_col).copied()?;
            let bal_str = r.cells.get(active_bal_idx)?;
            let balance = parse_amount(bal_str)?;

            let withdrawal = withdrawal_idx
                .and_then(|w_col| table.active_indices.get(w_col))
                .and_then(|&act_idx| r.cells.get(act_idx))
                .and_then(|c| parse_amount(c))
                .map(|v| v.abs());

            let deposit = deposit_idx
                .and_then(|d_col| table.active_indices.get(d_col))
                .and_then(|&act_idx| r.cells.get(act_idx))
                .and_then(|c| parse_amount(c))
                .map(|v| v.abs());

            let date = date_idx
                .and_then(|d_col| table.active_indices.get(d_col))
                .and_then(|&act_idx| r.cells.get(act_idx))
                .cloned()
                .unwrap_or_default();

            Some(ParsedRow {
                orig_index,
                row: r,
                date,
                withdrawal,
                deposit,
                balance,
            })
        })
        .collect();

    if parsed_rows.len() < 2 {
        return BalanceReconciliation {
            total_evaluated: 0,
            matching_rows: 0,
            discrepancies: Vec::new(),
            is_reconciled: true,
        };
    }

    let mut forward_matches = 0;
    let mut reverse_matches = 0;
    let sample_limit = (parsed_rows.len() - 1).min(10);

    for i in 1..=sample_limit {
        let prev = &parsed_rows[i - 1];
        let curr = &parsed_rows[i];

        let exp_fwd = prev.balance - curr.withdrawal.unwrap_or(0.0) + curr.deposit.unwrap_or(0.0);
        if (curr.balance - exp_fwd).abs() < 0.05 {
            forward_matches += 1;
        }

        let exp_rev = prev.balance + prev.withdrawal.unwrap_or(0.0) - prev.deposit.unwrap_or(0.0);
        if (curr.balance - exp_rev).abs() < 0.05 {
            reverse_matches += 1;
        }
    }

    let is_reverse = reverse_matches > forward_matches;

    let mut discrepancies = Vec::new();
    let mut matching_rows = 0;
    let total_evaluated = parsed_rows.len() - 1;

    for i in 1..parsed_rows.len() {
        let prev = &parsed_rows[i - 1];
        let curr = &parsed_rows[i];

        let expected = if is_reverse {
            prev.balance + prev.withdrawal.unwrap_or(0.0) - prev.deposit.unwrap_or(0.0)
        } else {
            prev.balance - curr.withdrawal.unwrap_or(0.0) + curr.deposit.unwrap_or(0.0)
        };

        let diff = curr.balance - expected;
        if diff.abs() <= 0.05 {
            matching_rows += 1;
        } else {
            discrepancies.push(BalanceDiscrepancy {
                row_index: curr.orig_index,
                row_id: curr.row.id.clone(),
                date: curr.date.clone(),
                previous_balance: prev.balance,
                withdrawal: curr.withdrawal,
                deposit: curr.deposit,
                reported_balance: curr.balance,
                expected_balance: expected,
                difference: (diff * 100.0).round() / 100.0,
            });
        }
    }

    let is_reconciled = discrepancies.is_empty();

    BalanceReconciliation {
        total_evaluated,
        matching_rows,
        discrepancies,
        is_reconciled,
    }
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

    #[test]
    fn test_export_to_qif() {
        let table = ExtractedTable {
            headers: vec![
                "DATE".to_string(),
                "DESCRIPTION".to_string(),
                "WITHDRAWAL".to_string(),
                "DEPOSIT".to_string(),
            ],
            active_indices: vec![0, 1, 2, 3],
            rows: vec![
                PageRow {
                    id: "row-0".to_string(),
                    y: 100.0,
                    page: 1,
                    cells: vec![
                        "01/01/2024".to_string(),
                        "Grocery Store".to_string(),
                        "150.00".to_string(),
                        "".to_string(),
                    ],
                },
                PageRow {
                    id: "row-1".to_string(),
                    y: 110.0,
                    page: 1,
                    cells: vec![
                        "02/01/2024".to_string(),
                        "Salary".to_string(),
                        "".to_string(),
                        "5000.00".to_string(),
                    ],
                },
            ],
        };

        let qif_bytes = export_to_qif(&table).unwrap();
        let qif_str = String::from_utf8(qif_bytes).unwrap();
        assert!(qif_str.starts_with("!Type:Bank"));
        assert!(qif_str.contains("D01/01/2024"));
        assert!(qif_str.contains("T-150.00"));
        assert!(qif_str.contains("PGrocery Store"));
        assert!(qif_str.contains("D02/01/2024"));
        assert!(qif_str.contains("T5000.00"));
        assert!(qif_str.contains("PSalary"));
    }

    #[test]
    fn test_reconcile_running_balance() {
        let table = ExtractedTable {
            headers: vec![
                "DATE".to_string(),
                "WITHDRAWAL".to_string(),
                "DEPOSIT".to_string(),
                "BALANCE".to_string(),
            ],
            active_indices: vec![0, 1, 2, 3],
            rows: vec![
                PageRow {
                    id: "r1".to_string(),
                    y: 100.0,
                    page: 1,
                    cells: vec![
                        "01-01-2024".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "10,000.00".to_string(),
                    ],
                },
                PageRow {
                    id: "r2".to_string(),
                    y: 110.0,
                    page: 1,
                    cells: vec![
                        "02-01-2024".to_string(),
                        "2,000.00".to_string(),
                        "".to_string(),
                        "8,000.00".to_string(),
                    ],
                },
                PageRow {
                    id: "r3".to_string(),
                    y: 120.0,
                    page: 1,
                    cells: vec![
                        "03-01-2024".to_string(),
                        "".to_string(),
                        "5,000.00".to_string(),
                        "13,000.00".to_string(),
                    ],
                },
                PageRow {
                    id: "r4".to_string(),
                    y: 130.0,
                    page: 1,
                    cells: vec![
                        "04-01-2024".to_string(),
                        "1,000.00".to_string(),
                        "".to_string(),
                        "11,500.00".to_string(), // Error! Expected 12,000.00
                    ],
                },
            ],
        };

        let rec = reconcile_running_balance(&table);
        assert_eq!(rec.total_evaluated, 3);
        assert_eq!(rec.matching_rows, 2);
        assert_eq!(rec.discrepancies.len(), 1);
        assert!(!rec.is_reconciled);
        assert_eq!(rec.discrepancies[0].row_index, 3);
        assert_eq!(rec.discrepancies[0].expected_balance, 12000.0);
        assert_eq!(rec.discrepancies[0].reported_balance, 11500.0);
    }
}
