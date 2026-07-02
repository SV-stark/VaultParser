use crate::error::ExtractorError;
use crate::models::ExtractedTable;
use rust_xlsxwriter::Workbook;

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

pub fn export_to_xlsx(table: &ExtractedTable) -> Result<Vec<u8>, ExtractorError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // Write headers
    for (col_idx, header) in table.headers.iter().enumerate() {
        worksheet
            .write_string(0, col_idx as u16, header)
            .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
    }

    // Write rows (only active indices)
    for (row_idx, r) in table.rows.iter().enumerate() {
        for (col_idx, &active_idx) in table.active_indices.iter().enumerate() {
            let cell_value = if active_idx < r.cells.len() {
                &r.cells[active_idx]
            } else {
                ""
            };
            worksheet
                .write_string((row_idx + 1) as u32, col_idx as u16, cell_value)
                .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))?;
        }
    }

    workbook
        .save_to_buffer()
        .map_err(|e| ExtractorError::XlsxWriteError(e.to_string()))
}
