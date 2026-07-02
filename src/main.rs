use axum::{
    Json, Router,
    extract::Multipart,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use std::collections::HashMap;
use std::fs;
use tower_http::services::ServeDir;

use vaultparser::{
    ExtractionConfig,
    exporter::{export_to_csv, export_to_xlsx},
    extract_from_bytes,
};

async fn convert_pdf(mut multipart: Multipart) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut file_bytes = Vec::new();
    let mut col_guides = String::new();
    let mut col_mappings = String::new();
    let mut y_tolerance = 6.0;
    let mut merge_multi_line = true;
    let mut skip_header_rows = 0;
    let mut skip_footer_rows = 0;
    let mut filter_only_date = true;
    let mut filter_only_amount = false;
    let mut format_type = String::from("json");
    let mut manual_edits = String::from("{}");
    let mut deleted_rows = String::from("{}");
    let mut y_top_trim = 0.0;
    let mut y_bottom_trim = 1.0;
    let mut password = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_bytes = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec();
            }
            "col_guides" => {
                col_guides = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            "col_mappings" => {
                col_mappings = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            "y_tolerance" => {
                let t = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                y_tolerance = t.parse::<f64>().unwrap_or(6.0);
            }
            "merge_multi_line" => {
                let m = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                merge_multi_line = m.parse::<bool>().unwrap_or(true);
            }
            "skip_header_rows" => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                skip_header_rows = s.parse::<usize>().unwrap_or(0);
            }
            "skip_footer_rows" => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                skip_footer_rows = s.parse::<usize>().unwrap_or(0);
            }
            "filter_only_date" => {
                let f = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                filter_only_date = f.parse::<bool>().unwrap_or(true);
            }
            "filter_only_amount" => {
                let f = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                filter_only_amount = f.parse::<bool>().unwrap_or(false);
            }
            "format" => {
                format_type = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            "manual_edits" => {
                manual_edits = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            "deleted_rows" => {
                deleted_rows = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            "y_top_trim" => {
                let y = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                y_top_trim = y.parse::<f64>().unwrap_or(0.0);
            }
            "y_bottom_trim" => {
                let y = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                y_bottom_trim = y.parse::<f64>().unwrap_or(1.0);
            }
            "password" => {
                let p = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                if !p.trim().is_empty() {
                    password = Some(p);
                }
            }
            _ => {}
        }
    }

    let guides: Vec<f64> = serde_json::from_str(&col_guides).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid guides JSON: {}", e),
        )
    })?;
    let mappings: Vec<String> = serde_json::from_str(&col_mappings).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid mappings JSON: {}", e),
        )
    })?;
    let edits_data: HashMap<String, HashMap<String, HashMap<String, String>>> =
        serde_json::from_str(&manual_edits).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid edits JSON: {}", e),
            )
        })?;
    let deletes_data: HashMap<String, HashMap<String, bool>> = serde_json::from_str(&deleted_rows)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid deletes JSON: {}", e),
            )
        })?;

    let config = ExtractionConfig::builder()
        .col_guides(guides)
        .col_mappings(mappings)
        .y_tolerance(y_tolerance)
        .merge_multi_line(merge_multi_line)
        .skip_header_rows(skip_header_rows)
        .skip_footer_rows(skip_footer_rows)
        .filter_only_date(filter_only_date)
        .filter_only_amount(filter_only_amount)
        .y_top_trim(y_top_trim)
        .y_bottom_trim(y_bottom_trim)
        .manual_edits(edits_data)
        .deleted_rows(deletes_data)
        .password(password)
        .build();

    let extracted_table = extract_from_bytes(&file_bytes, &config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Extraction failed: {}", e),
        )
    })?;

    if format_type == "xlsx" {
        let xlsx_bytes = export_to_xlsx(&extracted_table).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Excel export failed: {}", e),
            )
        })?;

        let mut res_headers = HeaderMap::new();
        res_headers.insert(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                .parse()
                .unwrap(),
        );
        res_headers.insert(
            header::CONTENT_DISPOSITION,
            "attachment; filename=converted_statement.xlsx"
                .parse()
                .unwrap(),
        );

        Ok((StatusCode::OK, res_headers, xlsx_bytes).into_response())
    } else if format_type == "csv" {
        let csv_data = export_to_csv(&extracted_table).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("CSV export failed: {}", e),
            )
        })?;

        let mut res_headers = HeaderMap::new();
        res_headers.insert(header::CONTENT_TYPE, "text/csv".parse().unwrap());
        res_headers.insert(
            header::CONTENT_DISPOSITION,
            "attachment; filename=converted_statement.csv"
                .parse()
                .unwrap(),
        );

        Ok((StatusCode::OK, res_headers, csv_data).into_response())
    } else {
        let json_response = serde_json::json!({
            "headers": extracted_table.headers,
            "active_indices": extracted_table.active_indices,
            "rows": extracted_table.rows
        });
        Ok(Json(json_response).into_response())
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vaultparser=info,tower_http=info,axum=info".into()),
        )
        .init();

    // Clean up temp directory on startup
    if let Ok(entries) = fs::read_dir("temp") {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let app = Router::new()
        .route("/api/convert", post(convert_pdf))
        .fallback_service(ServeDir::new("static"))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    tracing::info!("Server running on http://127.0.0.1:8000");
    axum::serve(listener, app).await.unwrap();
}
