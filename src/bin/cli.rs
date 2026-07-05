use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use vaultparser::exporter::export_to_csv;
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
    input_pdf: String,

    /// Bank preset name (e.g., hdfc, sbi, canara, union, uco, auto)
    preset: String,

    /// Optional path to write output file (CSV or XLSX). If omitted, prints CSV to stdout.
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

    let pdf_path = Path::new(&args.input_pdf);
    if !pdf_path.exists() {
        eprintln!("Error: PDF file '{}' does not exist.", args.input_pdf);
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

    let config = if args.preset.to_lowercase() == "auto" {
        spinner.set_message("Analyzing PDF to auto-detect bank preset...");
        if let Some(detected_preset) = detect_preset_from_file(pdf_path, args.password.as_deref())?
        {
            spinner.println(format!(
                "Auto-detected bank preset: {}!",
                detected_preset.name()
            ));
            let mut c = detected_preset.config();
            c.password = args.password.clone();
            c
        } else {
            spinner.println("No known bank preset matched. Falling back to auto-detecting column guide boundaries...");
            spinner.set_message("Auto-detecting column boundaries...");
            let guides = detect_column_guides(pdf_path, args.password.as_deref(), 0.0, 1.0)?;
            spinner.println(format!("Auto-detected column boundaries: {:?}", guides));

            let mut mappings = vec!["description".to_string()];
            for i in 0..guides.len() {
                mappings.push(format!("column_{}", i + 1));
            }

            ExtractionConfig::builder()
                .col_guides(guides)
                .col_mappings(mappings)
                .password(args.password.clone())
                .build()?
        }
    } else {
        let preset = match BankPreset::from_str(&args.preset) {
            Some(p) => p,
            None => {
                spinner.finish_and_clear();
                eprintln!("Error: Unknown bank preset '{}'.", args.preset);
                eprintln!("Available Presets: hdfc, sbi, canara, union, uco, auto");
                std::process::exit(1);
            }
        };

        spinner.println(format!("Loading configuration for {}...", preset.name()));
        let mut c = preset.config();
        c.password = args.password.clone();
        c
    };

    spinner.set_message(format!(
        "Extracting transaction table natively from '{}'...",
        args.input_pdf
    ));
    let table = extract_from_file(pdf_path, &config)?;
    spinner.finish_with_message(format!("Success! Extracted {} rows.", table.rows.len()));

    if let Some(out_path_str) = &args.output {
        if out_path_str.to_lowercase().ends_with(".xlsx") {
            use vaultparser::exporter::export_to_xlsx;
            let xlsx_bytes = export_to_xlsx(&table)?;
            std::fs::write(out_path_str, &xlsx_bytes)?;
            println!("Saved Excel output to: {}", out_path_str);
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
