use std::env;
use std::path::Path;
use vaultparser::exporter::export_to_csv;
use vaultparser::{BankPreset, ExtractionConfig, detect_column_guides, extract_from_file};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("🏦 VaultParser Command Line Tool");
        println!("=================================");
        println!("Usage: cargo run --bin cli -- <input-pdf> <bank-preset> [output-csv] [options]");
        println!("Available Presets: hdfc, sbi, canara, union, uco, auto");
        println!("\nOptions:");
        println!("  -p, --password <password>   Specify the password to decrypt the PDF");
        println!("\nExamples:");
        println!("  cargo run --bin cli -- \"RKS USER CHARGES bank.pdf\" hdfc output.csv");
        println!("  cargo run --bin cli -- secure_statement.pdf auto -p \"my_secret_pass\"");
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vaultparser=info".into()),
        )
        .init();

    let pdf_path_str = &args[1];
    let preset_str = &args[2];

    let mut password = None;
    let mut output_path = None;

    let mut idx = 3;
    while idx < args.len() {
        match args[idx].as_str() {
            "--password" | "-p" => {
                if idx + 1 < args.len() {
                    password = Some(args[idx + 1].clone());
                    idx += 2;
                } else {
                    eprintln!("Error: Missing value for password option");
                    std::process::exit(1);
                }
            }
            path => {
                output_path = Some(path.to_string());
                idx += 1;
            }
        }
    }

    let pdf_path = Path::new(pdf_path_str);
    if !pdf_path.exists() {
        eprintln!("Error: PDF file '{}' does not exist.", pdf_path_str);
        std::process::exit(1);
    }

    let config = if preset_str.to_lowercase() == "auto" {
        println!("Analyzing PDF to auto-detect columns...");
        let guides = detect_column_guides(pdf_path, password.as_deref(), 0.0, 1.0)?;
        println!("Auto-detected column boundaries: {:?}", guides);

        let mut mappings = vec!["description".to_string()];
        for i in 0..guides.len() {
            mappings.push(format!("column_{}", i + 1));
        }

        ExtractionConfig::builder()
            .col_guides(guides)
            .col_mappings(mappings)
            .password(password)
            .build()
    } else {
        let preset = match BankPreset::from_str(preset_str) {
            Some(p) => p,
            None => {
                eprintln!("Error: Unknown bank preset '{}'.", preset_str);
                eprintln!("Available Presets: hdfc, sbi, canara, union, uco, auto");
                std::process::exit(1);
            }
        };

        println!("Loading configuration for {}...", preset.name());
        let mut c = preset.config();
        c.password = password;
        c
    };

    println!(
        "Extracting transaction table natively from '{}'...",
        pdf_path_str
    );
    let table = extract_from_file(pdf_path, &config)?;
    println!("Success! Extracted {} rows.", table.rows.len());

    let csv_bytes = export_to_csv(&table)?;

    if let Some(out_path_str) = output_path {
        std::fs::write(&out_path_str, &csv_bytes)?;
        println!("Saved CSV output to: {}", out_path_str);
    } else {
        let csv_text = String::from_utf8(csv_bytes)?;
        println!("\n--- Extracted Transactions (CSV) ---");
        println!("{}", csv_text);
    }

    Ok(())
}
