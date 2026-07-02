# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
