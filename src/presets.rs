use crate::config::ExtractionConfig;

/// Supported out-of-the-box bank statement templates.
///
/// Each preset contains standard column coordinates and label mappings
/// for common Indian banks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankPreset {
    /// HDFC Bank India statement template.
    Hdfc,
    /// State Bank of India (SBI) statement template.
    Sbi,
    /// Canara Bank statement template.
    Canara,
    /// Union Bank of India statement template.
    Union,
    /// UCO Bank statement template.
    Uco,
    /// Indian Bank statement template.
    Indian,
    /// H P State Co-operative Bank statement template.
    Hpscb,
    /// ICICI Bank statement template.
    Icici,
}

impl BankPreset {
    /// Returns the human-readable display name of the bank preset.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hdfc => "HDFC Bank India",
            Self::Sbi => "State Bank of India (SBI)",
            Self::Canara => "Canara Bank",
            Self::Union => "Union Bank of India",
            Self::Uco => "UCO Bank",
            Self::Indian => "Indian Bank",
            Self::Hpscb => "H P State Co-operative Bank",
            Self::Icici => "ICICI Bank",
        }
    }

    /// Returns the pre-configured [`ExtractionConfig`] for the specific bank.
    pub fn config(&self) -> ExtractionConfig {
        let mut config = ExtractionConfig::default();
        match self {
            Self::Hdfc => {
                config.col_guides = vec![0.11, 0.42, 0.52, 0.62, 0.75, 0.88];
                config.col_mappings = vec![
                    "date".to_string(),
                    "description".to_string(),
                    "reference".to_string(),
                    "skip".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
            }
            Self::Sbi => {
                config.col_guides = vec![0.10, 0.20, 0.52, 0.64, 0.76, 0.88];
                config.col_mappings = vec![
                    "date".to_string(),
                    "skip".to_string(),
                    "description".to_string(),
                    "reference".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
                config.y_tolerance = 15.0;
            }
            Self::Canara => {
                config.col_guides = vec![0.12, 0.48, 0.60, 0.74, 0.87];
                config.col_mappings = vec![
                    "date".to_string(),
                    "description".to_string(),
                    "reference".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
            }
            Self::Union => {
                config.col_guides = vec![0.11, 0.42, 0.52, 0.62, 0.75, 0.88];
                config.col_mappings = vec![
                    "date".to_string(),
                    "description".to_string(),
                    "reference".to_string(),
                    "skip".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
            }
            Self::Uco => {
                config.col_guides = vec![0.12, 0.24, 0.60, 0.74, 0.87];
                config.col_mappings = vec![
                    "date".to_string(),
                    "reference".to_string(),
                    "description".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
            }
            Self::Indian => {
                config.col_guides = vec![0.08, 0.18, 0.45, 0.58, 0.71, 0.80];
                config.col_mappings = vec![
                    "date".to_string(),
                    "value_date".to_string(),
                    "description".to_string(),
                    "chq_no".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
                config.y_tolerance = 15.0;
            }
            Self::Hpscb => {
                config.col_guides = vec![0.08, 0.16, 0.38, 0.41, 0.65, 0.74, 0.82];
                config.col_mappings = vec![
                    "s_no".to_string(),
                    "date".to_string(),
                    "value_date".to_string(),
                    "chq_no".to_string(),
                    "description".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
                config.y_tolerance = 15.0;
            }
            Self::Icici => {
                config.col_guides = vec![0.08, 0.44, 0.55, 0.62, 0.88];
                config.col_mappings = vec![
                    "date".to_string(),
                    "description".to_string(),
                    "chq_no".to_string(),
                    "debit".to_string(),
                    "credit".to_string(),
                    "balance".to_string(),
                ];
                config.filter_only_date = true;
                config.y_tolerance = 12.0;
            }
        }
        config
    }

    /// Attempts to parse a case-insensitive string into a [`BankPreset`].
    /// Returns `None` if the name is unrecognized.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hdfc" => Some(Self::Hdfc),
            "sbi" => Some(Self::Sbi),
            "canara" => Some(Self::Canara),
            "union" => Some(Self::Union),
            "uco" => Some(Self::Uco),
            "indian" => Some(Self::Indian),
            "hpscb" => Some(Self::Hpscb),
            "icici" => Some(Self::Icici),
            _ => None,
        }
    }
}
