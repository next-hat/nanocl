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
  #[tabled(skip)]
  pub state: String,
  #[tabled(skip)]
  pub wanted: Option<String>,
  #[tabled(skip)]
  pub results: Option<String>,
  #[tabled(skip)]
  pub recent_failure: Option<(chrono::NaiveDateTime, String)>,
  #[tabled(skip)]
  pub read_error: Option<String>,
}
