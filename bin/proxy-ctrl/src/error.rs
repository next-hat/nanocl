pub(crate) enum ErrorHint {
  Warning(String),
  Error(String),
}

impl std::fmt::Display for ErrorHint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ErrorHint::Warning(msg) => write!(f, "{msg}"),
      ErrorHint::Error(msg) => write!(f, "{msg}"),
    }
  }
}

impl ErrorHint {
  pub(crate) fn warning(msg: String) -> ErrorHint {
    ErrorHint::Warning(msg)
  }

  pub(crate) fn error(msg: String) -> ErrorHint {
    ErrorHint::Error(msg)
  }

  pub(crate) fn print(&self) {
    match self {
      ErrorHint::Warning(msg) => log::warn!("{msg}"),
      ErrorHint::Error(msg) => log::error!("{msg}"),
    }
  }
}

#[cfg(test)]
mod tests {

  use super::*;

  use crate::utils::tests::*;

  #[test]
  fn error_hint_basic() {
    before();
    let hint = ErrorHint::warning("This is a warning".to_string());
    println!("{hint}");
    hint.print();
    assert_eq!(hint.to_string(), "This is a warning");
    let hint = ErrorHint::error("This is an error".to_string());
    println!("{hint}");
    assert_eq!(hint.to_string(), "This is an error");
    hint.print();
  }
}
