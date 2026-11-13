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
    BankPreset, ExtractionConfig, detect_layout_from_bytes,
    exporter::{
        export_to_csv, export_to_json, export_to_qif, export_to_tsv, export_to_xlsx,
        reconcile_running_balance,
    },
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
            "y_tolerance": cfg.y_tolerance,
            "y_top_trim": cfg.y_top_trim,
            "y_bottom_trim": cfg.y_bottom_trim,
            "merge_multi_line": cfg.merge_multi_line,
            "filter_only_date": cfg.filter_only_date,
            "filter_only_amount": cfg.filter_only_amount,
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
                match y.parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) && !v.is_nan() => y_top_trim = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid y_top_trim: must be a number in [0.0, 1.0]".to_string(),
                        ));
                    }
                }
            }
            "y_bottom_trim" => {
                let y = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                match y.parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) && !v.is_nan() => y_bottom_trim = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid y_bottom_trim: must be a number in [0.0, 1.0]".to_string(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No file uploaded".to_string()));
    }

    let layout = tokio::task::spawn_blocking(move || {
        detect_layout_from_bytes(&file_bytes, password.as_deref(), y_top_trim, y_bottom_trim)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let response = serde_json::json!({
        "preset": layout.preset_key,
        "preset_name": layout.preset_name,
        "guides": layout.guides,
        "mappings": layout.mappings,
        "y_tolerance": layout.y_tolerance,
        "y_top_trim": layout.y_top_trim,
        "y_bottom_trim": layout.y_bottom_trim,
    });

    Ok(Json(response).into_response())
}

async fn convert_pdf(mut multipart: Multipart) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut file_bytes = Vec::new();
    let mut upload_file_name: Option<String> = None;
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
                upload_file_name = field.file_name().map(|s| s.to_string());
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
                match t.parse::<f64>() {
                    Ok(v) if v > 0.0 && !v.is_nan() => y_tolerance = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid y_tolerance: must be a positive number".to_string(),
                        ));
                    }
                }
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
                match s.parse::<usize>() {
                    Ok(v) => skip_header_rows = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid skip_header_rows: must be a non-negative integer".to_string(),
                        ));
                    }
                }
            }
            "skip_footer_rows" => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                match s.parse::<usize>() {
                    Ok(v) => skip_footer_rows = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid skip_footer_rows: must be a non-negative integer".to_string(),
                        ));
                    }
                }
            }
            "filter_only_date" => {
                let f = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                match f.parse::<bool>() {
                    Ok(v) => filter_only_date = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid filter_only_date: must be true or false".to_string(),
                        ));
                    }
                }
            }
            "filter_only_amount" => {
                let f = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                match f.parse::<bool>() {
                    Ok(v) => filter_only_amount = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid filter_only_amount: must be true or false".to_string(),
                        ));
                    }
                }
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
                match y.parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) && !v.is_nan() => y_top_trim = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid y_top_trim: must be a number in [0.0, 1.0]".to_string(),
                        ));
                    }
                }
            }
            "y_bottom_trim" => {
                let y = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                match y.parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) && !v.is_nan() => y_bottom_trim = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid y_bottom_trim: must be a number in [0.0, 1.0]".to_string(),
                        ));
                    }
                }
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
                match c.parse::<bool>() {
                    Ok(v) => categorize = v,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Invalid categorize: must be true or false".to_string(),
                        ));
                    }
                }
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
        } else if format_type_clone == "qif" {
            let qif_data =
                export_to_qif(&extracted_table).map_err(|e| format!("QIF export failed: {}", e))?;
            Ok((extracted_table, Some(("qif", qif_data))))
        } else if format_type_clone == "json_export" {
            let json_data = export_to_json(&extracted_table)
                .map_err(|e| format!("JSON export failed: {}", e))?;
            Ok((extracted_table, Some(("json", json_data))))
        } else {
            Ok((extracted_table, None))
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some((kind, bytes)) = export_payload {
        let stem = upload_file_name
            .as_deref()
            .and_then(|n| std::path::Path::new(n).file_stem())
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("statement");
        let mut res_headers = HeaderMap::new();
        match kind {
            "xlsx" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static(
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    ),
                );
                let disp = format!("attachment; filename={}_converted.xlsx", stem);
                if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
                    res_headers.insert(header::CONTENT_DISPOSITION, v);
                }
            }
            "csv" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/csv"),
                );
                let disp = format!("attachment; filename={}_converted.csv", stem);
                if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
                    res_headers.insert(header::CONTENT_DISPOSITION, v);
                }
            }
            "tsv" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/tab-separated-values"),
                );
                let disp = format!("attachment; filename={}_converted.tsv", stem);
                if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
                    res_headers.insert(header::CONTENT_DISPOSITION, v);
                }
            }
            "qif" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("application/x-qif"),
                );
                let disp = format!("attachment; filename={}_converted.qif", stem);
                if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
                    res_headers.insert(header::CONTENT_DISPOSITION, v);
                }
            }
            "json" => {
                res_headers.insert(
                    header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("application/json"),
                );
                let disp = format!("attachment; filename={}_converted.json", stem);
                if let Ok(v) = axum::http::HeaderValue::from_str(&disp) {
                    res_headers.insert(header::CONTENT_DISPOSITION, v);
                }
            }
            _ => {}
        }
        Ok((StatusCode::OK, res_headers, bytes).into_response())
    } else {
        let reconciliation = reconcile_running_balance(&extracted_table);
        let json_response = serde_json::json!({
            "headers": extracted_table.headers,
            "active_indices": extracted_table.active_indices,
            "rows": extracted_table.rows,
            "reconciliation": reconciliation
        });
        Ok(Json(json_response).into_response())
    }
}

/// Cleans up any orphaned temporary PDF files left from previous unexpected terminations.
fn cleanup_stale_temp_files() {
    if let Ok(entries) = fs::read_dir("temp") {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    // Only ever clean our own dedicated temp subdirectory.
    let temp_dir = std::env::temp_dir().join("vaultparser");
    if let Ok(entries) = fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.ends_with(".pdf")
                && let Ok(meta) = entry.metadata()
                && let Ok(modified) = meta.modified()
                && let Ok(elapsed) = modified.elapsed()
                && elapsed.as_secs() > 1800
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (diagnostics go to stderr so stdout stays reserved for payloads)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vaultparser=info,tower_http=info,axum=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Clean up temporary files on startup
    cleanup_stale_temp_files();

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
