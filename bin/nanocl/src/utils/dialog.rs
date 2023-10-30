use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use nanocl_error::io::IoResult;

/// ## Confirm
///
/// Ask for confirmation
///
/// ## Arguments
///
/// * [msg](str) The message to display
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation is confirmed
///   * [Err](IoError) An error occured
///
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
