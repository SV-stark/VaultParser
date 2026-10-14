use vaultparser::{BankPreset, extract_from_file};
fn main() {
    let config = BankPreset::Ujjivan.config();
    let table = extract_from_file("shoghi.pdf", &config).unwrap();
    let mut counts = std::collections::HashMap::new();
    for r in &table.rows { *counts.entry(r.page).or_insert(0usize) += 1; }
    for p in 1usize..=89 { println!("page {}: {}", p, counts.get(&p).copied().unwrap_or(0)); }
    println!("total {}", table.rows.len());
}