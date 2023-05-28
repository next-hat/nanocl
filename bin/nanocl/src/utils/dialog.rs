use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use nanocl_utils::io_error::IoResult;

pub fn confirm(msg: &str) -> IoResult<()> {
  ctrlc::set_handler(move || {
    let term = dialoguer::console::Term::stdout();
    let _ = term.show_cursor();
    let _ = term.clear_last_lines(1);
  })
  .expect("Error setting Ctrl-C handler");

  let result = Confirm::with_theme(&ColorfulTheme::default())
    .with_prompt(msg)
    .default(false)
    .interact();
  match result {
    Ok(true) => Ok(()),
    _ => Err(
      std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "interupted by user",
      )
      .into(),
    ),
  }
}
