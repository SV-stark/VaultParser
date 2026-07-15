# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
