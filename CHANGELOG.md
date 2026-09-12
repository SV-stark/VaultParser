# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.7] - 2026-09-12

### Added
- Implemented `std::fmt::Display`, `Hash`, `PartialOrd`, and `Ord` traits for `BankPreset`.
- Implemented `PartialEq` and `#[serde(default)]` on all optional fields for `ExtractionConfig`.
- Implemented `std::error::Error::source` on `ExtractorError` to preserve nested I/O error source chains.
- Added release profile optimizations (`lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`) in `Cargo.toml`.

### Changed
- Refactored `parse_amount` to perform single-pass zero-redundant-allocation string parsing, replacing repeated `.replace()` allocations.
- Optimized word extraction and row clustering in `parser.rs` to consume words and grouped rows by value via `into_iter()`, eliminating unnecessary word cloning.
- Pre-calculated summary totals in exporters in a single $O(\text{Rows})$ pass, eliminating nested $O(\text{Cols} \times \text{Rows})$ re-parsing loops.
- Deduplicated CSV and TSV exporter logic into a shared `export_delimited` function.
- Replaced partial float comparisons with `f64::total_cmp` for reliable, NaN-safe sorting across column guides and coordinate clustering.
- Replaced `.parse().unwrap()` runtime MIME string parsing with compile-time verified `HeaderValue::from_static`.
- Replaced `std::process::exit(1)` in CLI subroutines and unwrapped panics in server `main()` with graceful error propagation.

## [0.3.6] - 2026-09-03

### Added
- Implemented `std::str::FromStr` for `BankPreset`.
- Configurable server address via `HOST` and `PORT` environment variables in web server.
- Added comprehensive validation for `ExtractionConfig` covering date order (`from_date` <= `to_date`), column guide boundaries/ordering, and `y_tolerance`.
- Added trailing negative sign handling (e.g. `1,234.50-`) and currency-wrapped negative parenthesized numbers in `parse_amount`.

### Fixed
- Fixed column totals in exporters (CSV, XLSX, TSV) to exclude `BALANCE` column from being aggregated into total transaction volume.
- Fixed summary row indexing and width calculation in XLSX exporter.
- Added RAII `TempFileGuard` to guarantee automatic temporary file cleanup on drop during PDF detection.
- Fixed boundary-check safety in line continuation merging and categorization indexing.
- Fixed TSV export button parameters and crop percentages in the web UI.

## [0.3.5] - 2026-08-22

### Security
- Server binds to loopback (`127.0.0.1:8000`) rather than `0.0.0.0:8000` to prevent unintended LAN exposure.
- Added Axum `DefaultBodyLimit` (50 MB) to guard against unbounded multipart memory usage.
- Stored decrypted statements and temporary parsing files in OS temporary directory (`std::env::temp_dir()`) with unique process ID and timestamp suffixes, eliminating disk pollution beside source files and preventing concurrent collision.
- Offloaded CPU-bound PDF extraction and layout parsing to `tokio::task::spawn_blocking` to prevent reactor thread starvation.

### Fixed
- Fixed manual cell edits and row deletions drift by using stable, deterministic index-based row IDs (`row-{page}-{idx}`) with backwards-compatible coordinate fallback.
- Enhanced amount recognition (`is_possible_amount` and `parse_amount`) to properly parse accounting negative numbers formatted in parentheses like `(1,234.50)` as negative floats.
- Expanded date recognition (`is_possible_date` and `standardize_date`) to support full month names (e.g. "30 September 2025") and short date strings (`1/1/25`).
- Fixed error classification so PDF structural corruption / Xref errors are correctly surfaced as `PdfOpenError` rather than misattributed as `PasswordError`.
- Cleaned up redundant manual file deletion in `detect_column_guides`.

### Added
- Added `/api/presets` HTTP endpoint serving native bank statement presets dynamically.
- Added missing column mapping dropdown choices (`value_date`, `chq_no`, `s_no`) and preset definitions (`axis`, `bob`, `yes`, `idfc`, `indusind`) in Web UI.
- Upgraded CI workflow with `dtolnay/rust-toolchain@stable`, automated `cargo test --all-features`, and strict Clippy lint checks.
- Cleaned up `.oxlintrc.json` for vanilla JavaScript.
- Added unit tests covering accounting numbers, extended date formats, and export formatting.

## [0.3.4] - 2026-08-06

### Added
- Added native Bank Statement Preset for **Himachal Pradesh Gramin Bank** (`hpgb`) with fine-tuned column boundaries (`date`, `reference`, `description`, `debit`, `credit`, `balance`).
- Added automatic detection for Himachal Pradesh Gramin Bank statements in PDF analyzer.

## [0.3.2] - 2026-07-30

### Added
- Added automated **Summary & Totals Row** in `.xlsx` (Excel), CSV, and TSV exporters calculating total debits, credits, amounts, and row count.
- Added **Transaction Categorization & Tagging Engine** with `--categorize / -c` CLI flag and `config.categorize` setting (defaults off). Tags transactions into `UPI & Transfers`, `Salary & Income`, `Interest & Dividends`, `Investments`, `ATM & Cash`, `Shopping & Merchants`, `Food & Dining`, `Bills & Utilities`, `Bank Fees & Tax`, and `Suspense` (for unmatched transactions).
- Added **Date Range Filtering** via `--from <YYYY-MM-DD>` and `--to <YYYY-MM-DD>` CLI flags and `from_date` / `to_date` config builder methods.

## [0.3.1] - 2026-07-30

### Changed
- Upgraded `lopdf` to `0.44` and `rust_xlsxwriter` to `0.97`.

## [0.3.0] - 2026-07-30

### Added
- Added TSV (Tab-Separated Values) format export support via `exporter::export_to_tsv`.
- Added **Copy TSV to Clipboard** feature in Web UI dashboard for fast copy-pasting directly into Excel / Google Sheets.
- Added explicit `--format / -f` CLI flag to `vpcli` supporting `csv`, `tsv`, `xlsx`, and `json`.
- Added batch directory processing in `vpcli` to process folders of PDF bank statements in a single command.
- Added 5 new native bank statement presets: Axis Bank (`axis`), Bank of Baroda (`bob`), YES Bank (`yes`), IDFC FIRST Bank (`idfc`), and IndusInd Bank (`indusind`).

### Changed
- Updated dependencies in `Cargo.lock` to latest semver-compatible versions.

## [0.2.5] - 2026-07-22

### Added
- Native `/api/detect` HTTP endpoint in Axum web server to auto-detect bank presets and column guide boundaries natively in Rust for uploaded statements (including encrypted PDFs).
- Integrated native `/api/detect` endpoint call in web dashboard `app.js`.

### Fixed
- Fixed row Y-coordinate clustering centroid drift in parser engine by using fixed anchor Y coordinates during assignment.
- Enhanced `is_possible_amount` to handle Indian Rupee symbols (`₹`, `Rs.`, `Rs`, `INR`) and credit/debit indicators (`Cr`, `Dr`, `CR`, `DR`).
- Expanded `is_possible_date` and `standardize_date` to parse timestamped dates (e.g., `30/04/2025 10:15:30`), month-name formats, and single-token fallback values.

## [0.2.4] - 2026-07-15

### Changed
- Updated dependencies to latest semver-compatible versions (notably `pdfsink-rs` 0.2.9 and `tokio` 1.52.3) and removed the yanked `spin` 0.9.8.

## [0.2.3] - 2026-07-15

### Added
- Doctests for the `ExtractionConfig` builder, `BankPreset` helpers, and CSV/Excel/JSON exporters.

## [0.2.2] - 2026-07-06

### Added
- Interactive CLI Wizard mode when executing `vpcli` with no arguments.
- Exporter support for structured JSON representation of extracted transaction ledgers.
- Support for parsing statements with custom presets loaded dynamically from JSON config files.

## [0.2.1] - 2026-07-06

### Fixed
- Configured CLI to show the help menu by default when executed without arguments (`arg_required_else_help = true`).

## [0.2.0] - 2026-07-06

### Added
- Support for saving Excel files (`.xlsx`) directly via the CLI by specifying a `.xlsx` output path.
- Standardized all exported date columns to `DD-MM-YYYY` format.
- Renamed the generic CLI binary from `cli` to a unique command name `vpcli`.

### Changed
- Upgraded dependencies to their latest compatible versions:
  - `rust_xlsxwriter` to `0.96`
  - `lopdf` to `0.43`
  - `indicatif` to `0.18`

## [0.1.7] - 2026-07-06

### Added
- Support for saving Excel files (`.xlsx`) via CLI when the output path ends with `.xlsx`.
- Standardized date exports to `DD-MM-YYYY` format.

## [0.1.6] - 2026-07-03

### Added
- Configuration validation inside `ExtractionConfigBuilder` and `ExtractionConfig` to prevent mismatched mappings, out-of-bounds crop guides, or invalid trim bounds.
- Custom structured error variants (`InvalidConfig`, `PasswordError`) to `ExtractorError` for better API error classification.
- Added comprehensive unit testing for builder validation rules.

### Fixed
- Fixed a decrypted PDF temp-file resource leak on early exit or panic inside extraction, preset detection, and layout analysis by introducing a drop-guard `TempFileGuard`.
- Fixed a concurrency race condition in test suites/parallel runs by appending unique timestamps to decrypted temporary filenames.
- Cleaned up CLI version tracking to pull version details automatically from `Cargo.toml`.

## [0.1.5] - 2026-07-02

### Added
- Integrated `clap` crate for structured command-line argument parsing.
- Integrated `chrono` date parsing to standardize date and value-date columns to ISO `YYYY-MM-DD` format.
- Integrated `indicatif` spinner indicators for interactive visual feedback in the CLI tool.
- Added a dedicated unit test suite for verifying date standardization.

### Fixed
- Fixed CLI panic when arguments are missing or invalid by returning usage hints cleanly.

## [0.1.4] - 2026-07-02

### Added
- Discovered and integrated real-world PNB and Kotak Mahindra bank statements from OneDrive local backup.
- Added two new built-in bank templates to `BankPreset`:
  - **Punjab National Bank (PNB)** (`pnb`): Tailored for standard PNB statement configurations.
  - **Kotak Mahindra Bank** (`kotak`): Formatted for Kotak's single-column transaction amount layouts with correct date guide bounds (`0.15`) to prevent word overlapping.
- Updated local web UI (`index.html` and `app.js`) to support selecting and auto-detecting PNB and Kotak statements.

### Fixed
- Refined the date validation heuristic (`is_possible_date` in `src/parser.rs`) to reject arbitrary strings like long addresses containing digits by imposing a limit of at most 4 alphabetic characters per date cell.

## [0.1.3] - 2026-07-02

### Added
- Discovered and integrated real-world bank statements from OneDrive local backup to calibrate coordinates.
- Added three new built-in bank templates to `BankPreset`:
  - **Indian Bank** (`indian`): Tailored for standard Allahabad/Indian Bank statement formats with multi-line grouping (`y_tolerance = 15.0`).
  - **H P State Co-operative Bank** (`hpscb`): Custom layout for HPSCB landscape A4 and detailed transaction items (`y_tolerance = 15.0`).
  - **ICICI Bank** (`icici`): Accurate coordinates and guides for standard ICICI transaction registers.
- Updated the local interactive web UI (`index.html` and `app.js`) to support selecting and auto-detecting Indian Bank, HPSCB, and ICICI Bank statements.

## [0.1.2] - 2026-07-02

### Added
- Feature flags in `Cargo.toml`: introduced the `web` feature to make web dependencies (`axum`, `tokio`, `tower-http`, `serde_json`) optional and keep library compilation lean.
- README status badges for crates.io version, docs.rs, GitHub Actions release build status, and license.

### Changed
- Configured GitHub Actions build workflow to compile the web binary with the `--features web` flag.

## [0.1.1] - 2026-07-02

### Added
- Comprehensive Rust doc comments (`///`) for all public API items (`ExtractionConfig`, `ExtractionConfigBuilder`, `BankPreset`, `ExtractorError`, `WordItem`, `PageRow`, `ExtractedTable`, `export_to_csv`, `export_to_xlsx`) to support crates.io and docs.rs.
- GitHub Actions CI/CD workflow (`release.yml`) to automatically build, package, and attach binaries (Windows, Linux, macOS) to GitHub Releases upon tag pushes.

### Changed
- Configured package `exclude` rules in `Cargo.toml` to prevent bundling bank statement PDFs, output CSVs, temp files, and debug logs into crates.io packages.
- Updated examples in `README.md` to reference a generic `"hdfc bank.pdf"` instead of user-specific filenames.
- Formatted the codebase using `cargo fmt`.

---

## [0.1.0] - 2026-07-02

### Added
- Initial release of VaultParser library and command-line extractor.
- High-performance native Rust PDF parsing with empty-password decryption support.
- Builder pattern for custom extraction config (`ExtractionConfigBuilder`).
- Presets for popular Indian banks: HDFC, State Bank of India (SBI), Canara Bank, Union Bank of India, and UCO Bank.
- Multi-format exporters (CSV and Excel `.xlsx` via `rust_xlsxwriter`).
- Embedded interactive local web interface to visually customize column delimiters and trimming rules.

### Fixed
- Fixed State Bank of India (SBI) preset's `y_tolerance` (increased from `6.0` to `15.0`) to correctly cluster and extract transactions split across multiple lines.

### Changed
- Relicensed the project under the GNU Affero General Public License v3.0 (`AGPL-3.0-only`).
