/// ## Calculate Percentage
///
/// Calculate the percentage of a number
///
/// ## Arguments
///
/// - [current](u64) The current number
/// - [total](u64) The total number
///
/// ## Returns
///
/// - [u64](u64) The percentage
///
pub fn calculate_percentage(current: u64, total: u64) -> u64 {
  ((current as f64 / total as f64) * 100_f64).round() as u64
}
