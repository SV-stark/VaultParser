use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use vaultparser::exporter::{export_to_csv, export_to_json, export_to_tsv, export_to_xlsx};
use vaultparser::{
    BankPreset, ExtractedTable, ExtractionConfig, detect_column_guides, detect_preset_from_file,
    extract_from_file,
};

#[derive(Parser, Debug)]
#[command(
    name = "VaultParser CLI",
    version,
    about = "🏦 VaultParser — Pure Rust Bank Statement Extractor"
)]
struct Args {
    /// Path to input statement PDF file or directory containing PDFs
    input_path: Option<String>,

    /// Bank preset name (e.g. hdfc, sbi, canara, union, uco, indian, hpscb, icici, pnb, kotak, axis, bob, yes, idfc, indusind, auto, or JSON preset file)
    preset: Option<String>,

    /// Optional output file or directory path. If omitted, prints results to stdout.
    output: Option<String>,

    /// Password to decrypt secure PDFs
    #[arg(short, long)]
    password: Option<String>,

    /// Output format (csv, tsv, xlsx, json). Overrides extension inference.
    #[arg(short, long)]
    format: Option<String>,

    /// Force directory batch processing mode
    #[arg(short, long)]
    dir: bool,

    /// Automatically categorize transactions into a 'Category' column (default: false)
    #[arg(short = 'c', long)]
    categorize: bool,

    /// Inclusive start date filter (YYYY-MM-DD or DD-MM-YYYY)
    #[arg(long = "from")]
    from_date: Option<String>,

    /// Inclusive end date filter (YYYY-MM-DD or DD-MM-YYYY)
    #[arg(long = "to")]
    to_date: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vaultparser=info".into()),
        )
        .init();

    match (args.input_path, args.preset) {
        (Some(input_path), Some(preset)) => {
            let path = Path::new(&input_path);
            if args.dir || path.is_dir() {
                run_batch_extraction(
                    path,
                    &preset,
                    args.output.as_deref(),
                    args.format.as_deref(),
                    args.password.as_deref(),
                    args.categorize,
                    args.from_date.as_deref(),
                    args.to_date.as_deref(),
                )?;
            } else {
                run_extraction_process(
                    &input_path,
                    &preset,
                    args.output.as_deref(),
                    args.format.as_deref(),
                    args.password.as_deref(),
                    args.categorize,
                    args.from_date.as_deref(),
                    args.to_date.as_deref(),
                )?;
            }
        }
        (None, None) => {
            run_wizard()?;
        }
        _ => {
            eprintln!(
                "Error: Both INPUT_PATH and PRESET must be provided, or run without arguments for the interactive wizard."
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

fn get_format_bytes(
    table: &ExtractedTable,
    fmt_str: &str,
) -> Result<(Vec<u8>, &'static str), Box<dyn std::error::Error>> {
    match fmt_str.to_lowercase().as_str() {
        "xlsx" | "excel" => Ok((export_to_xlsx(table)?, "xlsx")),
        "json" => Ok((export_to_json(table)?, "json")),
        "tsv" => Ok((export_to_tsv(table)?, "tsv")),
        _ => Ok((export_to_csv(table)?, "csv")),
    }
}

fn infer_format_from_str(s: &str) -> Option<&'static str> {
    let lower = s.to_lowercase();
    if lower.ends_with(".xlsx") {
        Some("xlsx")
    } else if lower.ends_with(".json") {
        Some("json")
    } else if lower.ends_with(".tsv") {
        Some("tsv")
    } else if lower.ends_with(".csv") {
        Some("csv")
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn load_preset_config(
    pdf_path: &Path,
    preset_str: &str,
    password: Option<&str>,
    categorize: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
    spinner: &ProgressBar,
) -> Result<ExtractionConfig, Box<dyn std::error::Error>> {
    let mut config = if preset_str.to_lowercase() == "auto" {
        spinner.set_message("Analyzing PDF to auto-detect bank preset...");
        if let Some(detected_preset) = detect_preset_from_file(pdf_path, password)? {
            spinner.println(format!(
                "Auto-detected bank preset: {}!",
                detected_preset.name()
            ));
            let mut c = detected_preset.config();
            c.password = password.map(String::from);
            c
        } else {
            spinner.println("No known bank preset matched. Falling back to auto-detecting column guide boundaries...");
            spinner.set_message("Auto-detecting column boundaries...");
            let guides = detect_column_guides(pdf_path, password, 0.0, 1.0)?;
            spinner.println(format!("Auto-detected column boundaries: {:?}", guides));

            let mut mappings = vec!["description".to_string()];
            for idx in 1..=guides.len() {
                mappings.push(format!("column_{}", idx));
            }

            ExtractionConfig::builder()
                .col_guides(guides)
                .col_mappings(mappings)
                .password(password.map(String::from))
                .build()?
        }
    } else if preset_str.to_lowercase().ends_with(".json") || Path::new(preset_str).exists() {
        spinner.set_message(format!(
            "Loading custom JSON preset from '{}'...",
            preset_str
        ));
        let content = std::fs::read_to_string(preset_str)?;
        let mut c: ExtractionConfig = serde_json::from_str(&content)?;
        c.password = password.map(String::from);
        c
    } else {
        let preset = match BankPreset::from_str(preset_str) {
            Some(p) => p,
            None => {
                spinner.finish_and_clear();
                eprintln!("Error: Unknown bank preset '{}'.", preset_str);
                eprintln!(
                    "Available Presets: hdfc, sbi, canara, union, uco, indian, hpscb, icici, pnb, kotak, axis, bob, yes, idfc, indusind, auto, or a JSON preset file"
                );
                std::process::exit(1);
            }
        };

        spinner.println(format!("Loading configuration for {}...", preset.name()));
        let mut c = preset.config();
        c.password = password.map(String::from);
        c
    };

    config.categorize = categorize;
    config.from_date = from_date.map(String::from);
    config.to_date = to_date.map(String::from);
    Ok(config)
}

fn process_single_pdf(
    pdf_path: &Path,
    preset_str: &str,
    password: Option<&str>,
    categorize: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<ExtractedTable, Box<dyn std::error::Error>> {
    let spinner = ProgressBar::new_spinner();
    if let Ok(style) = ProgressStyle::default_spinner()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
        .template("{spinner:.green} {msg}")
    {
        spinner.set_style(style);
    }
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let config = load_preset_config(
        pdf_path, preset_str, password, categorize, from_date, to_date, &spinner,
    )?;
    spinner.set_message(format!(
        "Extracting transaction table natively from '{}'...",
        pdf_path.display()
    ));
    let table = extract_from_file(pdf_path, &config)?;
    spinner.finish_with_message(format!("Success! Extracted {} rows.", table.rows.len()));
    Ok(table)
}

#[allow(clippy::too_many_arguments)]
fn run_extraction_process(
    input_pdf: &str,
    preset_str: &str,
    output: Option<&str>,
    format_override: Option<&str>,
    password: Option<&str>,
    categorize: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let pdf_path = Path::new(input_pdf);
    if !pdf_path.exists() {
        eprintln!("Error: PDF file '{}' does not exist.", input_pdf);
        std::process::exit(1);
    }

    let table = process_single_pdf(
        pdf_path, preset_str, password, categorize, from_date, to_date,
    )?;

    let target_format = format_override
        .or_else(|| output.and_then(infer_format_from_str))
        .unwrap_or("csv");

    if let Some(out_path_str) = output {
        let (bytes, ext) = get_format_bytes(&table, target_format)?;
        let final_path = if infer_format_from_str(out_path_str).is_some() {
            out_path_str.to_string()
        } else {
            format!("{}.{}", out_path_str, ext)
        };
        std::fs::write(&final_path, &bytes)?;
        println!("Saved {} output to: {}", ext.to_uppercase(), final_path);
    } else {
        let (bytes, ext) = get_format_bytes(&table, target_format)?;
        let text = String::from_utf8(bytes)?;
        println!("\n--- Extracted Transactions ({}) ---", ext.to_uppercase());
        println!("{}", text);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_batch_extraction(
    dir_path: &Path,
    preset_str: &str,
    output_path: Option<&str>,
    format_opt: Option<&str>,
    password: Option<&str>,
    categorize: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = std::fs::read_dir(dir_path)?;
    let mut pdf_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
                == Some("pdf".to_string())
        {
            pdf_files.push(p);
        }
    }

    if pdf_files.is_empty() {
        println!("No PDF files found in directory '{}'.", dir_path.display());
        return Ok(());
    }

    println!(
        "Found {} PDF statement(s) in '{}' for batch processing.",
        pdf_files.len(),
        dir_path.display()
    );

    let out_dir = match output_path {
        Some(p) => {
            let path = Path::new(p);
            if !path.exists() {
                std::fs::create_dir_all(path)?;
            }
            path.to_path_buf()
        }
        None => dir_path.to_path_buf(),
    };

    let target_format = format_opt.unwrap_or("csv");

    for pdf in &pdf_files {
        let file_stem = pdf
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("statement");
        println!("\n📂 Batch extracting '{}'...", pdf.display());

        match process_single_pdf(pdf, preset_str, password, categorize, from_date, to_date) {
            Ok(table) => {
                let (bytes, ext) = get_format_bytes(&table, target_format)?;
                let out_file_name = format!("{}_converted.{}", file_stem, ext);
                let dest_path = out_dir.join(out_file_name);
                std::fs::write(&dest_path, &bytes)?;
                println!(
                    "  ✅ Saved {} extracted rows to: {}",
                    table.rows.len(),
                    dest_path.display()
                );
            }
            Err(e) => {
                eprintln!("  ❌ Failed to process '{}': {}", pdf.display(), e);
            }
        }
    }

    println!("\n🎉 Batch extraction complete!");
    Ok(())
}

fn run_wizard() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== 🏦 VaultParser CLI Interactive Wizard ===");

    // 1. Get input PDF or directory path
    let mut input_path = String::new();
    loop {
        print!("📁 Enter path to input PDF file or folder of PDFs: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        input_path.clear();
        std::io::stdin().read_line(&mut input_path)?;
        let trimmed = input_path.trim();
        if trimmed.is_empty() {
            println!("Error: Path cannot be empty.");
            continue;
        }
        let path = Path::new(trimmed);
        if !path.exists() {
            println!("Error: File or folder '{}' does not exist.", trimmed);
            continue;
        }
        input_path = trimmed.to_string();
        break;
    }

    // 2. Select Bank Preset
    println!("\n🏦 Select Bank Preset:");
    println!("  1) Auto-Detect / Dynamic boundary detection (auto)");
    println!("  2) HDFC Bank India (hdfc)");
    println!("  3) State Bank of India (sbi)");
    println!("  4) Canara Bank (canara)");
    println!("  5) Union Bank of India (union)");
    println!("  6) UCO Bank (uco)");
    println!("  7) Indian Bank (indian)");
    println!("  8) H P State Co-operative Bank (hpscb)");
    println!("  9) ICICI Bank (icici)");
    println!("  10) Punjab National Bank (pnb)");
    println!("  11) Kotak Mahindra Bank (kotak)");
    println!("  12) Axis Bank (axis)");
    println!("  13) Bank of Baroda (bob)");
    println!("  14) YES Bank (yes)");
    println!("  15) IDFC FIRST Bank (idfc)");
    println!("  16) IndusInd Bank (indusind)");
    println!("  17) Custom JSON configuration file");

    let mut preset = String::new();
    loop {
        print!("👉 Select option (1-17) [default: 1]: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        preset.clear();
        std::io::stdin().read_line(&mut preset)?;
        let trimmed = preset.trim();
        if trimmed.is_empty() || trimmed == "1" {
            preset = "auto".to_string();
            break;
        }
        match trimmed {
            "2" => {
                preset = "hdfc".to_string();
                break;
            }
            "3" => {
                preset = "sbi".to_string();
                break;
            }
            "4" => {
                preset = "canara".to_string();
                break;
            }
            "5" => {
                preset = "union".to_string();
                break;
            }
            "6" => {
                preset = "uco".to_string();
                break;
            }
            "7" => {
                preset = "indian".to_string();
                break;
            }
            "8" => {
                preset = "hpscb".to_string();
                break;
            }
            "9" => {
                preset = "icici".to_string();
                break;
            }
            "10" => {
                preset = "pnb".to_string();
                break;
            }
            "11" => {
                preset = "kotak".to_string();
                break;
            }
            "12" => {
                preset = "axis".to_string();
                break;
            }
            "13" => {
                preset = "bob".to_string();
                break;
            }
            "14" => {
                preset = "yes".to_string();
                break;
            }
            "15" => {
                preset = "idfc".to_string();
                break;
            }
            "16" => {
                preset = "indusind".to_string();
                break;
            }
            "17" => {
                let mut json_path = String::new();
                loop {
                    print!("📂 Enter path to custom JSON preset file: ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    json_path.clear();
                    std::io::stdin().read_line(&mut json_path)?;
                    let path_trimmed = json_path.trim();
                    if path_trimmed.is_empty() {
                        println!("Error: Path cannot be empty.");
                        continue;
                    }
                    if !Path::new(path_trimmed).exists() {
                        println!("Error: Custom JSON file '{}' does not exist.", path_trimmed);
                        continue;
                    }
                    preset = path_trimmed.to_string();
                    break;
                }
                break;
            }
            _ => {
                if BankPreset::from_str(trimmed).is_some() || trimmed == "auto" {
                    preset = trimmed.to_string();
                    break;
                }
                println!("Error: Invalid option. Please choose 1 to 17.");
            }
        }
    }

    // 3. Get Password (optional)
    print!("\n🔑 Enter PDF password (leave empty if unencrypted): ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut password = String::new();
    std::io::stdin().read_line(&mut password)?;
    let password_opt = {
        let trimmed = password.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    // 4. Get Output File or Folder path
    print!(
        "\n💾 Enter output path (e.g. output.xlsx, output.tsv, output.csv, output.json, folder path, or leave empty for stdout): "
    );
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut output = String::new();
    std::io::stdin().read_line(&mut output)?;
    let output_opt = {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    let path = Path::new(&input_path);
    if path.is_dir() {
        run_batch_extraction(
            path,
            &preset,
            output_opt.as_deref(),
            None,
            password_opt.as_deref(),
            false,
            None,
            None,
        )
    } else {
        run_extraction_process(
            &input_path,
            &preset,
            output_opt.as_deref(),
            None,
            password_opt.as_deref(),
            false,
            None,
            None,
        )
    }
}
