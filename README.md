# 🏦 VaultParser — Pure Rust Bank Statement Extractor

[![crates.io](https://img.shields.io/crates/v/vaultparser.svg)](https://crates.io/crates/vaultparser) [![docs.rs](https://docs.rs/vaultparser/badge.svg)](https://docs.rs/vaultparser) [![CI](https://github.com/SV-stark/VaultParser/actions/workflows/release.yml/badge.svg)](https://github.com/SV-stark/VaultParser/actions/workflows/release.yml) [![license](https://img.shields.io/crates/l/vaultparser.svg)](https://github.com/SV-stark/VaultParser/blob/main/LICENSE)

VaultParser is a high-performance, secure, offline-only library and interactive web service built in **pure Rust** for extracting tabular transaction ledgers from PDF bank statements.

It is designed to be **100% Python-free**, utilizing native Rust parsers and coordinate-clustering engines to process PDF structures with zero external runtimes (including native support for decrypting AES-256 encrypted PDFs with empty passwords using `lopdf 0.39.0`).

---

## 🚀 Key Features

*   **📦 Native Rust Library**: Can be integrated directly into other Rust applications.
*   **🛠️ Builder-Pattern Configurations**: Programmatic setup of column boundaries, row tolerance margins, page trimming, and transaction row filters.
*   **🏦 Indian Bank Presets**: Out-of-the-box templates for:
    *   HDFC Bank
    *   State Bank of India (SBI)
    *   Canara Bank
    *   Union Bank of India
    *   UCO Bank
    *   Indian Bank
    *   H P State Co-operative Bank (HPSCB)
    *   ICICI Bank
    *   Punjab National Bank (PNB)
    *   Kotak Mahindra Bank
*   **✨ Interactive Web UI**: Includes a web-based visual dashboard where you can:
    *   Drag, double-click to add, or right-click to delete vertical column guides over the PDF canvas.
    *   Crop header/footer regions in real-time.
    *   Directly override cell values or delete rows with coordinates that persist across exports.
*   **💾 Multi-format Exporters**: Bulk compile transaction history to `.xlsx` (Excel) or `.csv`.

---

## 📂 Project Structure

```text
├── src/
│   ├── lib.rs          # Core library entry point
│   ├── config.rs       # Extraction parameter builder (ExtractionConfig)
│   ├── presets.rs      # Native bank column coordinate templates
│   ├── parser.rs       # Core Y-coordinate clustering engine
│   ├── models.rs       # Word, Row, and Table structures
│   ├── exporter.rs     # CSV & XLSX exporting utilities
│   ├── error.rs        # Custom library error handling
│   ├── main.rs         # Local web UI server binary
│   └── bin/
│       └── cli.rs      # Command line tool binary
├── static/             # Frontend assets (HTML, style.css, app.js, pdf.js)
├── temp/               # Temporary parsing folder (automatically cleaned up)
└── Cargo.toml          # Cargo package file
```

---

## 📖 Library Usage Example

Add the library to your own Rust application to parse statements programmatically:

```rust
use vaultparser::{ExtractionConfig, BankPreset, extract_from_file};
use vaultparser::exporter::export_to_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load HDFC preset column coordinates
    let config = BankPreset::Hdfc.config();

    // 2. Natively extract tabular data from statement
    let table = extract_from_file("statement.pdf", &config)?;
    println!("Parsed {} transaction rows!", table.rows.len());

    // 3. Export ledger to CSV bytes
    let csv_bytes = export_to_csv(&table)?;
    std::fs::write("statement_ledger.csv", csv_bytes)?;

    Ok(())
}
```

---

## 💻 Running the CLI Tool

You can run the extraction directly from the command line without opening a browser:

```bash
# Print CSV results to stdout:
cargo run --release --bin cli -- <input-pdf> <bank-preset>

# Save CSV results directly to a file:
cargo run --release --bin cli -- <input-pdf> <bank-preset> [output-csv]
```

### Examples:
```bash
# Output HDFC statement directly to stdout
cargo run --release --bin cli -- "hdfc bank.pdf" hdfc

# Save Union Bank statement output to statement.csv
cargo run --release --bin cli -- "statement.pdf" union output.csv
```

---

## 🏃 Running the Web UI

1. Make sure you have the Rust toolchain installed.
2. Start the local server:
   ```bash
   cargo run --release
   ```
3. Open your browser and navigate to:
   **[http://localhost:8000/](http://localhost:8000/)**

---

## 📋 Step-by-Step Usage Guide

### A. Using the Web Dashboard (Interactive Visual Extraction)
1. **Upload Statement**: Drag and drop your PDF bank statement into the upload zone.
2. **Apply Preset**: Select your bank (e.g. *HDFC Bank*) from the **Preset Selector** dropdown. The vertical column guides will automatically overlay on the PDF preview.
3. **Refine Columns**:
   * **Move**: Drag existing vertical red lines to match the statement's columns.
   * **Add**: Double-click anywhere on the PDF canvas to add a new vertical guide.
   * **Remove**: Right-click on any guide to delete it.
4. **Trim Headers & Footers**: Adjust the **Top Trim** and **Bottom Trim** slider percentages to exclude running page headers, summaries, or footers.
5. **Review & Override Data**:
   * **Edit cells**: Double-click any cell in the live preview table to correct typos.
   * **Delete rows**: Click the red `✕` on any row to delete headers or background noise.
   * *Note: Manual edits are locked to PDF coordinates and persist when you switch pages or export!*
6. **Download Ledger**: Click **Export to Excel (.xlsx)** or **Export to CSV** to save the formatted ledger.

### B. Integrating the Rust Library in Your Code
1. **Add Cargo Dependency**:
   Add `vaultparser` to your project's `Cargo.toml`. To include the visual Web UI server and its async framework dependencies, enable the optional `web` feature:
   ```toml
   [dependencies]
   vaultparser = "0.1.4"
   # Or with the optional web UI:
   # vaultparser = { version = "0.1.4", features = ["web"] }
   ```
2. **Define Configuration**: Use the builder pattern or bank presets to create the configuration:
   ```rust
   let config = vaultparser::BankPreset::Hdfc.config();
   ```
3. **Run Extraction**: Pass the PDF file path or byte buffer to the parser:
   ```rust
   let table = vaultparser::extract_from_file("my_statement.pdf", &config)?;
   ```
4. **Save Export**: Output the transaction data directly:
   ```rust
   let csv_data = vaultparser::exporter::export_to_csv(&table)?;
   std::fs::write("statement.csv", csv_data)?;
   ```

---

## 🧪 Testing

To run the built-in unit and integration test suite:
```bash
cargo test -- --nocapture
```
This runs coordinate/amount parsing tests, an extraction check against the sample HDFC statement, and a full integration extraction check on the user's statement `hdfc bank.pdf` to verify native decryption.

---

## 🪵 Logging & Diagnostics

The library utilizes the standard `tracing` crate for diagnostic logging. Both the Web UI and CLI binaries are pre-configured to output formatted logs to stdout.

You can control the log verbosity using the `RUST_LOG` environment variable:

* **Show only start-up and high-level extraction counts (default):**
  ```powershell
  # Windows PowerShell
  $env:RUST_LOG="info"
  cargo run --bin cli -- "hdfc bank.pdf" hdfc
  ```
  ```bash
  # Linux/macOS
  export RUST_LOG=info
  cargo run --bin cli -- "hdfc bank.pdf" hdfc
  ```

* **Show page-by-page word counts and layout details (debug):**
  ```powershell
  $env:RUST_LOG="debug"
  cargo run --bin cli -- "hdfc bank.pdf" hdfc
  ```

---

## 🛡️ License
Distributed under the GNU Affero General Public License v3.0 (AGPL-3.0). Offline-only, secure, and private by design.

