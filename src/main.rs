use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use std::collections::HashMap;
use std::fs;
use tower_http::services::ServeDir;

use vaultparser::{
    BankPreset, ExtractionConfig, detect_column_guides, detect_preset_from_file,
    exporter::{export_to_csv, export_to_tsv, export_to_xlsx},
    extract_from_bytes,
};

async fn get_presets() -> impl IntoResponse {
    let mut presets = Vec::new();
    for preset in BankPreset::all() {
        let cfg = preset.config();
        presets.push(serde_json::json!({
            "key": preset.key(),
            "name": preset.name(),
            "guides": cfg.col_guides,
            "mappings": cfg.col_mappings,
        }));
    }
    Json(presets)
}

async fn detect_pdf(mut multipart: Multipart) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut file_bytes = Vec::new();
    let mut password = None;
    let mut y_top_trim = 0.0;
    let mut y_bottom_trim = 1.0;

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
            "password" => {
                let p = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                if !p.trim().is_empty() {
                    password = Some(p);
                }
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
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No file uploaded".to_string()));
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_path =
        std::env::temp_dir().join(format!("vp_detect_{}_{}.pdf", std::process::id(), ts));
    let (preset_key, preset_name, guides) =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            struct TempFileGuard(std::path::PathBuf);
            impl Drop for TempFileGuard {
                fn drop(&mut self) {
                    let _ = fs::remove_file(&self.0);
                }
            }
            fs::write(&temp_path, &file_bytes).map_err(|e| e.to_string())?;
            let _guard = TempFileGuard(temp_path.clone());

            let preset_opt = detect_preset_from_file(&temp_path, password.as_deref())
                .ok()
                .flatten();
            let (p_key, p_name) = match preset_opt {
                Some(preset) => (
                    Some(preset.key().to_string()),
                    Some(preset.name().to_string()),
                ),
                None => (None, None),
            };
            let guides =
                detect_column_guides(&temp_path, password.as_deref(), y_top_trim, y_bottom_trim)
                    .unwrap_or_default();
            Ok((p_key, p_name, guides))
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let response = serde_json::json!({
        "preset": preset_key,
        "preset_name": preset_name,
        "guides": guides
    });

    Ok(Json(response).into_response())
}

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
    let mut categorize = false;
    let mut from_date = None;
    let mut to_date = None;

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
            "categorize" => {
                let c = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                categorize = c.parse::<bool>().unwrap_or(false);
            }
            "from_date" => {
                let d = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                if !d.trim().is_empty() {
                    from_date = Some(d.trim().to_string());
                }
            }
            "to_date" => {
                let d = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                if !d.trim().is_empty() {
                    to_date = Some(d.trim().to_string());
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
        .categorize(categorize)
        .from_date(from_date)
        .to_date(to_date)
        .build()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid config: {}", e)))?;

    let format_type_clone = format_type.clone();
    let (extracted_table, export_payload) = tokio::task::spawn_blocking(move || {
        let extracted_table = extract_from_bytes(&file_bytes, &config)
            .map_err(|e| format!("Extraction failed: {}", e))?;

        if format_type_clone == "xlsx" {
            let xlsx_bytes = export_to_xlsx(&extracted_table)
                .map_err(|e| format!("Excel export failed: {}", e))?;
            Ok((extracted_table, Some(("xlsx", xlsx_bytes))))
        } else if format_type_clone == "csv" {
            let csv_data =
                export_to_csv(&extracted_table).map_err(|e| format!("CSV export failed: {}", e))?;
            Ok((extracted_table, Some(("csv", csv_data))))
        } else if format_type_clone == "tsv" {
            let tsv_data =
                export_to_tsv(&extracted_table).map_err(|e| format!("TSV export failed: {}", e))?;
            Ok((extracted_table, Some(("tsv", tsv_data))))
        } else {
            Ok((extracted_table, None))
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some((kind, bytes)) = export_payload {
        let mut res_headers = HeaderMap::new();
        match kind {
            "xlsx" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static(
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    ),
                );
                res_headers.insert(
                    header::CONTENT_DISPOSITION,
                    axum::http::HeaderValue::from_static(
                        "attachment; filename=converted_statement.xlsx",
                    ),
                );
            }
            "csv" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/csv"),
                );
                res_headers.insert(
                    header::CONTENT_DISPOSITION,
                    axum::http::HeaderValue::from_static(
                        "attachment; filename=converted_statement.csv",
                    ),
                );
            }
            "tsv" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/tab-separated-values"),
                );
                res_headers.insert(
                    header::CONTENT_DISPOSITION,
                    axum::http::HeaderValue::from_static(
                        "attachment; filename=converted_statement.tsv",
                    ),
                );
            }
            _ => {}
        }
        Ok((StatusCode::OK, res_headers, bytes).into_response())
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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vaultparser=info,tower_http=info,axum=info".into()),
        )
        .init();

    // Clean up temp directory on startup if it exists
    if let Ok(entries) = fs::read_dir("temp") {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let app = Router::new()
        .route("/api/convert", post(convert_pdf))
        .route("/api/detect", post(detect_pdf))
        .route("/api/presets", get(get_presets))
        .fallback_service(ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server running on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
