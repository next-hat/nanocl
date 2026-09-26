use tabled::Tabled;

/// A bounded runtime overview; declarations and container configuration stay out
/// of the output.
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct StateStatusRow {
  pub kind: String,
  pub name: String,
  pub running: String,
  pub status: String,
  pub health: String,
  #[tabled(rename = "RECENT FAILURE")]
  pub failure: String,
  #[tabled(skip)]
  pub read_failed: bool,
}
