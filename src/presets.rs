use crate::config::ExtractionConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankPreset {
    Hdfc,
    Sbi,
    Canara,
    Union,
    Uco,
}

impl BankPreset {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hdfc => "HDFC Bank India",
            Self::Sbi => "State Bank of India (SBI)",
            Self::Canara => "Canara Bank",
            Self::Union => "Union Bank of India",
            Self::Uco => "UCO Bank",
        }
    }

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
        }
        config
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hdfc" => Some(Self::Hdfc),
            "sbi" => Some(Self::Sbi),
            "canara" => Some(Self::Canara),
            "union" => Some(Self::Union),
            "uco" => Some(Self::Uco),
            _ => None,
        }
    }
}
