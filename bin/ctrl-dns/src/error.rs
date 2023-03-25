/// Error hint in case of an error
pub enum ErrorHint {
  /// Critical process exited
  Error(String),
  /// Warning just for information
  Warning(String),
}

/// Display the ErrorHint
impl std::fmt::Display for ErrorHint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ErrorHint::Error(msg) => write!(f, "[ERROR]: {msg}"),
      ErrorHint::Warning(msg) => write!(f, "[WARNING]: {msg}"),
    }
  }
}
