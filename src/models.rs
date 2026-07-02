use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordItem {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRow {
    pub id: String,
    pub y: f64,
    pub page: usize,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedTable {
    pub headers: Vec<String>,
    pub active_indices: Vec<usize>,
    pub rows: Vec<PageRow>,
}
