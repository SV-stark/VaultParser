use vaultparser::{BankPreset, extract_from_file};
fn main() {
    let config = BankPreset::Ujjivan.config();
    let table = extract_from_file("shoghi.pdf", &config).unwrap();
    for r in &table.rows {
        if r.page == 2 {
            println!("y={:.1} cells={:?}", r.y, r.cells);
        }
    }
}