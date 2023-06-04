use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use nanocl_utils::io_error::IoResult;

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
