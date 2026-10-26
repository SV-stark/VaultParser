pub mod config;
pub mod error;
pub mod exporter;
pub mod models;
pub mod parser;
pub mod presets;

pub use config::{ExtractionConfig, ExtractionConfigBuilder};
pub use error::ExtractorError;
pub use exporter::{
    export_to_csv, export_to_json, export_to_qif, export_to_tsv, export_to_xlsx,
    reconcile_running_balance,
};
pub use models::{BalanceDiscrepancy, BalanceReconciliation, ExtractedTable, PageRow, WordItem};
pub use parser::{
    detect_column_guides, detect_column_guides_from_bytes, detect_column_mappings,
    detect_column_mappings_from_bytes, detect_preset_from_bytes, detect_preset_from_file,
    extract_from_bytes, extract_from_file,
};
pub use presets::BankPreset;
