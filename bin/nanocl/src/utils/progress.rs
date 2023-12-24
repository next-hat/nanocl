use indicatif::{ProgressBar, ProgressStyle};

pub(crate) fn create_progress(
  token: &str,
  style: &ProgressStyle,
) -> ProgressBar {
  let pg = ProgressBar::new(1);
  pg.enable_steady_tick(std::time::Duration::from_millis(50));
  pg.set_style(style.clone());
  pg.set_message(token.to_owned());
  pg
}

pub(crate) fn create_spinner_style(color: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!("{{spinner:.{color}}} {{msg}}"))
    .unwrap()
    .tick_strings(&["▹▹▹▹▹", "▸▹▹▹▹", "▹▸▹▹▹", "▹▹▸▹▹", "▹▹▹▸▹", "▹▹▹▹▸", ">"])
}
