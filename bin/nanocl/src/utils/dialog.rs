use dialoguer::theme::ColorfulTheme;
use dialoguer::Confirm;
use nanocl_error::io::IoResult;

/// Ask for confirmation
pub fn confirm(msg: &str) -> IoResult<()> {
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
