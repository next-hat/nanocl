/// Calculate the percentage of a number
pub fn calculate_percentage(current: u64, total: u64) -> u64 {
  ((current as f64 / total as f64) * 100_f64).round() as u64
}
