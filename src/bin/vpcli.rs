use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use vaultparser::exporter::{export_to_csv, export_to_json, export_to_xlsx};
use vaultparser::{
    BankPreset, ExtractionConfig, detect_column_guides, detect_preset_from_file, extract_from_file,
};

#[derive(Parser, Debug)]
#[command(
    name = "VaultParser CLI",
    version,
    about = "🏦 VaultParser — Pure Rust Bank Statement Extractor"
)]
struct Args {
    /// Path to the input statement PDF
    input_pdf: Option<String>,

    /// Bank preset name (e.g., hdfc, sbi, canara, union, uco, auto, or path to a JSON preset file)
    preset: Option<String>,

    /// Optional path to write output file (CSV, XLSX, or JSON). If omitted, prints CSV to stdout.
    output: Option<String>,

    /// Password to decrypt secure PDFs
    #[arg(short, long)]
    password: Option<String>,
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

    match (args.input_pdf, args.preset) {
        (Some(input_pdf), Some(preset)) => {
            run_extraction_process(
                &input_pdf,
                &preset,
                args.output.as_deref(),
                args.password.as_deref(),
            )?;
        }
        (None, None) => {
            run_wizard()?;
        }
        _ => {
            eprintln!(
                "Error: Both INPUT_PDF and PRESET must be provided, or run without arguments for the interactive wizard."
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_extraction_process(
    input_pdf: &str,
    preset_str: &str,
    output: Option<&str>,
    password: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let pdf_path = Path::new(input_pdf);
    if !pdf_path.exists() {
        eprintln!("Error: PDF file '{}' does not exist.", input_pdf);
        std::process::exit(1);
    }

    let spinner = ProgressBar::new_spinner();
    if let Ok(style) = ProgressStyle::default_spinner()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
        .template("{spinner:.green} {msg}")
    {
        spinner.set_style(style);
    }
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let config = if preset_str.to_lowercase() == "auto" {
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
                    "Available Presets: hdfc, sbi, canara, union, uco, auto, or a path to a JSON preset file"
                );
                std::process::exit(1);
            }
        };

        spinner.println(format!("Loading configuration for {}...", preset.name()));
        let mut c = preset.config();
        c.password = password.map(String::from);
        c
    };

    spinner.set_message(format!(
        "Extracting transaction table natively from '{}'...",
        input_pdf
    ));
    let table = extract_from_file(pdf_path, &config)?;
    spinner.finish_with_message(format!("Success! Extracted {} rows.", table.rows.len()));

    if let Some(out_path_str) = output {
        if out_path_str.to_lowercase().ends_with(".xlsx") {
            let xlsx_bytes = export_to_xlsx(&table)?;
            std::fs::write(out_path_str, &xlsx_bytes)?;
            println!("Saved Excel output to: {}", out_path_str);
        } else if out_path_str.to_lowercase().ends_with(".json") {
            let json_bytes = export_to_json(&table)?;
            std::fs::write(out_path_str, &json_bytes)?;
            println!("Saved JSON output to: {}", out_path_str);
        } else {
            let csv_bytes = export_to_csv(&table)?;
            std::fs::write(out_path_str, &csv_bytes)?;
            println!("Saved CSV output to: {}", out_path_str);
        }
    } else {
        let csv_bytes = export_to_csv(&table)?;
        let csv_text = String::from_utf8(csv_bytes)?;
        println!("\n--- Extracted Transactions (CSV) ---");
        println!("{}", csv_text);
    }

    Ok(())
}

fn run_wizard() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== 🏦 VaultParser CLI Interactive Wizard ===");

    // 1. Get input PDF path
    let mut input_pdf = String::new();
    loop {
        print!("📁 Enter path to input PDF bank statement: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        input_pdf.clear();
        std::io::stdin().read_line(&mut input_pdf)?;
        let trimmed = input_pdf.trim();
        if trimmed.is_empty() {
            println!("Error: PDF path cannot be empty.");
            continue;
        }
        let path = Path::new(trimmed);
        if !path.exists() {
            println!("Error: File '{}' does not exist.", trimmed);
            continue;
        }
        input_pdf = trimmed.to_string();
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
    println!("  12) Custom JSON configuration file");

    let mut preset = String::new();
    loop {
        print!("👉 Select option (1-12) [default: 1]: ");
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
                println!("Error: Invalid option. Please choose 1 to 12.");
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

    // 4. Get Output File path
    print!(
        "\n💾 Enter output path (e.g. output.xlsx, output.json, output.csv, or leave empty for stdout): "
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

    run_extraction_process(
        &input_pdf,
        &preset,
        output_opt.as_deref(),
        password_opt.as_deref(),
    )
}
