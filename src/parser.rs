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
    min_y: f64,
    max_y: f64,
    y: f64,
    items: Vec<WordItem>,
}

fn is_possible_date(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let cleaned = val.trim();
    if cleaned.len() > 35 || cleaned.len() < 4 {
        return false;
    }

    let has_separator = cleaned.contains('-') || cleaned.contains('/') || cleaned.contains('.');
    let has_space = cleaned.contains(' ');

    if !has_separator && !has_space {
        return false;
    }

    if has_space && !has_separator {
        let months = [
            "jan",
            "feb",
            "mar",
            "apr",
            "may",
            "jun",
            "jul",
            "aug",
            "sep",
            "sept",
            "oct",
            "nov",
            "dec",
            "january",
            "february",
            "march",
            "april",
            "june",
            "july",
            "august",
            "september",
            "october",
            "november",
            "december",
        ];
        let lower = cleaned.to_lowercase();
        let contains_month = months.iter().any(|m| lower.contains(m));
        if !contains_month {
            return false;
        }
    }

    // Allow full month names (e.g. September = 9 chars) while filtering out long non-date text
    if cleaned.chars().filter(|c| c.is_ascii_alphabetic()).count() > 12 {
        return false;
    }
    cleaned.chars().filter(|c| c.is_ascii_digit()).count() >= 2
}

fn is_possible_amount(val: &str) -> bool {
    crate::exporter::parse_amount(val).is_some()
}

fn standardize_date(val: &str) -> String {
    let mut cleaned = val.trim().to_string();
    if cleaned.is_empty() {
        return String::new();
    }

    // Strip ordinal suffixes (1st, 2nd, 3rd, 4th...) so chrono can parse.
    {
        let chars: Vec<char> = cleaned.chars().collect();
        let mut out = String::with_capacity(cleaned.len());
        let mut i = 0;
        while i < chars.len() {
            out.push(chars[i]);
            if chars[i].is_ascii_digit() && i + 2 < chars.len() {
                let suf: String = chars[i + 1..i + 3].iter().collect();
                if suf.eq_ignore_ascii_case("st")
                    || suf.eq_ignore_ascii_case("nd")
                    || suf.eq_ignore_ascii_case("rd")
                    || suf.eq_ignore_ascii_case("th")
                {
                    i += 2;
                }
            }
            i += 1;
        }
        cleaned = out;
    }

    cleaned = cleaned.replace(" of ", " ");

    // Normalise the non-standard month abbreviation "Sept" / "Sept." to the
    // chrono-recognised "Sep" form without corrupting the full "September".
    let lower = cleaned.to_lowercase();
    if lower.contains("sept") && !lower.contains("september") {
        cleaned = cleaned.replace("Sept.", "Sep").replace("sept.", "sep");
        cleaned = cleaned.replace("Sept", "Sep").replace("sept", "sep");
        cleaned = cleaned.replace("SEPT", "SEP");
    }

    // Try a series of common datetime formats first
    let datetime_formats = [
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y %H:%M",
        "%d-%m-%Y %H:%M:%S",
        "%d-%m-%Y %H:%M",
        "%d-%b-%Y %H:%M:%S",
        "%d-%b-%Y %H:%M",
        "%d-%B-%Y %H:%M:%S",
        "%d-%B-%Y %H:%M",
        "%d %b, %Y",
        "%d %B, %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d/%m/%Y,",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];

    for fmt in &datetime_formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&cleaned, fmt) {
            return dt.format("%d-%m-%Y").to_string();
        }
    }

    // Try a series of common date formats
    let formats = [
        "%d/%m/%y",  // 30/04/25
        "%d/%m/%Y",  // 30/04/2025
        "%d-%m-%y",  // 30-04-25
        "%d-%m-%Y",  // 30-04-2025
        "%d.%m.%y",  // 30.04.25
        "%d.%m.%Y",  // 30.04.2025
        "%d %b %y",  // 30 Apr 25
        "%d %b %Y",  // 30 Apr 2025
        "%d-%b-%y",  // 30-Apr-25
        "%d-%b-%Y",  // 30-Apr-2025
        "%d %B %y",  // 30 April 25
        "%d %B %Y",  // 30 April 2025
        "%d-%B-%y",  // 30-April-25
        "%d-%B-%Y",  // 30-April-2025
        "%B %d, %Y", // September 30, 2025
        "%b %d, %Y", // Sep 30, 2025
        "%Y-%m-%d",  // 2025-04-30
        "%Y/%m/%d",  // 2025/04/30
        "%m/%d/%Y",  // 04/30/2025
        "%m/%d/%y",  // 04/30/25
        "%d %b, %Y",
        "%d %B, %Y",
        "%d %b.",
        "%d-%b.",
        "%d/%m/%Y,",
    ];

    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(&cleaned, fmt) {
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
    // Only fall back to lopdf when the primary extractor cannot open the file
    // at all (e.g. encrypted or structurally damaged). A page with zero words
    // (blank/scanned) is not by itself evidence that decryption is needed and
    // forcing lopdf can surface spurious password errors.
    let use_decrypted = PdfDocument::open(temp_path).is_err();

    if use_decrypted {
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let decrypted_name = format!(
            "vp_decrypted_{}_{}_{}.pdf",
            pid,
            ts,
            temp_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("temp")
        );
        let decrypted_path = std::env::temp_dir().join(decrypted_name);
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

/// Infer column mappings for an unknown/auto statement by scanning the first
/// page's (header) words and assigning each column a best-guess label.
pub fn detect_column_mappings<P: AsRef<Path>>(
    pdf_path: P,
    password: Option<&str>,
    guides: &[f64],
) -> Result<Vec<String>, ExtractorError> {
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
        // Fall back to generic labels matching the guide count.
        let mut mappings = vec!["date".to_string()];
        for idx in 1..=guides.len() {
            mappings.push(format!("column_{}", idx));
        }
        return Ok(mappings);
    }

    let page = &pages[0];
    let width = page.width;
    let height = page.height;
    let header_limit = height * 0.30;

    let page_guides: Vec<f64> = guides.iter().map(|&g| g * width).collect();
    let num_cols = page_guides.len() + 1;

    let mut column_text = vec![String::new(); num_cols];
    for w in page.extract_words() {
        if w.top > header_limit {
            continue;
        }
        let center = (w.x0 + w.x1) / 2.0;
        let mut col = 0;
        while col < page_guides.len() && center > page_guides[col] {
            col += 1;
        }
        if !column_text[col].is_empty() {
            column_text[col].push(' ');
        }
        column_text[col].push_str(&w.text);
    }

    let mut mappings = Vec::with_capacity(num_cols);
    let mut used_date = false;
    for (idx, text) in column_text.iter().enumerate() {
        let t = text.to_lowercase();
        let label = if t.contains("value date") || t.contains("value dt") {
            "value_date".to_string()
        } else if !used_date && (t.contains("date") || t.contains("dt") || t.contains("txn date")) {
            used_date = true;
            "date".to_string()
        } else if t.contains("description")
            || t.contains("narration")
            || t.contains("particulars")
            || t.contains("details")
        {
            "description".to_string()
        } else if t.contains("cheque") || t.contains("chq") || t.contains("instrument") {
            "chq_no".to_string()
        } else if t.contains("debit") || t.contains("withdrawal") || t.contains("dr") {
            "debit".to_string()
        } else if t.contains("credit") || t.contains("deposit") || t.contains("cr") {
            "credit".to_string()
        } else if t.contains("balance") {
            "balance".to_string()
        } else if t.contains("reference") || t.contains("ref") || t.contains("txn") {
            "reference".to_string()
        } else if t.contains("amount") {
            "amount".to_string()
        } else if idx == 0 && !used_date {
            used_date = true;
            "date".to_string()
        } else {
            format!("column_{}", idx)
        };
        mappings.push(label);
    }

    // Ensure date is present somewhere; fall back to first column if none matched.
    if !mappings.iter().any(|m| m == "date") && num_cols > 0 {
        mappings[0] = "date".to_string();
    }

    Ok(mappings)
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
    } else if full_text.contains("HIMACHAL PRADESH GRAMIN") || full_text.contains("HPGB") {
        Ok(Some(crate::presets::BankPreset::Hpgb))
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
    } else if full_text.contains("AXIS BANK") || full_text.contains("AXISBANK") {
        Ok(Some(crate::presets::BankPreset::Axis))
    } else if full_text.contains("BANK OF BARODA") || full_text.contains("BARODA") {
        Ok(Some(crate::presets::BankPreset::Bob))
    } else if full_text.contains("YES BANK") || full_text.contains("YESBANK") {
        Ok(Some(crate::presets::BankPreset::Yes))
    } else if full_text.contains("IDFC FIRST") || full_text.contains("IDFC BANK") {
        Ok(Some(crate::presets::BankPreset::Idfc))
    } else if full_text.contains("INDUSIND") {
        Ok(Some(crate::presets::BankPreset::Indusind))
    } else if full_text.contains("UJJIVAN") {
        Ok(Some(crate::presets::BankPreset::Ujjivan))
    } else if full_text.contains("FEDERAL BANK") {
        Ok(Some(crate::presets::BankPreset::Federal))
    } else if full_text.contains("CENTRAL BANK OF INDIA") || full_text.contains("CBI ") {
        Ok(Some(crate::presets::BankPreset::Cbi))
    } else if full_text.contains("BANDHAN BANK") || full_text.contains("BANDHAN") {
        Ok(Some(crate::presets::BankPreset::Bandhan))
    } else if full_text.contains("RBL BANK") || full_text.contains("RATNAKAR") {
        Ok(Some(crate::presets::BankPreset::Rbl))
    } else if full_text.contains("PUNJAB & SIND")
        || full_text.contains("PUNJAB AND SIND")
        || full_text.contains("PSB ")
    {
        Ok(Some(crate::presets::BankPreset::Psb))
    } else {
        Ok(None)
    }
}

fn parse_date_naive(s: &str) -> Option<chrono::NaiveDate> {
    crate::config::parse_date(s)
}

pub fn classify_transaction(narrative: &str) -> &'static str {
    let text = narrative.to_uppercase();
    if text.contains("UPI")
        || text.contains("PAYTM")
        || text.contains("GPAY")
        || text.contains("GPH")
        || text.contains("PHONEPE")
        || text.contains("IMPS")
        || text.contains("NEFT")
        || text.contains("RTGS")
        || text.contains("BHIM")
        || text.contains("RAZORPAY")
    {
        "UPI & Transfers"
    } else if text.contains("SALARY") || text.contains("PAYROLL") || text.contains("STIPEND") {
        "Salary & Income"
    } else if text.contains("INTEREST")
        || text.contains("DIVIDEND")
        || text.contains("INT PD")
        || text.contains("INT.PD")
        || text.contains("CREDIT INT")
    {
        "Interest & Dividends"
    } else if text.contains("MUTUAL")
        || text.contains("ZERODHA")
        || text.contains("GROWW")
        || text.contains("SIP")
        || text.contains("STOCKS")
        || text.contains("SECURITIES")
        || text.contains("FUNDS")
        || text.contains("NAV ")
    {
        "Investments"
    } else if text.contains("ATM")
        || text.contains("CASH WDL")
        || text.contains("CASH WITHDRAWAL")
        || text.contains("ATM-WDL")
        || text.contains("CASH DEP")
    {
        "ATM & Cash"
    } else if text.contains("AMAZON")
        || text.contains("FLIPKART")
        || text.contains("POS ")
        || text.contains("E-COMMERCE")
        || text.contains("MERCHANT")
        || text.contains("RETAIL")
        || text.contains("MYNTRA")
    {
        "Shopping & Merchants"
    } else if text.contains("SWIGGY")
        || text.contains("ZOMATO")
        || text.contains("DUNZO")
        || text.contains("DINE")
        || text.contains("RESTAURANT")
        || text.contains("FOOD")
    {
        "Food & Dining"
    } else if text.contains("ELECTRICITY")
        || text.contains("WATER")
        || text.contains("BROADBAND")
        || text.contains("AIRTEL")
        || text.contains("JIO")
        || text.contains("RECHARGE")
        || text.contains("GAS")
    {
        "Bills & Utilities"
    } else if text.contains("CHARGES")
        || text.contains("GST")
        || text.contains("FEE")
        || text.contains("TAX")
        || text.contains("PENALTY")
        || text.contains("ANNUAL FEE")
        || text.contains("CHQ RET")
        || text.contains("SMS CHARGES")
    {
        "Bank Fees & Tax"
    } else {
        "Suspense"
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
        if err_str.to_lowercase().contains("password") {
            ExtractorError::PasswordError("Incorrect or missing password".to_string())
        } else {
            ExtractorError::PdfOpenError(format!("{:?}", e))
        }
    })?;

    let mut all_rows: Vec<PageRow> = Vec::new();
    let pages = pdf.pages();

    let total_words: usize = pages.iter().map(|p| p.extract_words().len()).sum();
    if !pages.is_empty() && total_words == 0 {
        return Err(ExtractorError::ScannedPdf(
            "No extractable text found in this PDF. The document appears to be a scanned image or photograph."
                .to_string(),
        ));
    }

    // Parse the inclusive date-range bounds once up front instead of re-parsing
    // them for every row on every page.
    let from_naive = config.from_date.as_deref().and_then(parse_date_naive);
    let to_naive = config.to_date.as_deref().and_then(parse_date_naive);
    let has_date_range = from_naive.is_some() || to_naive.is_some();
    if has_date_range && !config.col_mappings.iter().any(|m| m == "date") {
        warn!(
            "A from_date/to_date range was supplied but no column is mapped to 'date'; \
             the date range filter will be skipped."
        );
    }

    for (page_idx, page) in pages.iter().enumerate() {
        let width = page.width;
        let height = page.height;
        let top_px = config.y_top_trim * height;
        let bottom_px = config.y_bottom_trim * height;

        let mut items: Vec<WordItem> = page
            .extract_words()
            .into_iter()
            .filter(|w| w.top >= top_px && w.top <= bottom_px)
            .map(|w| WordItem {
                text: w.text,
                x: w.x0,
                y: w.top,
                width: w.x1 - w.x0,
            })
            .collect();

        if items.is_empty() {
            continue;
        }

        items.sort_by(|a, b| a.y.total_cmp(&b.y));

        let mut grouped_rows: Vec<GroupedRow> = Vec::new();
        for item in items {
            // Attach the word to the *nearest* anchor inside the tolerance
            // window, not merely the first one found. Scanning in insertion
            // order made a word jump into the previous row whenever two
            // consecutive anchors sat closer together than `y_tolerance`
            // (e.g. anchors at 100 and 106 with y_tolerance 6 swallowed a
            // word at y=105.5 that belongs to the 106 row).
            let mut best_idx: Option<usize> = None;
            let mut best_dist = f64::INFINITY;
            for (idx, r) in grouped_rows.iter().enumerate() {
                let dist = (r.anchor_y - item.y).abs();
                let new_min = r.min_y.min(item.y);
                let new_max = r.max_y.max(item.y);
                // Keep each clustered row's vertical span inside the tolerance so
                // words from a following line cannot be absorbed just because they
                // happen to sit closer to this row's anchor than to their own.
                if dist <= config.y_tolerance
                    && (new_max - new_min) <= config.y_tolerance
                    && dist < best_dist
                {
                    best_dist = dist;
                    best_idx = Some(idx);
                }
            }
            match best_idx {
                Some(idx) => {
                    let row = &mut grouped_rows[idx];
                    row.items.push(item.clone());
                    row.min_y = row.min_y.min(item.y);
                    row.max_y = row.max_y.max(item.y);
                    let total_y: f64 = row.items.iter().map(|it| it.y).sum();
                    row.y = total_y / row.items.len() as f64;
                }
                None => grouped_rows.push(GroupedRow {
                    anchor_y: item.y,
                    min_y: item.y,
                    max_y: item.y,
                    y: item.y,
                    items: vec![item],
                }),
            }
        }

        for r in &mut grouped_rows {
            let total_y: f64 = r.items.iter().map(|it| it.y).sum();
            r.y = total_y / r.items.len() as f64;
            r.items.sort_by(|a, b| a.x.total_cmp(&b.x));
        }
        grouped_rows.sort_by(|a, b| a.y.total_cmp(&b.y));

        let page_guides: Vec<f64> = config.col_guides.iter().map(|&g| g * width).collect();
        let num_cols = page_guides.len() + 1;

        let mut page_rows = Vec::with_capacity(grouped_rows.len());
        for (row_idx, r) in grouped_rows.into_iter().enumerate() {
            let mut cell_contents = vec![Vec::new(); num_cols];
            for item in r.items {
                // Prefer the column whose horizontal band overlaps the word
                // box the most. Falling back to centre-point assignment can
                // push wide words (or words straddling a guide) into the wrong
                // column purely because their midpoint crossed the divider.
                let mut bounds = Vec::with_capacity(page_guides.len() + 2);
                bounds.push(0.0);
                bounds.extend_from_slice(&page_guides);
                bounds.push(width);

                let x0 = item.x;
                let x1 = item.x + item.width;
                let mut best_col = 0usize;
                let mut best_overlap = -1.0_f64;
                for col in 0..num_cols {
                    let lo = bounds[col].max(x0);
                    let hi = bounds[col + 1].min(x1);
                    let overlap = (hi - lo).max(0.0);
                    if overlap > best_overlap {
                        best_overlap = overlap;
                        best_col = col;
                    }
                }

                // If the word does not overlap any band (degenerate width),
                // fall back to the traditional midpoint rule.
                if best_overlap <= 0.0 {
                    let center_x = item.x + item.width / 2.0;
                    let mut col_idx = 0;
                    while col_idx < page_guides.len() && center_x > page_guides[col_idx] {
                        col_idx += 1;
                    }
                    best_col = col_idx;
                }

                cell_contents[best_col].push(item);
            }

            let mut cells = Vec::with_capacity(num_cols);
            for mut items_in_col in cell_contents {
                items_in_col.sort_by(|a, b| a.x.total_cmp(&b.x));
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
                id: format!("row-{}-{}", page_idx + 1, row_idx),
                y: r.y,
                page: page_idx + 1,
                cells,
            });
        }

        page_rows.retain(|row| row.cells.iter().any(|c| !c.is_empty()));

        let page_str = (page_idx + 1).to_string();

        // Handle manual row deletions (support both stable row ID and legacy y-key)
        if let Some(deletes) = config.deleted_rows.get(&page_str)
            && !deletes.is_empty()
        {
            page_rows.retain(|row| {
                if deletes.get(&row.id).copied().unwrap_or(false) {
                    return false;
                }
                let y_key = format!("{:.2}", row.y);
                !deletes.get(&y_key).copied().unwrap_or(false)
            });
        }

        // Handle manual cell edits (support both stable row ID and legacy y-key)
        if let Some(edits) = config.manual_edits.get(&page_str)
            && !edits.is_empty()
        {
            for row in &mut page_rows {
                let col_edits_opt = edits.get(&row.id).or_else(|| {
                    let y_key = format!("{:.2}", row.y);
                    edits.get(&y_key)
                });
                if let Some(col_edits) = col_edits_opt {
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

        // Normalise cell whitespace for consistent downstream parsing/filtering.
        for row in &mut page_rows {
            for cell in &mut row.cells {
                // Drop control characters and collapse all whitespace runs.
                let cleaned: String = cell.chars().filter(|c| !c.is_control()).collect();
                *cell = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
            }
        }

        let desc_idx = config.col_mappings.iter().position(|m| m == "description");
        let date_idx = config.col_mappings.iter().position(|m| m == "date");
        let val_date_idx = config.col_mappings.iter().position(|m| m == "value_date");
        let amount_idx = config.col_mappings.iter().position(|m| m == "amount");
        let debit_idx = config.col_mappings.iter().position(|m| m == "debit");
        let credit_idx = config.col_mappings.iter().position(|m| m == "credit");
        let balance_idx = config.col_mappings.iter().position(|m| m == "balance");

        // Lightweight auto-detection of repeated table header rows.
        let header_keywords = [
            "date",
            "description",
            "particulars",
            "narration",
            "details",
            "debit",
            "credit",
            "balance",
            "amount",
            "value",
            "chq",
            "cheque",
            "reference",
            "ref",
            "transaction",
            "sl",
            "s no",
            "sr",
        ];
        page_rows.retain(|row| {
            let joined = row
                .cells
                .iter()
                .map(|c| c.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            let hits = header_keywords
                .iter()
                .filter(|kw| joined.contains(*kw))
                .count();
            let has_date = date_idx.is_some_and(|idx| {
                row.cells
                    .get(idx)
                    .map(|c| is_possible_date(c))
                    .unwrap_or(false)
            });
            let has_amount = [amount_idx, debit_idx, credit_idx]
                .into_iter()
                .flatten()
                .any(|idx| {
                    row.cells
                        .get(idx)
                        .map(|c| is_possible_amount(c))
                        .unwrap_or(false)
                });

            // Auto-skip repeated footer/page-marker rows as well.
            let is_footer_like = (joined.contains("page")
                || joined.contains("continued")
                || joined.contains("carried forward")
                || joined.contains("brought forward")
                || joined.contains("b/f")
                || joined.contains("c/f"))
                && !has_date
                && !has_amount;

            !(hits >= 3 && !has_date && !has_amount) && !is_footer_like
        });

        // Standardize date formats using chrono NaiveDate parser
        for idx in [date_idx, val_date_idx].into_iter().flatten() {
            for row in &mut page_rows {
                if idx < row.cells.len() && !row.cells[idx].is_empty() {
                    row.cells[idx] = standardize_date(&row.cells[idx]);
                }
            }
        }

        if let Some(desc_idx) = config.merge_multi_line.then_some(desc_idx).flatten() {
            let mut merged: Vec<PageRow> = Vec::with_capacity(page_rows.len());
            for row in page_rows {
                let cell_desc = row.cells.get(desc_idx).map(|s| s.as_str()).unwrap_or("");
                let is_date_empty =
                    date_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_amount_empty =
                    amount_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_debit_empty =
                    debit_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_credit_empty =
                    credit_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_balance_empty =
                    balance_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));

                let is_continuation = is_date_empty
                    && is_amount_empty
                    && is_debit_empty
                    && is_credit_empty
                    && is_balance_empty
                    && !cell_desc.is_empty();

                if is_continuation {
                    if let Some(last_row) = merged.last_mut()
                        && let Some(last_desc) = last_row.cells.get_mut(desc_idx)
                    {
                        *last_desc = format!("{} {}", last_desc, cell_desc).trim().to_string();
                    }
                } else {
                    merged.push(row);
                }
            }
            page_rows = merged;
        }

        // Carry continuation-only rows from the top of the current page onto the
        // last transaction of the previous page (page boundaries can split a
        // multi-line narration). This runs before skip/filtering so the carried
        // rows are not discarded by `filter_only_date`.
        if page_idx > 0 {
            let cell_desc_of = |row: &PageRow| -> String {
                desc_idx
                    .and_then(|idx| row.cells.get(idx))
                    .cloned()
                    .unwrap_or_default()
            };
            let is_row_continuation = |row: &PageRow| -> bool {
                let cell_desc = cell_desc_of(row);
                let is_date_empty =
                    date_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_amount_empty =
                    amount_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_debit_empty =
                    debit_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_credit_empty =
                    credit_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));
                let is_balance_empty =
                    balance_idx.is_none_or(|idx| row.cells.get(idx).is_none_or(|c| c.is_empty()));

                is_date_empty
                    && is_amount_empty
                    && is_debit_empty
                    && is_credit_empty
                    && is_balance_empty
                    && !cell_desc.is_empty()
            };

            while !page_rows.is_empty()
                && is_row_continuation(&page_rows[0])
                && !all_rows.is_empty()
            {
                let carried_desc = cell_desc_of(&page_rows[0]).to_string();
                if let Some(d_idx) = desc_idx
                    && let Some(last_row) = all_rows.last_mut()
                    && let Some(last_desc) = last_row.cells.get_mut(d_idx)
                {
                    *last_desc = format!("{} {}", last_desc, carried_desc).trim().to_string();
                }
                page_rows.remove(0);
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
            if config.filter_only_date
                && date_idx
                    .is_some_and(|idx| row.cells.get(idx).is_none_or(|c| !is_possible_date(c)))
            {
                keep = false;
            }

            if config.filter_only_amount {
                let has_amount = amount_idx
                    .is_some_and(|idx| row.cells.get(idx).is_some_and(|c| is_possible_amount(c)));
                let has_debit = debit_idx
                    .is_some_and(|idx| row.cells.get(idx).is_some_and(|c| is_possible_amount(c)));
                let has_credit = credit_idx
                    .is_some_and(|idx| row.cells.get(idx).is_some_and(|c| is_possible_amount(c)));

                if !has_amount && !has_debit && !has_credit {
                    keep = false;
                }
            }

            // Date Range Filtering
            if has_date_range && let Some(d_idx) = date_idx {
                if let Some(row_d) = row.cells.get(d_idx).and_then(|c| parse_date_naive(c)) {
                    if from_naive.is_some_and(|from_d| row_d < from_d) {
                        keep = false;
                    }
                    if to_naive.is_some_and(|to_d| row_d > to_d) {
                        keep = false;
                    }
                } else {
                    // Let rows whose date cannot be interpreted survive the
                    // range filter rather than being silently dropped. Such
                    // rows are still subject to `filter_only_date` above.
                    debug!(
                        "Date in row could not be parsed against range filter, retaining row: {:?}",
                        row.cells.get(d_idx)
                    );
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

    // Categorization Tagging Engine
    if config.categorize {
        let desc_col_idx = config
            .col_mappings
            .iter()
            .position(|m| m.to_lowercase() == "description" || m.to_lowercase() == "narrative")
            .unwrap_or(1);

        headers.push("CATEGORY".to_string());
        let cat_col_index = config.col_mappings.len();
        active_indices.push(cat_col_index);

        for row in &mut all_rows {
            while row.cells.len() < cat_col_index {
                row.cells.push(String::new());
            }
            let narrative = if desc_col_idx < row.cells.len() {
                &row.cells[desc_col_idx]
            } else {
                ""
            };
            let category = classify_transaction(narrative);
            row.cells.push(category.to_string());
        }
    }

    // Lightweight balance continuity audit: surfaces potential clustering or
    // column-shift issues as warnings without changing extracted values.
    if let (Some(b_idx), Some(d_idx), Some(c_idx)) = (
        config.col_mappings.iter().position(|m| m == "balance"),
        config.col_mappings.iter().position(|m| m == "debit"),
        config.col_mappings.iter().position(|m| m == "credit"),
    ) {
        let mut previous_balance: Option<f64> = None;
        for row in &all_rows {
            let current = crate::exporter::parse_amount(
                row.cells.get(b_idx).map(String::as_str).unwrap_or(""),
            );
            let debit = crate::exporter::parse_amount(
                row.cells.get(d_idx).map(String::as_str).unwrap_or(""),
            )
            .unwrap_or(0.0);
            let credit = crate::exporter::parse_amount(
                row.cells.get(c_idx).map(String::as_str).unwrap_or(""),
            )
            .unwrap_or(0.0);

            if let Some(current_balance) = current {
                if let Some(prev) = previous_balance
                    && (current_balance - prev - (credit - debit)).abs() > 0.015
                {
                    warn!(
                        "Balance continuity mismatch on page {} ({}): expected {:.2}, found {:.2}",
                        row.page,
                        row.id,
                        prev + credit - debit,
                        current_balance
                    );
                }
                previous_balance = Some(current_balance);
            }
        }
    }

    // Date-order and duplicate-row diagnostics (non-fatal).
    if let Some(d_idx) = config.col_mappings.iter().position(|m| m == "date") {
        let mut previous: Option<chrono::NaiveDate> = None;
        for row in &all_rows {
            if let Some(current) = row
                .cells
                .get(d_idx)
                .and_then(|c| crate::config::parse_date(c.trim()))
            {
                if let Some(prev_date) = previous
                    && current < prev_date
                {
                    warn!(
                        "Transaction date out of sequence on page {} ({}): {} < {}",
                        row.page, row.id, current, prev_date
                    );
                }
                previous = Some(current);
            }
        }
    }

    {
        use std::collections::HashSet;
        let mut seen: HashSet<String> = HashSet::new();
        for row in &all_rows {
            let key = row.cells.join("|");
            if !key.trim().is_empty() && !seen.insert(key.clone()) {
                warn!(
                    "Possible duplicate transaction row on page {}: {}",
                    row.page, row.id
                );
            }
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

    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let temp_name = format!("vp_bytes_{}_{}.pdf", pid, ts);
    let temp_path = std::env::temp_dir().join(temp_name);

    fs::write(&temp_path, pdf_bytes)?;
    let _guard = TempFileGuard::new(temp_path.clone());

    extract_from_file(&temp_path, config)
}

/// Automatically detect column guides directly from PDF bytes
pub fn detect_column_guides_from_bytes(
    pdf_bytes: &[u8],
    password: Option<&str>,
    y_top_trim: f64,
    y_bottom_trim: f64,
) -> Result<Vec<f64>, ExtractorError> {
    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!("vp_guides_{}_{}.pdf", std::process::id(), ts);
    let temp_path = std::env::temp_dir().join(temp_name);

    fs::write(&temp_path, pdf_bytes)?;
    let _guard = TempFileGuard::new(temp_path.clone());

    detect_column_guides(&temp_path, password, y_top_trim, y_bottom_trim)
}

/// Automatically detect bank preset directly from PDF bytes
pub fn detect_preset_from_bytes(
    pdf_bytes: &[u8],
    password: Option<&str>,
) -> Result<Option<crate::presets::BankPreset>, ExtractorError> {
    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!("vp_preset_{}_{}.pdf", std::process::id(), ts);
    let temp_path = std::env::temp_dir().join(temp_name);

    fs::write(&temp_path, pdf_bytes)?;
    let _guard = TempFileGuard::new(temp_path.clone());

    detect_preset_from_file(&temp_path, password)
}

/// Automatically infer column mappings directly from PDF bytes
pub fn detect_column_mappings_from_bytes(
    pdf_bytes: &[u8],
    password: Option<&str>,
    guides: &[f64],
) -> Result<Vec<String>, ExtractorError> {
    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!("vp_maps_{}_{}.pdf", std::process::id(), ts);
    let temp_path = std::env::temp_dir().join(temp_name);

    fs::write(&temp_path, pdf_bytes)?;
    let _guard = TempFileGuard::new(temp_path.clone());

    detect_column_mappings(&temp_path, password, guides)
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
        assert!(is_possible_date("30 September 2025"));
        assert!(is_possible_date("1/1/25"));
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
        assert!(is_possible_amount("(1,234.50)"));
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

            // Check first extracted row values
            let first_row = &table.rows[0];
            assert!(
                !first_row.cells.is_empty(),
                "First row cells should not be empty"
            );
            assert!(first_row.page >= 1);
            assert!(first_row.id.starts_with("row-"));
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
            eprintln!(
                "Warning: required PDF file 'RKS USER CHARGES bank.pdf' was not found; skipping."
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
        assert_eq!(standardize_date("30 September 2025"), "30-09-2025");
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

        // from_date > to_date
        let config_res4 = ExtractionConfig::builder()
            .from_date(Some("2024-12-31".to_string()))
            .to_date(Some("2024-01-01".to_string()))
            .build();
        assert!(config_res4.is_err());

        // Negative y_tolerance
        let config_res5 = ExtractionConfig::builder().y_tolerance(-1.0).build();
        assert!(config_res5.is_err());

        // Guide > 1.0
        let config_res6 = ExtractionConfig::builder()
            .col_guides(vec![1.2])
            .col_mappings(vec!["date".to_string(), "amount".to_string()])
            .build();
        assert!(config_res6.is_err());
    }

    #[test]
    fn test_preset_from_str_trait() {
        assert_eq!("hdfc".parse::<BankPreset>().unwrap(), BankPreset::Hdfc);
        assert_eq!("SBI".parse::<BankPreset>().unwrap(), BankPreset::Sbi);
        assert_eq!("axis".parse::<BankPreset>().unwrap(), BankPreset::Axis);
        assert!("unknown_bank".parse::<BankPreset>().is_err());
        assert_eq!(BankPreset::from_str("hdfc"), Some(BankPreset::Hdfc));
        assert_eq!(BankPreset::from_str("unknown_bank"), None);
    }

    #[test]
    fn test_classify_transaction() {
        assert_eq!(
            classify_transaction("UPI-SWIGGY-BANGALORE"),
            "UPI & Transfers"
        );
        assert_eq!(classify_transaction("SALARY FOR JULY"), "Salary & Income");
        assert_eq!(classify_transaction("ATM CASH WITHDRAWAL"), "ATM & Cash");
        assert_eq!(
            classify_transaction("AMAZON INDIA PAY"),
            "Shopping & Merchants"
        );
        assert_eq!(classify_transaction("ZOMATO ORDER #123"), "Food & Dining");
        assert_eq!(
            classify_transaction("AIRTEL BROADBAND BILL"),
            "Bills & Utilities"
        );
        assert_eq!(
            classify_transaction("CREDIT INTEREST PAID"),
            "Interest & Dividends"
        );
        assert_eq!(classify_transaction("ZERODHA BROKING SIP"), "Investments");
        assert_eq!(
            classify_transaction("RANDOM UNKNOWN CHARGE 9999"),
            "Suspense"
        );
    }

    #[test]
    fn test_hpgb_extraction() {
        let pdf_path = Path::new("HIMACHAL PRADESH GRAMIN BANK.pdf");
        if pdf_path.exists() {
            let preset = detect_preset_from_file(pdf_path, None).unwrap();
            assert_eq!(preset, Some(BankPreset::Hpgb));

            let config = BankPreset::Hpgb.config();
            let table = extract_from_file(pdf_path, &config).unwrap();
            assert!(
                !table.rows.is_empty(),
                "HPGB table should contain extracted rows"
            );
        }
    }
}
