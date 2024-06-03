use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

pub(crate) fn create_progress(msg: &str, style: &ProgressStyle) -> ProgressBar {
  let pg = ProgressBar::new(1);
  pg.enable_steady_tick(std::time::Duration::from_millis(50));
  pg.set_style(style.clone());
  pg.set_message(msg.to_owned());
  pg
}

pub(crate) fn create_spinner_style(key: &str, color: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!(
    "{{spinner:.{color}.bold}} {} {{msg}}",
    key.bold()
  ))
  .unwrap()
  .tick_strings(&[
    "▹▹▹▹▹",
    "▸▹▹▹▹",
    "▹▸▹▹▹",
    "▹▹▸▹▹",
    "▹▹▹▸▹",
    "▹▹▹▹▸",
    ">",
  ])
}
